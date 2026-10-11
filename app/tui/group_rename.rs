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
        let typed = super::text::typed(&characters, self.editor.cursor(), room);
        let (before, under, after) = (typed.before, typed.under, typed.after);

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
