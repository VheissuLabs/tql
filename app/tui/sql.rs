use std::sync::LazyLock;

use regex::Regex;
use zerorust_tui::prelude::*;

use super::chrome::theme;

const KEYWORDS: &[&str] = &[
    "select",
    "from",
    "where",
    "and",
    "or",
    "not",
    "null",
    "is",
    "in",
    "like",
    "ilike",
    "between",
    "order",
    "by",
    "group",
    "having",
    "limit",
    "offset",
    "asc",
    "desc",
    "insert",
    "into",
    "values",
    "update",
    "set",
    "delete",
    "truncate",
    "join",
    "left",
    "right",
    "inner",
    "outer",
    "full",
    "cross",
    "on",
    "using",
    "as",
    "distinct",
    "union",
    "all",
    "with",
    "case",
    "when",
    "then",
    "else",
    "end",
    "exists",
    "create",
    "table",
    "alter",
    "drop",
    "index",
    "view",
    "primary",
    "key",
    "foreign",
    "references",
    "default",
    "unique",
    "constraint",
    "cascade",
    "show",
    "explain",
    "describe",
    "pragma",
    "begin",
    "commit",
    "rollback",
    "true",
    "false",
    "count",
    "sum",
    "avg",
    "min",
    "max",
    "coalesce",
    "cast",
];

static TOKEN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(&format!(
        r#"(?i)(--[^\n]*)|(/\*.*?\*/)|('(?:''|[^'])*')|("(?:[^"]|"")*"|`[^`]*`|\[[^\]]*\])|\b({})\b|(-?\d+(?:\.\d+)?)|([=<>!+\-*/%,;()])"#,
        KEYWORDS.join("|")
    ))
    .expect("the SQL token pattern is valid")
});

static LIMIT: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)\blimit \d+").expect("the limit pattern is valid"));

const KINDS: [&str; 7] = [
    "comment",
    "comment",
    "string",
    "identifier",
    "keyword",
    "number",
    "operator",
];

pub fn tokenise(line: &str) -> Vec<(&'static str, String)> {
    let mut tokens = Vec::new();
    let mut offset = 0;

    for captures in TOKEN.captures_iter(line) {
        let whole = captures.get(0).expect("a match has a whole");
        if whole.start() > offset {
            tokens.push(("plain", line[offset..whole.start()].to_string()));
        }
        let kind = (1..=KINDS.len())
            .find(|group| {
                captures
                    .get(*group)
                    .is_some_and(|found| !found.as_str().is_empty())
            })
            .map(|group| KINDS[group - 1])
            .unwrap_or("plain");
        tokens.push((kind, whole.as_str().to_string()));
        offset = whole.end();
    }

    if offset < line.len() {
        tokens.push(("plain", line[offset..].to_string()));
    }
    tokens
}

pub fn shown(statement: &str, page: usize) -> String {
    LIMIT
        .replace_all(statement, format!("limit {page}"))
        .into_owned()
}

fn wrap(characters: &[(&'static str, char)], width: usize) -> Vec<usize> {
    let mut starts = vec![0];
    let mut at = 0;
    while width > 0 && characters.len() - at > width {
        let space = characters[at..at + width]
            .iter()
            .rposition(|(_, character)| *character == ' ');
        at += match space {
            None | Some(0) => width,
            Some(space) => space + 1,
        };
        starts.push(at);
    }
    starts
}

pub fn lines(statement: &str, inner_width: usize, inner_height: usize) -> Vec<Line<'static>> {
    let mut rows = Vec::new();

    for line in statement.split('\n') {
        let characters: Vec<(&'static str, char)> = tokenise(line)
            .into_iter()
            .flat_map(|(kind, text)| {
                text.chars()
                    .map(move |character| (kind, character))
                    .collect::<Vec<_>>()
            })
            .collect();
        let starts = wrap(&characters, inner_width);

        for (index, from) in starts.iter().enumerate() {
            let to = starts.get(index + 1).copied().unwrap_or(characters.len());
            let mut spans: Vec<Span<'static>> = Vec::new();
            let mut run = String::new();
            let mut kind = "plain";

            for (character_kind, character) in &characters[*from..to] {
                if *character_kind != kind && !run.is_empty() {
                    spans.push(Span::styled(std::mem::take(&mut run), theme::sql(kind)));
                }
                kind = character_kind;
                run.push(*character);
            }
            if !run.is_empty() {
                spans.push(Span::styled(run, theme::sql(kind)));
            }
            rows.push(Line::from(spans));
        }
    }

    rows.truncate(inner_height);
    rows
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokenises_like_tqls_sql_class() {
        let kinds: Vec<_> =
            tokenise("select * from `users` where name = 'O''Brien' and id > 10 -- note")
                .into_iter()
                .filter(|(kind, _)| *kind != "plain")
                .collect();

        assert_eq!(
            kinds,
            [
                ("keyword", "select".to_string()),
                ("operator", "*".to_string()),
                ("keyword", "from".to_string()),
                ("identifier", "`users`".to_string()),
                ("keyword", "where".to_string()),
                ("operator", "=".to_string()),
                ("string", "'O''Brien'".to_string()),
                ("keyword", "and".to_string()),
                ("operator", ">".to_string()),
                ("number", "10".to_string()),
                ("comment", "-- note".to_string()),
            ]
        );
    }

    #[test]
    fn shows_the_page_size_rather_than_the_lookahead_row() {
        assert_eq!(
            shown("select * from \"t\" limit 101 offset 0", 100),
            "select * from \"t\" limit 100 offset 0"
        );
    }

    #[test]
    fn wraps_at_the_last_space_that_fits() {
        let text: Vec<String> = lines(
            "select * from `activity_log` order by `id` asc limit 100 offset 0",
            40,
            5,
        )
        .iter()
        .map(|line| line.to_string())
        .collect();

        assert_eq!(
            text,
            [
                "select * from `activity_log` order by ",
                "`id` asc limit 100 offset 0"
            ]
        );
    }
}
