use std::collections::HashMap;

use super::edit::plural;
use super::*;
use crate::keystore::shown as shown_bytes;
use crate::tui::chrome::{self, Island, Modal, theme};
use crate::tui::grid::{self, Grid};
use crate::tui::text::{pad, truncate, typed, width, window, wrap};
use crate::tui::{json, palette, sql};

const CLI_HEIGHT: u16 = 6;

impl KeyBrowser {
    fn summary(&self) -> String {
        let count = match (self.scan.is_some(), self.capped) {
            (true, _) => format!("scanning {} of {} keys", self.keys.len(), self.total),
            (false, true) => format!("first {} of {} keys", self.keys.len(), self.total),
            (false, false) => plural(self.keys.len(), "key"),
        };
        let pattern = match self.pattern.is_empty() {
            true => String::new(),
            false => format!(" · /{}", self.pattern),
        };
        let pending = match self.pending.len() {
            0 => String::new(),
            count => format!(" · {} pending", plural(count, "change")),
        };
        format!(
            "{} · {} · db{} · {count}{pattern}{pending}",
            self.connection.name, self.server, self.database
        )
    }

    fn offers(&self) -> Vec<(String, &'static str)> {
        let pairs: &[(&str, &'static str)] = match (self.cli.is_some(), self.focus) {
            (true, _) => &[("ctrl+r", "Run"), ("esc", "Close")],
            (false, Focus::Keys) => &[
                ("↵", "Open"),
                ("/", "Find"),
                ("b", "Database"),
                ("N", "New"),
                ("d", "Delete"),
                ("T", "TTL"),
                ("R", "Rename"),
                ("s", "Command"),
                (":w", "Write"),
            ],
            (false, Focus::Value) => &[
                ("e", "Edit"),
                ("N", "Add"),
                ("d", "Delete"),
                ("n p", "Page"),
                ("esc", "Keys"),
                (":w", "Write"),
            ],
        };
        pairs
            .iter()
            .map(|(key, label)| (key.to_string(), *label))
            .collect()
    }

    fn status_line(&self) -> Line<'static> {
        if let Some(command) = &self.command {
            return Line::from(vec![Span::raw(format!(" :{command}")), Span::raw("█")]);
        }
        if let Some(typed) = &self.filtering {
            return Line::from(vec![
                Span::raw(format!(" /{typed}")),
                Span::raw("█"),
                Span::styled("   a pattern such as user:* , or words", theme::dim()),
            ]);
        }
        match &self.status {
            Some(status) if self.failed => Line::styled(format!(" {status}"), theme::paint("red")),
            Some(status) => Line::styled(format!(" {status}"), theme::fresh()),
            None => Line::styled(format!(" {}", self.summary()), theme::dim()),
        }
    }

    fn sidebar(&self, x: u16, y: u16, width_: u16, height: u16) -> Island {
        let mut island = Island::placed(x, y, width_, height);
        island.focused = self.focus == Focus::Keys && self.cli.is_none();
        island.title = format!("[1] keys · db{}", self.database);
        let inner_width = island.inner_width();
        let inner_height = island.inner_height();
        let rows = self.rows();
        if rows.is_empty() {
            island.content = vec![Line::styled(
                match self.scan.is_some() {
                    true => " scanning…",
                    false => " no keys",
                },
                theme::dim(),
            )];
            return island;
        }
        let start = window(self.index, rows.len(), inner_height);
        island.content = rows
            .iter()
            .enumerate()
            .skip(start)
            .take(inner_height)
            .map(|(position, row)| {
                let (label, right, style) = match row {
                    Row::Space {
                        name,
                        depth,
                        count,
                        open,
                        ..
                    } => (
                        format!(
                            " {}{} {name}",
                            "  ".repeat(*depth),
                            if *open { "▾" } else { "▸" }
                        ),
                        format!("({count}) "),
                        theme::bold(),
                    ),
                    Row::Key { key, name, depth } => {
                        let kind = self
                            .kinds
                            .get(key)
                            .map(|kind| kind.name().to_string())
                            .unwrap_or_default();
                        let style = match self.tone_of_key(key) {
                            Some(tone) => tone.style(),
                            None => Style::default(),
                        };
                        (
                            format!(" {}  {name}", "  ".repeat(*depth)),
                            format!("{kind} "),
                            style,
                        )
                    }
                };
                let room = inner_width.saturating_sub(width(&right) + 1);
                let left = pad(&truncate(&label, room), room);
                let line = format!("{left} {right}");
                if position == self.index {
                    Line::styled(pad(&line, inner_width), theme::selection())
                } else {
                    Line::from(vec![
                        Span::styled(left, style),
                        Span::styled(format!(" {right}"), theme::dim()),
                    ])
                }
            })
            .collect();
        island
    }

