use super::*;
use ratatui::buffer::Buffer;

use crate::registry::{self, Screen};
use crate::tui::chrome::{Island, Modal};
use crate::tui::keymap::{self, Action};
use crate::tui::palette::{self, Item, Kind, Palette};
use crate::tui::text::sentence;

impl Picker {
    pub(super) fn opens_palette(key: Key) -> bool {
        keymap::action(key) == Some(Action::Palette)
    }

    pub(super) fn open_palette(&mut self) {
        let commands = registry::on(Screen::Connections)
            .filter(|entry| entry.palette)
            .map(|entry| {
                Item::new(
                    Kind::Command(entry.name),
                    sentence(entry.description),
                    format!(":{}", entry.word()),
                    entry.word(),
                )
            });
        let connections = self.connections.iter().map(|connection| {
            Item::new(
                Kind::Connection,
                connection.name.clone(),
                connection.group_name.clone().unwrap_or_default(),
                connection.id.to_string(),
            )
        });
        self.palette = Some(Palette::new(
            "COMMANDS",
            commands.chain(connections).collect(),
        ));
    }

    pub(super) fn type_into_palette(&mut self, key: Key) -> Flow<Option<Connection>> {
        let Some(palette) = self.palette.as_mut() else {
            return Flow::Continue;
        };
        match key {
            Key::Escape => self.palette = None,
            Key::Enter => {
                let chosen = palette.selected();
                self.palette = None;
                if let Some(item) = chosen {
                    return self.run_palette_item(item);
                }
            }
            Key::Up | Key::Ctrl('p') => palette.move_by(false),
            Key::Down | Key::Ctrl('n') => palette.move_by(true),
            other => palette.type_key(other),
        }
        Flow::Continue
    }

    fn run_palette_item(&mut self, item: Item) -> Flow<Option<Connection>> {
        match item.kind {
            Kind::Command(name) => self.run_named(name, ""),
            Kind::Connection => {
                let chosen = self
                    .connections
                    .iter()
                    .find(|connection| connection.id.to_string() == item.target)
                    .cloned();
                match chosen {
                    Some(connection) => Flow::Exit(Some(connection)),
                    None => Flow::Continue,
                }
            }
            _ => Flow::Continue,
        }
    }

    pub(super) fn draw_palette(&self, buffer: &mut Buffer, frame_width: u16, frame_lines: u16) {
        let Some(palette) = &self.palette else {
            return;
        };
        let width = frame_width.saturating_sub(4).min(palette::WIDTH);
        let height = frame_lines.saturating_sub(2).min(palette.rows());
        let island = Island {
            title: palette.title.clone(),
            content: palette.content(
                width.saturating_sub(2) as usize,
                height.saturating_sub(2) as usize,
            ),
            ..Island::default()
        };
        Modal::new(frame_width, 1, frame_lines, width).draw(buffer, island, height);
    }
}
