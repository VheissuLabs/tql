use zerorust_tui::prelude::*;

use super::chrome::{Island, theme};
use super::editor::QueryEditor;
use super::json;
use super::text::{pad, window};

pub const WIDTH: usize = 76;

pub struct ValueView<'a> {
    pub title: String,
    pub buffer: &'a QueryEditor,
    pub json: bool,
    pub show_cursor: bool,
    pub selection: Option<(usize, usize)>,
}

impl ValueView<'_> {
    pub fn natural_width(&self) -> usize {
        let lines = self.buffer.lines();
        let longest = lines
            .iter()
            .map(|line| line.chars().count())
            .max()
            .unwrap_or(0);
        WIDTH.max(longest + gutter(lines.len()) + 7)
    }

    pub fn rows(&self) -> usize {
        self.buffer.lines().len().max(1) + 2
    }

    pub fn first_line(&self, inner_height: usize) -> usize {
        window(
            self.buffer.cursor_line(),
            self.buffer.lines().len(),
            inner_height,
        )
    }

    pub fn island(&self, inner_width: usize, inner_height: usize) -> Island {
        Island {
            title: if self.json {
                format!("{}  ·  json", self.title)
            } else {
                self.title.clone()
            },
            content: self.content(inner_width, inner_height),
            ..Island::default()
        }
    }

    fn content(&self, inner_width: usize, inner_height: usize) -> Vec<Line<'static>> {
        let lines = self.buffer.lines();
        let cursor_line = self.buffer.cursor_line();
        let gutter = gutter(lines.len());
        let room = inner_width.saturating_sub(gutter + 4);

        lines
            .iter()
            .enumerate()
            .skip(self.first_line(inner_height))
            .take(inner_height)
            .map(|(number, line)| {
                let here = match (number == cursor_line, self.show_cursor) {
                    (true, false) => Span::styled("▸", theme::bold()),
                    _ => Span::raw(" "),
                };
                let label = Span::styled(format!("{:>gutter$}", number + 1), theme::sql("gutter"));
                let mut spans = vec![here, label, Span::raw(" ")];

                let selected = self
                    .selection
                    .is_some_and(|(from, to)| number >= from && number <= to);
                if selected {
                    let shown: String = line.chars().take(room).collect();
                    spans.push(Span::styled(pad(&shown, room), theme::selection()));
                    return Line::from(spans);
                }

                let cursor = (self.show_cursor && number == cursor_line)
                    .then(|| self.buffer.cursor_column());
                spans.extend(self.painted(line, room, cursor));
                Line::from(spans)
            })
            .collect()
    }

    fn painted(&self, line: &str, room: usize, cursor: Option<usize>) -> Vec<Span<'static>> {
        let tokens = if self.json {
            json::tokenise(line)
        } else {
            vec![("plain", line.to_string())]
        };
        let mut characters: Vec<(char, Style)> = tokens
            .iter()
            .flat_map(|(kind, text)| {
                let style = match *kind {
                    "plain" => Style::new(),
                    other => theme::sql(other),
                };
                text.chars().map(move |character| (character, style))
            })
            .take(room)
            .collect();

        let length = characters.len();
        match (cursor, cursor.and_then(|cursor| characters.get_mut(cursor))) {
            (_, Some((_, style))) => *style = theme::cursor(),
            (Some(_), None) if length < room => characters.push((' ', theme::cursor())),
            _ => {}
        }

        characters
            .into_iter()
            .map(|(character, style)| Span::styled(character.to_string(), style))
            .collect()
    }
}

fn gutter(count: usize) -> usize {
    count.to_string().len().max(2)
}
