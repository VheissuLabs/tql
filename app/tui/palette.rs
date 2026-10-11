use std::sync::LazyLock;

use crate::terminal::prelude::*;
use regex::Regex;

use super::chrome::theme;
use super::editor::QueryEditor;
use super::keymap::Action;
use super::text::{pad, truncate, width, window};

pub const WIDTH: u16 = 64;
const ROWS: usize = 12;

static WORD_START: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"[\s_:/.-]").expect("the word start pattern is valid"));

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Action(Action),
    Command,
    Table,
    Database,
    Connection,
    History,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Item {
    pub kind: Kind,
    pub label: String,
    pub hint: String,
    pub target: String,
}

impl Item {
    pub fn new(
        kind: Kind,
        label: impl Into<String>,
        hint: impl Into<String>,
        target: impl Into<String>,
    ) -> Self {
        Item {
            kind,
            label: label.into(),
            hint: hint.into(),
            target: target.into(),
        }
    }
}

pub struct Palette {
    pub title: String,
    pub items: Vec<Item>,
    pub index: usize,
    pub query: QueryEditor,
}

impl Palette {
    pub fn new(title: &str, items: Vec<Item>) -> Self {
        Palette {
            title: title.to_string(),
            items,
            index: 0,
            query: QueryEditor::new(false),
        }
    }

    pub fn matches(&self) -> Vec<&Item> {
        let needle = self.query.buffer().trim().to_lowercase();
        if needle.is_empty() {
            return self.items.iter().collect();
        }

        let mut ranked: Vec<(usize, usize, &Item)> = self
            .items
            .iter()
            .enumerate()
            .filter_map(|(order, item)| {
                Some((score(&item.label.to_lowercase(), &needle)?, order, item))
            })
            .collect();
        ranked.sort_by_key(|(score, order, _)| (*score, *order));
        ranked.into_iter().map(|(_, _, item)| item).collect()
    }

    pub fn move_by(&mut self, forward: bool) {
        let count = self.matches().len();
        if count == 0 {
            self.index = 0;
            return;
        }
        self.index = if forward {
            (self.index + 1) % count
        } else {
            (self.index + count - 1) % count
        };
    }

    pub fn type_key(&mut self, key: Key) {
        self.query.handle(key);
        self.index = 0;
    }

    pub fn selected(&self) -> Option<Item> {
        self.matches().get(self.index).map(|item| (*item).clone())
    }

    pub fn rows(&self) -> u16 {
        (self.items.len().clamp(1, ROWS) + 5) as u16
    }

    pub fn content(&self, inner_width: usize, inner_height: usize) -> Vec<Line<'static>> {
        let mut lines = vec![
            Line::default(),
            Line::from(
                [
                    vec![
                        Span::raw("  "),
                        Span::styled("›", theme::dim()),
                        Span::raw(" "),
                    ],
                    self.typing(),
                ]
                .concat(),
            ),
            Line::default(),
        ];

        let matches = self.matches();
        if matches.is_empty() {
            lines.push(Line::from(vec![
                Span::raw("  "),
                Span::styled("nothing matches", theme::dim()),
            ]));
            lines.truncate(inner_height);
            return lines;
        }

        let room = inner_height.saturating_sub(lines.len()).max(1);
        let start = window(self.index, matches.len(), room);
        lines.extend(
            matches
                .iter()
                .enumerate()
                .skip(start)
                .take(room)
                .map(|(position, item)| row(item, inner_width, position == self.index)),
        );
        lines.truncate(inner_height);
        lines
    }

    fn typing(&self) -> Vec<Span<'static>> {
        let characters: Vec<char> = self.query.buffer().chars().collect();
        let at = self.query.cursor();
        let Some(under) = characters.get(at) else {
            return vec![
                Span::raw(self.query.buffer()),
                Span::styled(" ", theme::cursor()),
            ];
        };
        vec![
            Span::raw(characters[..at].iter().collect::<String>()),
            Span::styled(under.to_string(), theme::cursor()),
            Span::raw(characters[at + 1..].iter().collect::<String>()),
        ]
    }
}

pub fn score(label: &str, needle: &str) -> Option<usize> {
    if label == needle {
        return Some(0);
    }
    if label.starts_with(needle) {
        return Some(1);
    }
    let starts_a_word = label.match_indices(needle).any(|(at, _)| {
        label[..at]
            .chars()
            .last()
            .is_some_and(|before| WORD_START.is_match(&before.to_string()))
    });
    if starts_a_word {
        return Some(2);
    }
    if label.contains(needle) {
        return Some(3);
    }
    in_order(label, needle).then_some(4)
}

fn in_order(label: &str, needle: &str) -> bool {
    let mut remaining = label.chars();
    needle
        .chars()
        .all(|wanted| remaining.any(|character| character == wanted))
}

fn row(item: &Item, inner_width: usize, here: bool) -> Line<'static> {
    let hint_width = width(&item.hint);
    let label_room = inner_width.saturating_sub(hint_width + 6).max(4);
    let label = pad(
        &format!("  {}", truncate(&item.label, label_room)),
        inner_width.saturating_sub(hint_width + 2),
    );
    if here {
        return Line::styled(format!("{label}{}  ", item.hint), theme::selection());
    }
    Line::from(vec![
        Span::raw(label),
        Span::styled(item.hint.clone(), theme::dim()),
        Span::raw("  "),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ranks_an_exact_match_then_a_prefix_then_a_word_then_letters_in_order() {
        assert_eq!(score("filter rows", "filter rows"), Some(0));
        assert_eq!(score("filter rows", "filt"), Some(1));
        assert_eq!(score("filter rows", "rows"), Some(2));
        assert_eq!(score("customers", "tom"), Some(3));
        assert_eq!(score("filter rows", "frw"), Some(4));
        assert_eq!(score("filter rows", "xyz"), None);
    }

    #[test]
    fn narrows_as_you_type_and_wraps_when_moving() {
        let mut palette = Palette::new(
            "COMMANDS",
            vec![
                Item::new(Kind::Table, "invoices", "table", "invoices"),
                Item::new(
                    Kind::Action(Action::FilterTables),
                    "Filter tables",
                    "/",
                    "filter_tables",
                ),
                Item::new(
                    Kind::Action(Action::FilterRows),
                    "Filter rows",
                    "f",
                    "filter_rows",
                ),
            ],
        );
        palette.move_by(false);
        assert_eq!(palette.selected().unwrap().target, "filter_rows");

        for character in "filt".chars() {
            palette.type_key(Key::Char(character));
        }
        let targets: Vec<_> = palette
            .matches()
            .iter()
            .map(|item| item.target.clone())
            .collect();
        assert_eq!(targets, ["filter_tables", "filter_rows"]);
        assert_eq!(palette.index, 0);
    }
}
