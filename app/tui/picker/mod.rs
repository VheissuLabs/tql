use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use crate::driver::Driver;
use crate::terminal::prelude::*;

use super::ago::{ago, now};
use super::chrome::{self, theme};
use super::clipboard::{Clipboard, Reached, Recorded};
use super::connection_form::{ConnectionForm, Places, Step};
use super::group_rename::{self, GroupRename};
use super::text::{pad, single_line, truncate, width, window};
use crate::encrypter::Encrypter;
use crate::settings;
use crate::store::{Connection, Store};
use crate::store_writer::StoreWriter;
use crate::updates::remembered::Remembered;

mod command;
mod editing;
mod groups;
mod palette;
mod rows;
mod yank;

const MIN_LINES: u16 = 10;
const FIRST_BODY_ROW: i64 = 4;
const BAR: &str = " ↑↓ Move    ↵ Open    space Fold    e Edit    n New    y/Y Yank db/DSN    d Delete    :w Confirm    :q Quit";
pub const COLLAPSED: &str = "connections.collapsed_groups";
const FORM_BAR: &str = " ↑↓ Move    ↵ Select";
const PALETTE_BAR: &str = " ↑↓ Move    ↵ Run    esc Close";

pub struct Picker {
    pub connections: Vec<Connection>,
    pub index: usize,
    pub notice: Option<String>,
    pub status: Option<String>,
    pub command: Option<String>,
    updating: Option<crate::tui::updating::Updating>,
    pub form: Option<ConnectionForm>,
    pub renaming: Option<GroupRename>,
    pub palette: Option<super::palette::Palette>,
    pub settings: Option<super::settings_modal::SettingsModal>,
    pub pending_deletes: Vec<i64>,
    pub collapsed: Vec<String>,
    start: usize,
    store: Option<(PathBuf, PathBuf)>,
    places: Places,
    clipboard: Box<dyn Clipboard>,
    status_since: Instant,
}

enum Entry {
    Header {
        group: String,
        count: usize,
        collapsed: bool,
    },
    Connection {
        index: usize,
        group: String,
    },
}

struct Header {
    group: String,
    count: usize,
    collapsed: bool,
}

struct Row {
    header: Option<Header>,
    grouped: bool,
    name: String,
    tag: String,
    tag_color: Option<Color>,
    driver: Option<Driver>,
    place: String,
    used: String,
    marked: bool,
}

impl Picker {
    pub fn new(connections: Vec<Connection>, notice: Option<String>) -> Self {
        let mut picker = Picker {
            connections,
            index: 0,
            notice,
            status: None,
            command: None,
            updating: None,
            form: None,
            renaming: None,
            palette: None,
            settings: None,
            pending_deletes: Vec::new(),
            collapsed: Vec::new(),
            start: 0,
            store: None,
            places: Places::here(),
            clipboard: Box::new(Recorded::default()),
            status_since: Instant::now(),
        };
        picker.start_on_the_first();
        picker
    }

    pub fn with_store(mut self, store: &Path, key: &Path) -> Self {
        self.store = Some((store.to_path_buf(), key.to_path_buf()));
        self.collapsed = Remembered::new(store)
            .read(COLLAPSED)
            .and_then(|groups| serde_json::from_str(&groups).ok())
            .unwrap_or_default();
        self.start_on_the_first();
        self
    }

    pub fn with_clipboard(mut self, clipboard: impl Clipboard + 'static) -> Self {
        self.clipboard = Box::new(clipboard);
        self
    }

    fn start_on_the_first(&mut self) {
        let first = self.connections.first().map(|connection| connection.id);
        self.index = self.index_of(first).unwrap_or(0);
    }

    fn say(&mut self, status: impl Into<String>) {
        let status = status.into();
        if self.status.as_deref() != Some(status.as_str()) {
            self.status_since = Instant::now();
        }
        self.status = Some(status);
    }

    pub fn fade_status(&mut self, now: Instant) -> bool {
        let after = settings::with(|settings| settings.status_seconds);
        if after <= 0.0 || self.status.as_deref().is_none_or(str::is_empty) {
            return false;
        }
        if now.saturating_duration_since(self.status_since) < Duration::from_secs_f64(after) {
            return false;
        }
        self.status = None;
        true
    }

    fn select(&mut self) -> Flow<Option<Connection>> {
        if let Some(Entry::Header { .. }) = self.entries(false).get(self.index) {
            self.fold(None);
            return Flow::Continue;
        }
        match self.current() {
            Some(connection) => Flow::Exit(Some(connection.clone())),
            None if self.connections.is_empty() => Flow::Exit(None),
            None => Flow::Continue,
        }
    }

    pub fn with_places(mut self, places: Places) -> Self {
        self.places = places;
        self
    }

