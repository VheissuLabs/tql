use std::collections::{HashMap, HashSet};

pub type Row = Vec<(String, String)>;

const CELL_FLOOR: usize = 8;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Section {
    Record,
    Related,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Fold {
    Record,
    Related,
    Relation(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Line {
    pub text: String,
    pub fold: Option<Fold>,
    pub section: Section,
    pub heading: bool,
    pub column: Option<String>,
    pub table: Option<String>,
    pub row: Option<usize>,
}

impl Line {
    fn new(text: String, section: Section) -> Self {
        Line {
            text,
            fold: None,
            section,
            heading: false,
            column: None,
            table: None,
            row: None,
        }
    }

    fn heading(text: String, fold: Fold, section: Section) -> Self {
        Line {
            fold: Some(fold),
            heading: true,
            ..Line::new(text, section)
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Relation {
    pub table: String,
    pub rows: Option<Vec<Row>>,
    pub shown: usize,
    pub total: Option<usize>,
    pub hide: Vec<String>,
    pub kind: String,
}

impl Relation {
    fn one(&self) -> bool {
        !self.kind.starts_with("has many")
    }
}

pub struct RowDocument {
    row: Row,
    types: HashMap<String, String>,
    related: Vec<Relation>,
    folded: HashSet<Fold>,
    width: Option<usize>,
}

impl RowDocument {
    pub fn new(row: Row, types: HashMap<String, String>, related: Vec<Relation>) -> Self {
        let folded = related
            .iter()
            .map(|relation| Fold::Relation(relation.table.clone()))
            .collect();
        RowDocument {
            row,
            types,
            related,
            folded,
            width: None,
        }
    }

    pub fn lines(&self) -> Vec<Line> {
        let mut lines = vec![Line::heading(
            format!("RECORD  ({})", self.row.len()),
            Fold::Record,
            Section::Record,
        )];

        if !self.is_folded(&Fold::Record) {
            lines.extend(self.row.iter().map(|(column, value)| Line {
                column: Some(column.clone()),
                ..Line::new(
                    format!("  {}", self.field(column, value, true)),
                    Section::Record,
                )
            }));
        }

        if self.related.is_empty() {
            return lines;
        }

        lines.push(Line::heading(
            format!("RELATED  ({})", self.related.len()),
            Fold::Related,
            Section::Related,
        ));

        if self.is_folded(&Fold::Related) {
            return lines;
        }

        for relation in &self.related {
            let fold = Fold::Relation(relation.table.clone());
            let marker = if self.is_folded(&fold) { "▸" } else { "▾" };
            let count = match relation.total {
                _ if relation.one() => String::new(),
                Some(total) if total > relation.shown => {
                    format!("  ({} of {total})", relation.shown)
                }
                _ => format!("  ({})", relation.shown),
            };
            lines.push(Line {
                fold: Some(fold.clone()),
                ..Line::new(
                    format!("  {marker} {}  ·  {}{count}", relation.table, relation.kind),
                    Section::Related,
                )
            });

            if !self.is_folded(&fold) {
                lines.extend(self.body(relation));
            }
        }
        lines
    }

    fn body(&self, relation: &Relation) -> Vec<Line> {
        let Some(rows) = &relation.rows else {
            return vec![body_line("loading…", None, None)];
        };

        if relation.one() && rows.len() == 1 {
            return rows[0]
                .iter()
                .filter(|(column, _)| !relation.hide.contains(column))
                .map(|(column, value)| {
                    body_line(
                        &self.field(column, value, false),
                        Some(&relation.table),
                        Some(0),
                    )
                })
                .collect();
        }

        self.collection(rows, &relation.hide)
            .iter()
            .enumerate()
            .map(|(position, text)| body_line(text, Some(&relation.table), position.checked_sub(1)))
            .collect()
    }

    fn collection(&self, rows: &[Row], hide: &[String]) -> Vec<String> {
        let columns: Vec<&String> = rows
            .first()
            .map(|row| {
                row.iter()
                    .map(|(column, _)| column)
                    .filter(|column| !hide.contains(column))
                    .collect()
            })
            .unwrap_or_default();
        if columns.is_empty() {
            return Vec::new();
        }

        let value_of = |row: &Row, column: &str| -> String {
            row.iter()
                .find(|(name, _)| name == column)
                .map(|(_, value)| value.clone())
                .unwrap_or_else(|| "NULL".to_string())
        };
        let natural: Vec<usize> = columns
            .iter()
            .map(|column| {
                rows.iter()
                    .map(|row| length(&value_of(row, column)))
                    .fold(length(column), usize::max)
            })
            .collect();
        let cap = self.cap(&natural);
        let widths: Vec<usize> = natural.iter().map(|width| (*width).min(cap)).collect();

        let line = |cells: Vec<String>| -> String {
            cells
                .iter()
                .zip(&widths)
                .map(|(cell, width)| pad(&clamp(cell, *width), *width))
                .collect::<Vec<_>>()
                .join("  ")
                .trim_end()
                .to_string()
        };

        std::iter::once(line(
            columns.iter().map(|column| column.to_string()).collect(),
        ))
        .chain(
            rows.iter()
                .map(|row| line(columns.iter().map(|column| value_of(row, column)).collect())),
        )
        .collect()
    }

    fn cap(&self, natural: &[usize]) -> usize {
        let Some(width) = self.width else {
            return usize::MAX;
        };
        let gaps = natural.len().saturating_sub(1) * 2;
        let room = width.saturating_sub(gaps).max(CELL_FLOOR);
        if natural.iter().sum::<usize>() <= room {
            return usize::MAX;
        }

        let widest = natural.iter().copied().max().unwrap_or(CELL_FLOOR);
        (CELL_FLOOR..=widest)
            .rev()
            .find(|attempt| {
                natural
                    .iter()
                    .map(|width| (*width).min(*attempt))
                    .sum::<usize>()
                    <= room
            })
            .unwrap_or(CELL_FLOOR)
    }

    fn field(&self, column: &str, value: &str, top_level: bool) -> String {
        let kind = if top_level {
            self.types
                .get(column)
                .map(String::as_str)
                .unwrap_or_default()
        } else {
            ""
        };
        let name = pad(&clamp(column, 21), 22);
        if kind.is_empty() {
            return format!("{name}{value}");
        }
        format!("{name}{}{kind}", pad(&clamp(value, 46), 48))
    }

    pub fn section(&self, section: Section) -> Vec<(usize, Line)> {
        self.lines()
            .into_iter()
            .enumerate()
            .filter(|(_, line)| line.section == section && !line.heading)
            .collect()
    }

    pub fn heading_at(&self, section: Section) -> Option<usize> {
        self.lines()
            .iter()
            .position(|line| line.section == section && line.heading)
    }

    pub fn has_related(&self) -> bool {
        !self.related.is_empty()
    }

    pub fn waiting(&self) -> Vec<String> {
        if self.is_folded(&Fold::Related) {
            return Vec::new();
        }
        self.related
            .iter()
            .filter(|relation| {
                relation.rows.is_none() && !self.is_folded(&Fold::Relation(relation.table.clone()))
            })
            .map(|relation| relation.table.clone())
            .collect()
    }

    pub fn fill(&mut self, table: &str, rows: Vec<Row>) {
        let Some(relation) = self
            .related
            .iter_mut()
            .find(|relation| relation.table == table)
        else {
            return;
        };
        if relation.total.is_none() {
            relation.shown = rows.len();
        }
        relation.rows = Some(rows);
    }

    pub fn toggle(&mut self, line: usize) {
        let Some(fold) = self.lines().get(line).and_then(|line| line.fold.clone()) else {
            return;
        };
        if !self.folded.remove(&fold) {
            self.folded.insert(fold);
        }
    }

    pub fn is_folded(&self, fold: &Fold) -> bool {
        self.folded.contains(fold)
    }

    pub fn column_at(&self, line: usize) -> Option<String> {
        self.lines().get(line).and_then(|line| line.column.clone())
    }

    pub fn fit_to(&mut self, width: usize) {
        self.width = Some(width.max(CELL_FLOOR));
    }

    pub fn natural_width(&self) -> usize {
        self.lines()
            .iter()
            .map(|line| length(&line.text))
            .max()
            .unwrap_or(0)
            + 4
    }
}

fn body_line(text: &str, table: Option<&str>, row: Option<usize>) -> Line {
    Line {
        table: table.map(String::from),
        row,
        ..Line::new(format!("      {text}"), Section::Related)
    }
}

fn length(text: &str) -> usize {
    text.chars().count()
}

fn clamp(text: &str, width: usize) -> String {
    if length(text) <= width {
        return text.to_string();
    }
    let kept: String = text.chars().take(width.saturating_sub(1)).collect();
    format!("{kept}…")
}

fn pad(text: &str, width: usize) -> String {
    format!("{text}{}", " ".repeat(width.saturating_sub(length(text))))
}

pub fn shown(value: Option<&str>) -> String {
    let Some(value) = value else {
        return "NULL".to_string();
    };
    json(value)
        .map(|decoded| decoded.to_string())
        .unwrap_or_else(|| value.to_string())
}

pub fn json(value: &str) -> Option<serde_json::Value> {
    let trimmed = value.trim();
    if !trimmed.starts_with(['{', '[']) {
        return None;
    }
    serde_json::from_str(trimmed).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(document: &RowDocument) -> String {
        document
            .lines()
            .iter()
            .map(|line| line.text.as_str())
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn row(pairs: &[(&str, &str)]) -> Row {
        pairs
            .iter()
            .map(|(column, value)| (column.to_string(), value.to_string()))
            .collect()
    }

    fn albums(rows: Option<Vec<Row>>, total: Option<usize>) -> Relation {
        Relation {
            table: "albums".to_string(),
            rows,
            shown: 1,
            total,
            hide: vec!["artist_id".to_string()],
            kind: "has many".to_string(),
        }
    }

    fn artist(related: Vec<Relation>) -> RowDocument {
        RowDocument::new(
            row(&[("id", "1"), ("name", "AC/DC")]),
            [("id", "integer"), ("name", "text")]
                .into_iter()
                .map(|(column, kind)| (column.to_string(), kind.to_string()))
                .collect(),
            related,
        )
    }

    #[test]
    fn reads_column_value_then_type() {
        let document = artist(Vec::new());

        assert_eq!(
            text(&document),
            format!(
                "RECORD  (2)\n  id{}1{}integer\n  name{}AC/DC{}text",
                " ".repeat(20),
                " ".repeat(47),
                " ".repeat(18),
                " ".repeat(43)
            )
        );
    }

    #[test]
    fn opens_with_every_relation_folded_and_loads_only_what_is_unfolded() {
        let mut document = artist(vec![albums(None, Some(1))]);

        assert!(text(&document).contains("▸ albums  ·  has many  (1)"));
        assert!(document.waiting().is_empty());

        document.toggle(4);
        assert!(text(&document).contains("▾ albums"));
        assert!(text(&document).contains("loading…"));
        assert_eq!(document.waiting(), ["albums"]);

        document.fill(
            "albums",
            vec![row(&[
                ("id", "1"),
                ("artist_id", "1"),
                ("title", "Let There Be Rock"),
            ])],
        );
        let lines = document.lines();
        assert_eq!(lines[5].text, "      id  title");
        assert_eq!(lines[6].text, "      1   Let There Be Rock");
        assert_eq!(lines[6].row, Some(0));
    }

    #[test]
    fn says_how_many_of_how_many_when_capped() {
        let mut relation = albums(None, Some(13));
        relation.shown = 3;

        assert!(text(&artist(vec![relation])).contains("albums  ·  has many  (3 of 13)"));
    }

    #[test]
    fn clamps_a_long_column_to_fit_the_room_it_has() {
        let mut document = artist(vec![albums(
            Some(vec![row(&[
                ("id", "1"),
                ("title", "The Dispossessed"),
                ("blurb", &"a long description ".repeat(6)),
            ])]),
            Some(1),
        )]);
        document.toggle(4);
        document.fit_to(40);

        let lines = document.lines();
        assert!(lines[6].text.contains('…'));
        assert!(lines[6].text.trim_start().chars().count() <= 40);
    }

    #[test]
    fn decodes_json_into_its_compact_form() {
        assert_eq!(shown(Some("{\"a\": [1, 2]}")), "{\"a\":[1,2]}");
        assert_eq!(shown(None), "NULL");
        assert_eq!(shown(Some("[not json")), "[not json");
    }
}
