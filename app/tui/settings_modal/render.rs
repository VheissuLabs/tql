use ratatui::buffer::Buffer;
use toml::Value;

use super::{Message, Row, SettingsModal, title};
use crate::config_file::kinds::{self, Kind};
use crate::config_file::template::Setting;
use crate::paths;
use crate::settings;
use crate::terminal::prelude::*;
use crate::tui::chrome::{Island, Modal, theme};
use crate::tui::text::{pad, truncate, typed, width, window};

const WIDTH: u16 = 84;
const NAME_WIDTH: usize = 30;

pub fn shown(setting: &Setting, value: &Value) -> String {
    match (kinds::kind(setting), value) {
        (Some(Kind::Flag), Value::Boolean(true)) => "on".to_string(),
        (Some(Kind::Flag), Value::Boolean(false)) => "off".to_string(),
        (Some(Kind::Secret), Value::String(text)) if text.is_empty() => "not set".to_string(),
        (Some(Kind::Secret), _) => "••••••••".to_string(),
        (_, Value::String(text)) if text.is_empty() => "empty".to_string(),
        (_, Value::String(text)) => text.clone(),
        (_, other) => other.to_string(),
    }
}

fn described(setting: &Setting) -> String {
    let mut lines: Vec<String> = setting
        .comment
        .iter()
        .map(|line| line.to_string())
        .collect();
    if lines.is_empty() {
        lines.push(format!(
            "The glyph shown beside a {} connection",
            setting.key
        ));
    }
    if kinds::applies_at_start(setting) {
        lines.push("Applies the next time tql starts.".to_string());
    }
    lines.join(" ")
}

fn wrapped(text: &str, room: usize) -> Vec<String> {
    let mut lines = vec![String::new()];
    for word in text.split_whitespace() {
        let last = lines.last_mut().expect("there is always a line");
        if !last.is_empty() && width(last) + 1 + width(word) > room {
            lines.push(word.to_string());
        } else {
            if !last.is_empty() {
                last.push(' ');
            }
            last.push_str(word);
        }
    }
    lines
}

impl SettingsModal {
    pub fn draw(&self, buffer: &mut Buffer, frame_width: u16, frame_lines: u16) {
        let box_width = frame_width.saturating_sub(4).min(WIDTH);
        let height = frame_lines.saturating_sub(2).max(8);
        let inner_width = box_width.saturating_sub(4) as usize;
        let inner_height = height.saturating_sub(2) as usize;

        let selected = self.current();
        let description = match selected {
            Some(Row::Setting(setting)) => wrapped(&described(setting), inner_width),
            Some(Row::Header(section)) => {
                vec![format!("{} settings · ↵ folds them", title(section))]
            }
            None => vec!["Nothing matches.".to_string()],
        };
        let message = match self.message() {
            Some(Message::Said(said)) => Line::styled(truncate(said, inner_width), theme::fresh()),
            Some(Message::Failed(failed)) => {
                Line::styled(truncate(failed, inner_width), theme::paint("red"))
            }
            None => Line::default(),
        };
        let home = paths::home();
        let file = self.path().display().to_string();
        let file = match file.strip_prefix(&home.display().to_string()) {
            Some(rest) => format!("~{rest}"),
            None => file,
        };
        let mut footer = vec![Line::default()];
        footer.extend(
            description
                .iter()
                .map(|line| Line::styled(line.clone(), theme::dim())),
        );
        footer.push(message);
        footer.push(Line::styled(
            truncate(&format!("Saved as you go to {file}"), inner_width),
            theme::dim(),
        ));

        let mut header = Vec::new();
        if let Some(filter) = self.filter() {
            header.push(Line::from(vec![
                Span::styled("/", theme::bold()),
                Span::raw(filter.to_string()),
            ]));
        }

        let room = inner_height
            .saturating_sub(footer.len() + header.len())
            .max(1);
        let rows = self.rows();
        let first = window(self.index, rows.len(), room);
        let mut content: Vec<Line<'static>> = header;
        for (position, row) in rows.iter().enumerate().skip(first).take(room) {
            content.push(self.row_line(*row, position == self.index, inner_width));
        }
        while content.len() < room + usize::from(self.filter().is_some()) {
            content.push(Line::default());
        }
        content.extend(footer);

        let island = Island {
            title: "SETTINGS".to_string(),
            content: content
                .into_iter()
                .map(|line| {
                    let mut spans = vec![Span::raw(" ")];
                    spans.extend(line.spans.clone());
                    Line::from(spans).style(line.style)
                })
                .collect(),
            ..Island::default()
        };
        Modal::new(frame_width, 1, frame_lines, box_width).draw(buffer, island, height);
    }

    fn row_line(&self, row: Row, selected: bool, room: usize) -> Line<'static> {
        let line = match row {
            Row::Header(section) => {
                let marker = if self.is_folded(section) {
                    "▸"
                } else {
                    "▾"
                };
                Line::from(vec![Span::styled(
                    pad(&format!("{marker} {}", title(section)), room),
                    theme::bold(),
                )])
            }
            Row::Setting(setting) => {
                let name = pad(&truncate(setting.key, NAME_WIDTH - 2), NAME_WIDTH);
                let value_room = room.saturating_sub(NAME_WIDTH + 2);
                let mut spans = vec![Span::raw(format!("  {name}"))];
                match self.editing().filter(|_| selected) {
                    Some(editor) => {
                        let characters: Vec<char> = editor.text().chars().collect();
                        let typed = typed(&characters, editor.cursor(), value_room);
                        spans.push(Span::raw(typed.before));
                        spans.push(Span::styled(typed.under, theme::cursor()));
                        spans.push(Span::raw(typed.after));
                    }
                    None => {
                        let value = shown(setting, &self.value(setting));
                        let style = if self.is_default(setting) {
                            Style::new()
                        } else {
                            theme::paint(&settings::with(|settings| {
                                settings.color("edited", "yellow")
                            }))
                        };
                        let choosing = matches!(kinds::kind(setting), Some(Kind::Choice(_)));
                        let value = match (choosing, selected) {
                            (true, true) => format!("‹ {value} ›"),
                            _ => value,
                        };
                        spans.push(Span::styled(truncate(&value, value_room), style));
                    }
                }
                Line::from(spans)
            }
        };
        if !selected {
            return line;
        }
        let used = line.width();
        let mut spans = line.spans;
        spans.push(Span::raw(" ".repeat(room.saturating_sub(used))));
        Line::from(spans).style(theme::selection())
    }
}