    pub fn is_marked(&self, id: i64) -> bool {
        self.pending_deletes.contains(&id)
    }

    fn writer(&self) -> anyhow::Result<StoreWriter> {
        let Some((store, key)) = &self.store else {
            anyhow::bail!("there is no tql store to write to");
        };
        StoreWriter::open(store, key)
    }

    fn reload(&mut self) -> anyhow::Result<()> {
        let Some((store, _)) = &self.store else {
            return Ok(());
        };
        self.connections = Store::open(store)?.connections()?;
        Ok(())
    }

    fn move_by(&mut self, steps: isize) {
        let last = self.entries(false).len().saturating_sub(1);
        self.index = self.index.saturating_add_signed(steps).min(last);
    }

    fn quit(&mut self) -> Flow<Option<Connection>> {
        if self.pending_deletes.is_empty() {
            return Flow::Exit(None);
        }
        let count = self.pending_deletes.len();
        self.pending_deletes.clear();
        self.say(format!(
            "{count} unwritten mark{} dropped, quit again to leave",
            if count == 1 { "" } else { "s" }
        ));
        Flow::Continue
    }
}

impl Tui for Picker {
    type Output = Option<Connection>;

    fn on_key(&mut self, key: Key) -> Flow<Option<Connection>> {
        if self.form.is_some() {
            self.on_form_key(key);
            return Flow::Continue;
        }
        if self.renaming.is_some() {
            self.on_rename_key(key);
            return Flow::Continue;
        }
        if let Some(modal) = self.settings.as_mut() {
            if !modal.on_key(key) {
                self.settings = None;
            }
            return Flow::Continue;
        }

        if self.command.is_some() {
            return self.type_into_command(key);
        }
        if self.palette.is_some() {
            return self.type_into_palette(key);
        }
        if Self::opens_palette(key) {
            self.open_palette();
            return Flow::Continue;
        }

        match key {
            Key::Char(':') => self.command = Some(String::new()),
            Key::Ctrl('c') => return Flow::Exit(None),
            Key::Char('q') | Key::Escape => return self.quit(),
            Key::Char('n') => self.create(),
            Key::Char('e') => self.edit(),
            Key::Char('d') => self.mark(),
            Key::Char('u') => self.unmark_all(),
            Key::Char('y') => self.yank_database(),
            Key::Char('Y') => self.yank_connection_string(),
            Key::Up | Key::Char('k') => self.move_by(-1),
            Key::Down | Key::Char('j') => self.move_by(1),
            Key::Left | Key::Char('h') => self.fold(Some(true)),
            Key::Right | Key::Char('l') => self.fold(Some(false)),
            Key::Char(' ') => self.fold(None),
            Key::Enter => return self.select(),
            _ => {}
        }
        Flow::Continue
    }

    fn on_paste(&mut self, text: &str) -> Flow<Option<Connection>> {
        if let Some(modal) = self.settings.as_mut() {
            modal.on_paste(text);
        } else if let Some(form) = self.form.as_mut() {
            form.on_paste(text);
        } else if let Some(renaming) = self.renaming.as_mut() {
            renaming.on_paste(text);
        } else if let Some(command) = self.command.as_mut() {
            command.push_str(&single_line(text));
        }
        Flow::Continue
    }

    fn wants_mouse(&self) -> bool {
        crate::settings::with(|settings| settings.mouse)
    }

    fn cursor_shape(&self) -> CursorShape {
        match self.settings.as_ref().and_then(|modal| modal.editing()) {
            Some(_) => CursorShape::Bar,
            None => CursorShape::Default,
        }
    }

    fn on_mouse(&mut self, mouse: Mouse) -> Flow<Option<Connection>> {
        if self.form.is_some()
            || self.renaming.is_some()
            || self.palette.is_some()
            || self.settings.is_some()
            || !self.wants_mouse()
        {
            return Flow::Continue;
        }
        let count = self.entries(false).len();
        match mouse.button {
            Button::WheelUp => self.index = self.index.saturating_sub(1),
            Button::WheelDown => self.index = (self.index + 1).min(count.saturating_sub(1)),
            Button::Left => {
                let row_offset = crate::settings::with(|settings| settings.mouse_row_offset);
                let row = mouse.row as i64 - row_offset;
                let target = self.start as i64 + row - FIRST_BODY_ROW;
                if row >= FIRST_BODY_ROW && (target as usize) < count {
                    self.index = target as usize;
                    return self.select();
                }
            }
            _ => {}
        }
        Flow::Continue
    }

    fn idle(&mut self) -> bool {
        self.hear_from_update() || self.fade_status(Instant::now())
    }

    fn top_margin(&self) -> u16 {
        settings::with(|settings| settings.top_margin)
    }

    fn smallest(&self) -> (u16, u16) {
        (chrome::MIN_COLUMNS, MIN_LINES)
    }