    fn value_title(shown: &Shown) -> String {
        let size = match &shown.contents {
            Contents::Text(bytes) => plural(bytes.len(), "byte"),
            Contents::Rows { rows, more, .. } => {
                let noun = match shown.kind {
                    KeyKind::Hash => "field",
                    KeyKind::List => "item",
                    KeyKind::Stream => "entry",
                    _ => "member",
                };
                let counted = plural(shown.offset + rows.len(), noun);
                match (*more, noun) {
                    (true, "entry") => format!("{}+ entries", shown.offset + rows.len()),
                    (true, noun) => format!("{}+ {noun}s", shown.offset + rows.len()),
                    (false, _) => counted,
                }
            }
            Contents::Missing => "gone".to_string(),
            Contents::Unsupported(_) => "not shown".to_string(),
        };
        let ttl = match shown.ttl {
            Some(seconds) => format!("TTL {seconds}s"),
            None => "TTL none".to_string(),
        };
        format!("[2] {} · {} · {size} · {ttl}", shown.key, shown.kind.name())
    }

    fn grid_island(
        &self,
        mut island: Island,
        headers: &[String],
        rows: &[Vec<String>],
        tones: &[Option<crate::tui::grid::Tone>],
        row_index: usize,
    ) -> Island {
        let overrides: HashMap<String, usize> = HashMap::new();
        let grid = Grid {
            headers,
            rows,
            row_index,
            column_index: 0,
            column_offset: 0,
            sort: None,
            focused: island.focused,
            overrides: &overrides,
            tones,
            locked: false,
        };
        let (_, widths, content) = grid.lines(island.inner_width(), island.inner_height());
        if !headers.is_empty() {
            island.joins = grid::joins(&widths);
            island.rules = vec![1];
        }
        island.content = content;
        island
    }

