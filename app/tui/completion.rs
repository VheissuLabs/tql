use std::sync::LazyLock;

use crate::terminal::prelude::*;
use regex::Regex;

use super::chrome::theme;
use super::palette;
use super::sql::KEYWORDS;
use super::statements;
use super::text::{pad, truncate, width, window};

pub const LIMIT: usize = 50;
const ROWS: usize = 8;
const WIDEST: usize = 40;
const CLOSEST_SCORE_SHOWN: usize = 2;
const KEYWORDS_BEFORE_A_TABLE: [&str; 5] = ["from", "join", "into", "update", "table"];
const IDENTIFIER: &str = r#"(?:"[^"]+"|`[^`]+`|\[[^\]]+\]|[A-Za-z_][\w$]*)"#;

fn pattern(source: &str) -> Regex {
    Regex::new(source).expect("the completion patterns are valid")
}

static PREFIX: LazyLock<Regex> = LazyLock::new(|| pattern(r"[\w$]*\z"));
static QUALIFIED: LazyLock<Regex> = LazyLock::new(|| pattern(&format!(r"({IDENTIFIER})\.\z")));
static PREVIOUS: LazyLock<Regex> = LazyLock::new(|| pattern(r"([\w$]+)\W*\z"));
static REFERENCED: LazyLock<Regex> = LazyLock::new(|| {
    pattern(&format!(
        r"(?i)\b(?:from|join|update|into)\s+((?:{IDENTIFIER}\.)?{IDENTIFIER})(?:\s+(?:as\s+)?({IDENTIFIER}))?"
    ))
});
static QUOTED: LazyLock<Regex> = LazyLock::new(|| pattern(r#"'(?:''|[^'])*'|"[^"]*"|`[^`]*`"#));
static PLAIN_NAME: LazyLock<Regex> = LazyLock::new(|| pattern(r"^[a-z_][a-z0-9_]*$"));
static WRAPPED: LazyLock<Regex> = LazyLock::new(|| pattern(r#"(?s)^["`\[](.*)["`\]]$"#));

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Column,
    Table,
    Keyword,
}

impl Kind {
    fn label(self) -> &'static str {
        match self {
            Kind::Column => "col",
            Kind::Table => "table",
            Kind::Keyword => "sql",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Candidate {
    pub text: String,
    pub label: String,
    pub kind: Kind,
}

pub trait Lookup {
    fn columns(&mut self, table: &str) -> Vec<String>;

    fn quote(&self, name: &str) -> String;
}

pub struct Schema<'a> {
    pub tables: &'a [String],
    pub open: Option<&'a str>,
    pub lookup: &'a mut dyn Lookup,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Completion {
    pub items: Vec<Candidate>,
    pub start: usize,
    pub end: usize,
    pub index: usize,
}

impl Completion {
    pub fn at(buffer: &str, cursor: usize, schema: Schema) -> Option<Self> {
        let before: String = buffer.chars().take(cursor).collect();
        let last_line = before.rsplit('\n').next().unwrap_or_default();
        if in_string_or_comment(last_line) {
            return None;
        }

        let prefix = PREFIX
            .find(&before)
            .map(|found| found.as_str())
            .unwrap_or_default();
        let start = cursor - prefix.chars().count();
        let ahead = &before[..before.len() - prefix.len()];
        if prefix.starts_with(|character: char| character.is_ascii_digit()) {
            return None;
        }

        let statement = statements::at(buffer, cursor).map(|statement| statement.sql);
        let referenced = referenced(statement.as_deref().unwrap_or(buffer), schema.tables);

        if let Some(qualified) = QUALIFIED.captures(ahead) {
            let qualifier = unquote(&qualified[1]).to_lowercase();
            let table = referenced
                .iter()
                .find(|(name, _)| *name == qualifier)
                .map(|(_, table)| table.clone())
                .or_else(|| known(&qualifier, schema.tables));
            let candidates = table
                .map(|table| named(&schema.lookup.columns(&table), Kind::Column, schema.lookup))
                .unwrap_or_default();
            return narrowed(candidates, prefix, start, cursor, true);
        }

        if prefix.is_empty() {
            return None;
        }

        let previous = PREVIOUS
            .captures(ahead)
            .map(|captures| captures[1].to_lowercase())
            .unwrap_or_default();
        if previous == "as" {
            return None;
        }
        if KEYWORDS_BEFORE_A_TABLE.contains(&previous.as_str()) {
            return narrowed(
                named(schema.tables, Kind::Table, schema.lookup),
                prefix,
                start,
                cursor,
                false,
            );
        }

        let mut sources: Vec<String> = Vec::new();
        for (_, table) in &referenced {
            if !sources.contains(table) {
                sources.push(table.clone());
            }
        }
        if sources.is_empty() {
            sources.extend(schema.open.map(String::from));
        }
        let mut columns: Vec<String> = Vec::new();
        for table in &sources {
            for column in schema.lookup.columns(table) {
                if !columns.contains(&column) {
                    columns.push(column);
                }
            }
        }

        let candidates = [
            named(&columns, Kind::Column, schema.lookup),
            named(schema.tables, Kind::Table, schema.lookup),
            keywords(prefix),
        ]
        .concat();
        narrowed(candidates, prefix, start, cursor, false)
    }

    pub fn move_by(&mut self, forward: bool) {
        let count = self.items.len();
        if count == 0 {
            return;
        }
        self.index = if forward {
            (self.index + 1) % count
        } else {
            (self.index + count - 1) % count
        };
    }

    pub fn selected(&self) -> Option<&Candidate> {
        self.items.get(self.index)
    }

    pub fn rows(&self) -> u16 {
        (self.items.len().min(ROWS) + 2) as u16
    }

    pub fn columns(&self) -> u16 {
        let longest = self
            .items
            .iter()
            .map(|item| width(&item.label))
            .max()
            .unwrap_or_default();
        (longest + 10).min(WIDEST) as u16
    }

    pub fn content(&self, inner_width: usize, inner_height: usize) -> Vec<Line<'static>> {
        let start = window(self.index, self.items.len(), inner_height);
        self.items
            .iter()
            .enumerate()
            .skip(start)
            .take(inner_height)
            .map(|(position, item)| {
                let kind = item.kind.label();
                let label = pad(
                    &format!(
                        " {}",
                        truncate(&item.label, inner_width.saturating_sub(kind.len() + 3))
                    ),
                    inner_width.saturating_sub(kind.len() + 1),
                );
                if position == self.index {
                    return Line::styled(format!("{label}{kind} "), theme::selection());
                }
                Line::from(vec![
                    Span::raw(label),
                    Span::styled(kind, theme::dim()),
                    Span::raw(" "),
                ])
            })
            .collect()
    }
}

fn narrowed(
    candidates: Vec<Candidate>,
    prefix: &str,
    start: usize,
    end: usize,
    allow_empty: bool,
) -> Option<Completion> {
    let needle = prefix.to_lowercase();
    if needle.is_empty() && !allow_empty {
        return None;
    }

    let mut ranked: Vec<(usize, usize, Candidate)> = candidates
        .into_iter()
        .enumerate()
        .filter_map(|(order, candidate)| {
            let label = candidate.label.to_lowercase();
            if !needle.is_empty() && label == needle {
                return None;
            }
            let score = if needle.is_empty() {
                Some(1)
            } else {
                palette::score(&label, &needle)
            };
            score
                .filter(|score| *score <= CLOSEST_SCORE_SHOWN)
                .map(|score| (score, order, candidate))
        })
        .collect();
    if ranked.is_empty() {
        return None;
    }

    ranked.sort_by_key(|(score, order, _)| (*score, *order));
    Some(Completion {
        items: ranked
            .into_iter()
            .take(LIMIT)
            .map(|(_, _, candidate)| candidate)
            .collect(),
        start,
        end,
        index: 0,
    })
}

fn referenced(sql: &str, tables: &[String]) -> Vec<(String, String)> {
    let mut found: Vec<(String, String)> = Vec::new();
    let mut remember =
        |name: String, table: &String| match found.iter_mut().find(|(known, _)| *known == name) {
            Some(entry) => entry.1 = table.clone(),
            None => found.push((name, table.clone())),
        };

    for captures in REFERENCED.captures_iter(sql) {
        let last_part = captures[1]
            .rsplit('.')
            .next()
            .unwrap_or_default()
            .to_string();
        let Some(table) = known(&unquote(&last_part), tables) else {
            continue;
        };
        remember(table.to_lowercase(), &table);

        let alias = captures
            .get(2)
            .map(|alias| unquote(alias.as_str()))
            .unwrap_or_default();
        if !alias.is_empty() && !KEYWORDS.contains(&alias.to_lowercase().as_str()) {
            remember(alias.to_lowercase(), &table);
        }
    }
    found
}

fn known(name: &str, tables: &[String]) -> Option<String> {
    tables
        .iter()
        .find(|table| table.to_lowercase() == name.to_lowercase())
        .cloned()
}

fn named(names: &[String], kind: Kind, lookup: &dyn Lookup) -> Vec<Candidate> {
    names
        .iter()
        .map(|name| Candidate {
            text: if needs_quoting(name) {
                lookup.quote(name)
            } else {
                name.clone()
            },
            label: name.clone(),
            kind,
        })
        .collect()
}

fn needs_quoting(name: &str) -> bool {
    !PLAIN_NAME.is_match(name) || KEYWORDS.contains(&name)
}

fn keywords(prefix: &str) -> Vec<Candidate> {
    let shouting =
        prefix.chars().any(|character| character.is_uppercase()) && prefix.to_uppercase() == prefix;
    let mut sorted: Vec<&str> = KEYWORDS.to_vec();
    sorted.sort();
    sorted.dedup();
    sorted
        .into_iter()
        .map(|keyword| Candidate {
            text: if shouting {
                keyword.to_uppercase()
            } else {
                keyword.to_string()
            },
            label: keyword.to_string(),
            kind: Kind::Keyword,
        })
        .collect()
}

fn unquote(name: &str) -> String {
    WRAPPED
        .captures(name)
        .map(|captures| captures[1].to_string())
        .unwrap_or_else(|| name.to_string())
}

fn in_string_or_comment(line: &str) -> bool {
    let outside = QUOTED.replace_all(line, "");
    ['\'', '"', '`']
        .iter()
        .any(|quote| outside.contains(*quote))
        || outside.contains("--")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tables() -> Vec<String> {
        ["customers", "orders", "Order Lines"]
            .map(String::from)
            .to_vec()
    }

    struct Shop;

    impl Lookup for Shop {
        fn columns(&mut self, table: &str) -> Vec<String> {
            let names: &[&str] = match table {
                "customers" => &["id", "name", "email"],
                "orders" => &["id", "customer_id", "total"],
                _ => &[],
            };
            names.iter().map(|name| name.to_string()).collect()
        }

        fn quote(&self, name: &str) -> String {
            format!("\"{name}\"")
        }
    }

    fn complete(marked: &str) -> Option<Completion> {
        let cursor = marked
            .chars()
            .position(|character| character == '|')
            .unwrap();
        Completion::at(
            &marked.replace('|', ""),
            cursor,
            Schema {
                tables: &tables(),
                open: Some("customers"),
                lookup: &mut Shop,
            },
        )
    }

    fn texts(marked: &str) -> Vec<String> {
        complete(marked)
            .map(|completion| completion.items.into_iter().map(|item| item.text).collect())
            .unwrap_or_default()
    }

    #[test]
    fn offers_tables_after_from_and_quotes_names_that_need_it() {
        assert_eq!(texts("select * from cu|"), ["customers"]);
        assert_eq!(texts("select * from or|"), ["orders", "\"Order Lines\""]);
    }

    #[test]
    fn offers_columns_of_the_tables_in_the_statement_then_tables_then_keywords() {
        assert_eq!(texts("select na| from customers"), ["name"]);
        assert_eq!(texts("select o.to| from orders o"), ["total"]);
        assert_eq!(
            texts("select o.| from orders o"),
            ["id", "customer_id", "total"]
        );
        assert_eq!(texts("SEL|"), ["SELECT"]);
        assert_eq!(texts("em|"), ["email"]);
    }

    #[test]
    fn stays_quiet_in_strings_comments_numbers_and_aliases() {
        assert!(complete("select 'cu|").is_none());
        assert!(complete("select 1 -- cu|").is_none());
        assert!(complete("select 12|").is_none());
        assert!(complete("select id as na|").is_none());
        assert!(complete("select |").is_none());
    }

    #[test]
    fn records_where_the_word_being_completed_starts() {
        let completion = complete("select * from cu|").unwrap();
        assert_eq!((completion.start, completion.end), (14, 16));
    }
}