    fn left_margin(&self) -> u16 {
        settings::with(|settings| settings.left_margin)
    }

    fn right_margin(&self) -> u16 {
        settings::with(|settings| settings.right_margin)
    }

    fn render(&mut self, frame: &mut Frame) {
        let area = frame.area();
        if area.width < chrome::MIN_COLUMNS || area.height < MIN_LINES {
            let message = [
                "Make the window bigger".to_string(),
                format!("tql needs {}×{MIN_LINES}", chrome::MIN_COLUMNS),
                format!("this is {}×{}", area.width, area.height),
            ];
            for (row, text) in message.iter().enumerate().take(area.height as usize) {
                let text: String = text.chars().take(area.width as usize).collect();
                chrome::line_at(frame, row as u16, Line::raw(text));
            }
            return;
        }

        let inner = area.width.saturating_sub(2) as usize;
        let under = settings::with(|settings| settings.bottom_margin) as usize;
        let body_height = (area.height as usize).saturating_sub(6 + under).max(3);
        let rows = self.rows(false);
        let widths = Self::widths(&self.rows(true), inner);
        let edge = theme::border(true, false);
        let boxed = |content: Line<'static>| {
            let padding = inner.saturating_sub(content.width());
            let style = content.style;
            let mut spans = vec![Span::styled("│", edge)];
            spans.extend(
                content
                    .spans
                    .into_iter()
                    .map(|span| Span::styled(span.content, style.patch(span.style))),
            );
            spans.push(Span::styled(" ".repeat(padding), style));
            spans.push(Span::styled("│", edge));
            Line::from(spans)
        };

        let title = " CONNECTIONS ";
        let tail: String = Self::rule(&widths, '┬', inner)
            .chars()
            .skip(width(title) + 1)
            .collect();
        let mut lines = vec![Line::from(vec![
            Span::styled("╭─", edge),
            Span::styled(title, theme::title(true, false)),
            Span::styled(format!("{tail}╮"), edge),
        ])];

        let labels = ["NAME", "TAG", "WHERE", "LAST USED"];
        let mut header = Vec::new();
        for (index, (label, width)) in labels.iter().zip(widths).enumerate() {
            if index > 0 {
                header.push(Span::styled("│", edge));
            }
            header.push(Span::styled(
                format!(" {} ", pad(&truncate(label, width), width)),
                theme::bold(),
            ));
        }
        lines.push(boxed(Line::from(header)));
        lines.push(Line::styled(
            format!("├{}┤", Self::rule(&widths, '┼', inner)),
            edge,
        ));

        let blank: Line<'static> = {
            let mut spans = Vec::new();
            for (index, width) in widths.iter().enumerate() {
                if index > 0 {
                    spans.push(Span::styled("│", edge));
                }
                spans.push(Span::raw(" ".repeat(width + 2)));
            }
            Line::from(spans)
        };

        let body: Vec<Line<'static>> = if rows.is_empty() {
            vec![Line::styled(
                "  No connections yet. Press n to add one.",
                theme::dim(),
            )]
        } else {
            let start = window(self.index, rows.len(), body_height);
            self.start = start;
            rows.iter()
                .enumerate()
                .skip(start)
                .take(body_height)
                .map(|(position, row)| self.row_line(row, position == self.index, &widths))
                .collect()
        };
        for row in 0..body_height {
            lines.push(boxed(
                body.get(row).cloned().unwrap_or_else(|| blank.clone()),
            ));
        }
        lines.push(Line::styled(
            format!("╰{}╯", Self::rule(&widths, '┴', inner)),
            edge,
        ));
        let frame_lines = lines.len() as u16;
        let bar = match (&self.form, &self.renaming, &self.palette) {
            _ if self.settings.is_some() => super::settings_modal::BAR,
            (Some(_), _, _) => FORM_BAR,
            (None, Some(_), _) => group_rename::BAR,
            (None, None, Some(_)) => PALETTE_BAR,
            (None, None, None) => BAR,
        };
        lines.push(Line::styled(
            truncate(bar, area.width as usize),
            theme::dim(),
        ));
        lines.push(self.status_line());

        for (row, line) in lines.into_iter().enumerate() {
            chrome::line_at(frame, row as u16, line);
        }

        if let Some(form) = &self.form {
            form.draw(frame.buffer_mut(), area.width, frame_lines);
        }
        if let Some(renaming) = &self.renaming {
            renaming.draw(frame.buffer_mut(), area.width, frame_lines);
        }
        self.draw_palette(frame.buffer_mut(), area.width, frame_lines);
        if let Some(modal) = &self.settings
            && let Some(cursor) = modal.draw(frame.buffer_mut(), area.width, frame_lines)
        {
            frame.set_cursor_position(cursor);
        }
    }
}