    fn text_lines(text: &str, room: usize) -> Vec<Line<'static>> {
        let shown = match json::looks_like_json(Some(text)) {
            true => json::pretty(text),
            false => text.to_string(),
        };
        wrap(&shown, room.max(10))
            .into_iter()
            .map(|line| Line::raw(format!(" {line}")))
            .collect()
    }

    fn value(&self, x: u16, y: u16, width_: u16, height: u16) -> Island {
        let mut island = Island::placed(x, y, width_, height);
        island.focused = self.focus == Focus::Value && self.cli.is_none();
        if let Some(cli) = &self.cli {
            island.title = format!(
                "[2] {}",
                cli.ran.clone().unwrap_or_else(|| "reply".to_string())
            );
            return match &cli.reply {
                Some(Reply::Error(said)) => {
                    island.content = vec![Line::styled(format!(" {said}"), theme::paint("red"))];
                    island
                }
                Some(Reply::Text(text)) => {
                    island.content = Self::text_lines(text, island.inner_width());
                    island
                }
                Some(reply) => {
                    let (headers, rows) = reply_rows(reply);
                    let tones = vec![None; rows.len()];
                    self.grid_island(island, &headers, &rows, &tones, 0)
                }
                None => {
                    island.content = vec![Line::styled(" the reply shows here", theme::dim())];
                    island
                }
            };
        }
        let Some(shown) = &self.shown else {
            island.title = "[2] value".to_string();
            island.content = vec![Line::styled(" ↵ on a key shows its value", theme::dim())];
            return island;
        };
        island.title = Self::value_title(shown);
        match &shown.contents {
            Contents::Missing => {
                island.content = vec![Line::styled(
                    format!(" {} is gone: it expired or was deleted", shown.key),
                    theme::dim(),
                )];
                island
            }
            Contents::Unsupported(kind) => {
                island.content = vec![Line::styled(
                    format!(" {kind} values are not shown"),
                    theme::dim(),
                )];
                island
            }
            Contents::Text(bytes) => {
                let pending = self.pending.iter().rev().find_map(|change| match change {
                    Change::SetString { key, value } if *key == shown.key => Some(value.clone()),
                    _ => None,
                });
                let text = pending.clone().unwrap_or_else(|| shown_bytes(bytes));
                let mut lines = Self::text_lines(&text, island.inner_width().saturating_sub(2));
                if let Some(tone) = self.tone_of_key(&shown.key) {
                    lines = lines
                        .into_iter()
                        .map(|line| line.style(tone.style()))
                        .collect();
                }
                island.content = lines;
                island
            }
            Contents::Rows { .. } => {
                let (headers, rows, tones) = self.overlaid().unwrap_or_default();
                self.grid_island(island, &headers, &rows, &tones, shown.row)
            }
        }
    }

    fn cli_island(&self, x: u16, y: u16, width_: u16) -> Option<(Island, (usize, usize))> {
        let cli = self.cli.as_ref()?;
        let mut island = Island::placed(x, y, width_, CLI_HEIGHT);
        island.focused = true;
        island.title = format!("[3] {}", self.server);
        let pane = sql::pane(
            &cli.editor,
            None,
            true,
            &sql::Marks::default(),
            (
                island.inner_width().saturating_sub(1),
                island.inner_height(),
            ),
        );
        island.content = pane
            .lines
            .into_iter()
            .map(|line| {
                let mut spans = vec![Span::raw(" ")];
                spans.extend(line.spans);
                Line::from(spans)
            })
            .collect();
        Some((island, pane.cursor))
    }

    fn prompt_island(&self, prompt: &edit::Prompt, box_width: u16) -> Island {
        let room = box_width.saturating_sub(6) as usize;
        let characters: Vec<char> = prompt.editor.text().chars().collect();
        let shown = typed(&characters, prompt.editor.cursor(), room);
        let mut content = vec![Line::default()];
        for (label, value) in prompt.labels.iter().zip(&prompt.values) {
            content.push(Line::from(vec![
                Span::styled(format!("  {label}: "), theme::dim()),
                Span::raw(value.clone()),
            ]));
        }
        content.push(Line::styled(format!("  {}", prompt.label()), theme::dim()));
        content.push(Line::from(vec![
            Span::raw("  "),
            Span::raw(shown.before),
            Span::styled(shown.under, theme::cursor()),
            Span::raw(shown.after),
        ]));
        content.push(Line::default());
        content.push(Line::styled("  ↵ next · esc cancels", theme::dim()));
        Island {
            title: prompt.title.clone(),
            content,
            ..Island::default()
        }
    }

    fn help_lines() -> Vec<Line<'static>> {
        [
            ("keys", ""),
            ("j k  ↵", "move, open a key or fold a namespace"),
            ("h l", "fold or unfold"),
            ("/", "find keys by pattern, such as user:*"),
            ("b", "switch database"),
            ("r", "scan again"),
            ("N", "a new key"),
            ("d", "mark a key for deletion"),
            ("T", "set the TTL"),
            ("R", "rename"),
            ("", ""),
            ("value", ""),
            ("e", "edit the value, field, item, member or score"),
            ("N", "add a field, item or member"),
            ("d", "mark it for removal"),
            ("n p", "next or previous page"),
            ("", ""),
            ("everywhere", ""),
            (":w", "write every pending change at once"),
            ("u", "drop the pending changes"),
            ("s", "run a Redis command"),
            ("ctrl+k", "the command palette"),
            ("q", "quit"),
        ]
        .into_iter()
        .map(|(key, said)| match said.is_empty() {
            true => Line::styled(format!(" {key}"), theme::bold()),
            false => Line::from(vec![
                Span::styled(format!("   {}", pad(key, 10)), theme::bold()),
                Span::raw(said.to_string()),
            ]),
        })
        .collect()
    }

    pub(super) fn draw(&mut self, frame: &mut Frame) {
        if chrome::too_small(frame) {
            return;
        }
        let area = frame.area();
        let (hotkey_bar, under, gap, sidebar_width) = settings::with(|settings| {
            (
                settings.hotkey_bar,
                settings.bottom_margin,
                settings.pane_gap,
                settings.sidebar_width,
            )
        });
        let chrome_lines = if hotkey_bar { 2 } else { 1 };
        let frame_height = area.height.saturating_sub(chrome_lines + under).max(6);
        let left_width =
            (sidebar_width + 8).min((area.width as usize).saturating_sub(30).max(12)) as u16;
        let sidebar = self.sidebar(1, 1, left_width + 2, frame_height);
        sidebar.draw(frame.buffer_mut());

        let right_x = sidebar.x + sidebar.width + 1;
        let right_width = (area.width + 1).saturating_sub(right_x + gap).max(20);
        let mut value_y = 1;
        let mut value_height = frame_height;
        let mut cursor = None;
        if let Some((island, (row, column))) = self.cli_island(right_x, 1, right_width) {
            cursor = Some((island.x + 2 + column as u16, island.y + 1 + row as u16));
            island.draw(frame.buffer_mut());
            value_y += CLI_HEIGHT;
            value_height = value_height.saturating_sub(CLI_HEIGHT).max(5);
        }
        self.value(right_x, value_y, right_width, value_height)
            .draw(frame.buffer_mut());
        if let Some(cursor) = cursor {
            frame.set_cursor_position(cursor);
        }

        if let Some(prompt) = &self.prompt {
            let box_width = area.width.saturating_sub(4).min(64);
            let island = self.prompt_island(prompt, box_width);
            let height = island.content.len() as u16 + 2;
            Modal::new(area.width, 1, frame_height, box_width).draw(
                frame.buffer_mut(),
                island,
                height,
            );
        }
        if let Some(choice) = &self.databases {
            choice.draw_modal(frame.buffer_mut(), area.width, frame_height);
        }
        if let Some((_, choice)) = &self.kinds_choice {
            choice.draw_modal(frame.buffer_mut(), area.width, frame_height);
        }
        if self.help {
            let content = Self::help_lines();
            let height = (content.len() as u16 + 2).min(frame_height.saturating_sub(2));
            Modal::new(
                area.width,
                1,
                frame_height,
                area.width.saturating_sub(4).min(70),
            )
            .draw(
                frame.buffer_mut(),
                Island {
                    title: "HELP".to_string(),
                    content,
                    ..Island::default()
                },
                height,
            );
        }
        if let Some(palette) = &self.palette {
            let width_ = area.width.saturating_sub(4).min(palette::WIDTH);
            let height = frame_height.saturating_sub(2).min(palette.rows());
            let island = Island {
                title: palette.title.clone(),
                content: palette.content(
                    width_.saturating_sub(2) as usize,
                    height.saturating_sub(2) as usize,
                ),
                ..Island::default()
            };
            Modal::new(area.width, 1, frame_height, width_).draw(
                frame.buffer_mut(),
                island,
                height,
            );
        }
        if let Some(modal) = &self.settings
            && let Some(cursor) = modal.draw(frame.buffer_mut(), area.width, frame_height)
        {
            frame.set_cursor_position(cursor);
        }

        if hotkey_bar {
            chrome::line_at(
                frame,
                frame_height,
                chrome::hotkeys(&self.offers(), area.width as usize),
            );
        }
        chrome::line_at(frame, frame_height + chrome_lines - 1, self.status_line());
    }
}
