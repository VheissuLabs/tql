use crate::terminal::prelude::*;
use ratatui::buffer::Buffer;

use super::chrome::{Island, Modal, theme};
use super::text::{pad, truncate, window};

pub const WIDTH: u16 = 40;
const ROWS: usize = 9;

pub struct Choice {
    pub title: String,
    pub options: Vec<String>,
    pub index: usize,
    pub query: String,
    pub creates: bool,
    pub colors: Vec<(String, String)>,
}

impl Choice {
    pub fn new(title: &str, options: Vec<String>, chosen: &str, creates: bool) -> Self {
        let index = options
            .iter()
            .position(|option| option == chosen)
            .unwrap_or(0);
        Choice {
            title: title.to_string(),
            options,
            index,
            query: String::new(),
            creates,
            colors: Vec::new(),
        }
    }

    pub fn with_colors(mut self, colors: Vec<(String, String)>) -> Self {
        self.colors = colors;
        self
    }

    pub fn color_of(&self, option: &str) -> Option<&str> {
        self.colors
            .iter()
            .find(|(named, _)| named == option)
            .map(|(_, color)| color.as_str())
    }

    pub fn matches(&self) -> Vec<&String> {
        let needle = self.query.trim().to_lowercase();
        self.options
            .iter()
            .filter(|option| needle.is_empty() || option.to_lowercase().contains(&needle))
            .collect()
    }

    pub fn new_option(&self) -> Option<String> {
        let name = self.query.trim();
        let fresh =
            self.creates && !name.is_empty() && !self.options.iter().any(|option| option == name);
        fresh.then(|| name.to_string())
    }

    pub fn creating(&self) -> bool {
        self.new_option().is_some() && self.index == self.matches().len()
    }

    pub fn selected(&self) -> Option<String> {
        self.matches()
            .get(self.index)
            .map(|option| option.to_string())
            .or_else(|| self.new_option())
    }

    pub fn move_by(&mut self, forward: bool) {
        let count = self.matches().len() + usize::from(self.new_option().is_some());
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
        match key {
            Key::Char(character) => self.query.push(character),
            Key::Backspace | Key::Ctrl('h') => {
                self.query.pop();
            }
            Key::Ctrl('u') => self.query.clear(),
            _ => return,
        }
        self.index = 0;
    }

    pub fn rows(&self) -> u16 {
        (self.options.len() + usize::from(self.creates)).min(ROWS) as u16 + 5
    }

    pub fn draw_modal(&self, buffer: &mut Buffer, frame_width: u16, frame_height: u16) {
        let width = frame_width.saturating_sub(4).min(WIDTH);
        let height = self.rows();
        let island = Island {
            title: self.title.clone(),
            content: self.content(
                width.saturating_sub(2) as usize,
                height.saturating_sub(2) as usize,
            ),
            ..Island::default()
        };
        Modal::new(frame_width, 1, frame_height, width).draw(buffer, island, height);
    }

    pub fn content(&self, inner_width: usize, inner_height: usize) -> Vec<Line<'static>> {
        let width = inner_width.saturating_sub(4);
        let mut lines = vec![
            Line::raw(""),
            Line::from(vec![
                Span::raw("  "),
                Span::styled("/", theme::dim()),
                Span::raw(self.query.clone()),
                Span::styled(" ", theme::cursor()),
            ]),
            Line::raw(""),
        ];

        let matches = self.matches();
        let new_option = self.new_option();
        if matches.is_empty() && new_option.is_none() {
            lines.push(Line::from(vec![
                Span::raw("  "),
                Span::styled("nothing matches", theme::dim()),
            ]));
            lines.truncate(inner_height);
            return lines;
        }

        let mut rows: Vec<String> = matches.iter().map(|option| option.to_string()).collect();
        let creating_at = rows.len();
        rows.extend(new_option);

        let room = inner_height.saturating_sub(3).max(1);
        let start = window(self.index, rows.len(), room);
        for (at, option) in rows.iter().enumerate().skip(start).take(room) {
            let here = at == self.index;
            let color = (at != creating_at).then(|| self.color_of(option)).flatten();
            let text = if at == creating_at {
                pad(
                    &format!("  + create {}", truncate(option, width.saturating_sub(9))),
                    inner_width,
                )
            } else {
                pad(
                    &format!(
                        "  {}{}",
                        if color.is_some() { "● " } else { "  " },
                        truncate(option, width.saturating_sub(2))
                    ),
                    inner_width,
                )
            };
            lines.push(match (here, at == creating_at, color) {
                (true, _, _) => Line::styled(text, theme::selection()),
                (false, true, _) => Line::styled(text, theme::dim()),
                (false, false, Some(color)) => Line::from(vec![
                    Span::raw("  "),
                    Span::styled("●", theme::paint(color)),
                    Span::raw(text.chars().skip(3).collect::<String>()),
                ]),
                (false, false, None) => Line::raw(text),
            });
        }

        lines.truncate(inner_height);
        lines
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn databases() -> Choice {
        Choice::new(
            "DATABASE",
            ["information_schema", "mysql", "production", "sys"]
                .map(String::from)
                .to_vec(),
            "production",
            true,
        )
    }

    #[test]
    fn starts_on_the_database_in_use() {
        assert_eq!(databases().selected().as_deref(), Some("production"));
    }

    #[test]
    fn typing_filters_and_offers_to_create_what_is_not_there() {
        let mut choice = databases();
        for character in "prod".chars() {
            choice.type_key(Key::Char(character));
        }
        assert_eq!(choice.matches(), ["production"]);
        assert!(!choice.creating());

        choice.type_key(Key::Char('2'));
        assert!(choice.matches().is_empty());
        assert!(choice.creating());
        assert_eq!(choice.selected().as_deref(), Some("prod2"));
    }

    #[test]
    fn moving_wraps_around() {
        let mut choice = databases();
        choice.move_by(true);
        choice.move_by(true);
        assert_eq!(choice.selected().as_deref(), Some("information_schema"));
        choice.move_by(false);
        assert_eq!(choice.selected().as_deref(), Some("sys"));
    }
}
