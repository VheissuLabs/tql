use ratatui::buffer::Buffer;
use ratatui::text::{Line, Span};

use super::chrome::{Island, Modal, theme};
use super::editor::QueryEditor;
use super::text::{single_line, truncate};
use crate::terminal::Key;

pub const BAR: &str = " ↵ Rename    esc Cancel";

pub enum Step {
    Stay,
    Cancel,
    Save,
}

pub struct GroupRename {
    pub from: String,
    pub error: Option<String>,
    editor: QueryEditor,
}

impl GroupRename {
    pub fn new(group: &str) -> Self {
        GroupRename {
            from: group.to_string(),
            error: None,
            editor: QueryEditor::holding(group),
        }
    }

    pub fn name(&self) -> String {
        self.editor.buffer().trim().to_string()
    }

    pub fn on_key(&mut self, key: Key) -> Step {
        match key {
            Key::Escape => return Step::Cancel,
            Key::Enter | Key::Ctrl('s') => return Step::Save,
            Key::Up | Key::Down | Key::Tab | Key::BackTab => {}
            other => {
                self.editor.handle(other);
                self.error = None;
            }
        }
        Step::Stay
    }

    pub fn on_paste(&mut self, text: &str) {
        self.editor.paste(&single_line(text));
        self.error = None;
    }

    pub fn draw(&self, buffer: &mut Buffer, frame_width: u16, frame_lines: u16) {
        let box_width = frame_width.saturating_sub(4).clamp(20, 50);
        let room = box_width.saturating_sub(6) as usize;
        let characters: Vec<char> = self.editor.buffer().chars().collect();
        let cursor = self.editor.cursor().min(characters.len());
        let start = cursor.saturating_sub(room.saturating_sub(1));
        let before: String = characters[start..cursor].iter().collect();
        let under: String = characters
            .get(cursor)
            .map(|character| character.to_string())
            .unwrap_or_else(|| " ".to_string());
        let after: String = characters
            .iter()
            .skip(cursor + 1)
            .take(room.saturating_sub(before.chars().count() + 1))
            .collect();

        let mut content = vec![
            Line::from(vec![
                Span::raw("  "),
                Span::raw(before),
                Span::styled(under, theme::cursor()),
                Span::raw(after),
            ]),
            Line::default(),
        ];
        content.push(match &self.error {
            Some(error) => {
                Line::styled(format!("  {}", truncate(error, room)), theme::paint("red"))
            }
            None => Line::styled(
                format!(
                    "  {}",
                    truncate("every connection in it moves to the new name", room)
                ),
                theme::dim(),
            ),
        });

        let island = Island {
            title: "RENAME GROUP".to_string(),
            content,
            ..Island::default()
        };
        Modal::new(frame_width, 1, frame_lines, box_width).draw(buffer, island, 5);
    }
}
