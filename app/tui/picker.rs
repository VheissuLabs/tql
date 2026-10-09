use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use crate::terminal::prelude::*;

use super::ago::{ago, now};
use super::chrome::{self, theme};
use super::clipboard::{Clipboard, Reached, Recorded};
use super::connection_form::{ConnectionForm, Places, Step, single_line};
use super::text::{pad, truncate, width, window};
use crate::encrypter::Encrypter;
use crate::settings;
use crate::store::{Connection, Store};
use crate::store_writer::StoreWriter;
use crate::updates::remembered::Remembered;

const MIN_LINES: u16 = 10;
const FIRST_BODY_ROW: i64 = 4;
const BAR: &str = " ↑↓ Move    ↵ Open    space Fold    e Edit    n New    y/Y Yank db/DSN    d Delete    :w Confirm    :q Quit";
pub const COLLAPSED: &str = "connections.collapsed_groups";
const FORM_BAR: &str = " ↑↓ Move    ↵ Select";

pub struct Picker {
    pub connections: Vec<Connection>,
    pub index: usize,
    pub notice: Option<String>,
    pub status: Option<String>,
    pub command: Option<String>,
    pub form: Option<ConnectionForm>,
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
    driver: String,
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
            form: None,
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
        self.status = Some(status.into());
        self.status_since = Instant::now();
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

    fn entries(&self, unfolded: bool) -> Vec<Entry> {
        let mut ungrouped = Vec::new();
        let mut groups: Vec<(String, Vec<usize>)> = Vec::new();
        for (index, connection) in self.connections.iter().enumerate() {
            let group = connection.group_name.clone().unwrap_or_default();
            if group.is_empty() {
                ungrouped.push(Entry::Connection {
                    index,
                    group: String::new(),
                });
                continue;
            }
            match groups.iter_mut().find(|(name, _)| *name == group) {
                Some((_, members)) => members.push(index),
                None => groups.push((group, vec![index])),
            }
        }
        groups.sort_by_key(|(name, _)| name.to_lowercase());

        let mut entries = ungrouped;
        for (group, members) in groups {
            let collapsed = !unfolded && self.is_collapsed(&group);
            entries.push(Entry::Header {
                group: group.clone(),
                count: members.len(),
                collapsed,
            });
            if !collapsed {
                entries.extend(members.into_iter().map(|index| Entry::Connection {
                    index,
                    group: group.clone(),
                }));
            }
        }
        entries
    }

    pub fn is_collapsed(&self, group: &str) -> bool {
        self.collapsed.iter().any(|collapsed| collapsed == group)
    }

    fn current(&self) -> Option<&Connection> {
        match self.entries(false).get(self.index)? {
            Entry::Connection { index, .. } => self.connections.get(*index),
            Entry::Header { .. } => None,
        }
    }

    fn index_of(&self, id: Option<i64>) -> Option<usize> {
        self.entries(false).iter().position(|entry| {
            matches!(entry, Entry::Connection { index, .. } if Some(self.connections[*index].id) == id)
        })
    }

    fn header_index(&self, wanted: &str) -> usize {
        self.entries(false)
            .iter()
            .position(|entry| matches!(entry, Entry::Header { group, .. } if group == wanted))
            .unwrap_or(self.index)
    }

    fn groups(&self) -> Vec<String> {
        let mut groups: Vec<String> = Vec::new();
        for group in self
            .connections
            .iter()
            .filter_map(|connection| connection.group_name.clone())
        {
            if !groups.contains(&group) {
                groups.push(group);
            }
        }
        groups.sort_by_key(|group| group.to_lowercase());
        groups
    }

    fn remember_collapsed(&self) {
        if let Some((store, _)) = &self.store {
            Remembered::new(store).write(
                COLLAPSED,
                &serde_json::to_string(&self.collapsed).unwrap_or_default(),
            );
        }
    }

    fn fold(&mut self, shut: Option<bool>) {
        let (group, header) = match self.entries(false).get(self.index) {
            Some(Entry::Header { group, .. }) => (group.clone(), true),
            Some(Entry::Connection { group, .. }) => (group.clone(), false),
            None => return,
        };
        if group.is_empty() || (!header && shut != Some(true)) {
            return;
        }

        let shut = shut.unwrap_or(!self.is_collapsed(&group));
        self.collapsed.retain(|collapsed| *collapsed != group);
        if shut {
            self.collapsed.push(group.clone());
        }
        self.remember_collapsed();
        self.index = self.header_index(&group);
    }

    fn select(&mut self) -> Flow<Option<Connection>> {
        if let Some(Entry::Header { .. }) = self.entries(false).get(self.index) {
            self.fold(None);
            return Flow::Continue;
        }
        match self.current() {
            Some(connection) => Flow::Exit(Some(connection.clone())),
            None => Flow::Continue,
        }
    }

    fn yank_database(&mut self) {
        let Some(connection) = self.current() else {
            return;
        };
        let database = connection.database.clone().unwrap_or_default();
        if database.is_empty() {
            let status = format!(
                "{} has no database set, so it asks on connect",
                connection.name
            );
            self.say(status);
            return;
        }
        self.yanked(&database, &database);
    }

    fn yank_connection_string(&mut self) {
        let Some(connection) = self.current() else {
            return;
        };
        let password = connection.password.as_ref().and_then(|payload| {
            let (_, key) = self.store.as_ref()?;
            Encrypter::from_file(key).ok()?.decrypt_string(payload).ok()
        });
        let text = connection.connection_string(password.as_deref());
        self.yanked(&text, "connection string");
    }

    fn yanked(&mut self, text: &str, what: &str) {
        let reached = self.clipboard.copy(text);
        let place = match reached {
            Reached::Terminal => " to the terminal clipboard",
            Reached::System => "",
        };
        self.say(format!("yanked {what}{place}"));
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

    fn create(&mut self) {
        self.form = Some(ConnectionForm::creating(self.places.clone()).with_groups(self.groups()));
        self.status = None;
    }

    fn edit(&mut self) {
        let Some(connection) = self.current() else {
            return;
        };
        let encrypter = self
            .store
            .as_ref()
            .and_then(|(_, key)| Encrypter::from_file(key).ok());
        self.form = Some(
            ConnectionForm::editing(connection, encrypter.as_ref(), self.places.clone())
                .with_groups(self.groups()),
        );
        self.status = None;
    }

    fn mark(&mut self) {
        let Some(id) = self.current().map(|connection| connection.id) else {
            return;
        };
        match self.pending_deletes.iter().position(|marked| *marked == id) {
            Some(position) => {
                self.pending_deletes.remove(position);
            }
            None => self.pending_deletes.push(id),
        }
        self.move_by(1);
        let count = self.pending_deletes.len();
        self.say(match count {
            0 => "marks cleared".to_string(),
            count => format!("{count} marked for deletion · :w writes · u clears"),
        });
    }

    fn unmark_all(&mut self) {
        self.pending_deletes.clear();
        self.say("marks cleared");
    }

    fn write_pending(&mut self) {
        if self.pending_deletes.is_empty() {
            self.say("nothing to write");
            return;
        }
        let count = self.pending_deletes.len();
        let deleted = self
            .writer()
            .and_then(|writer| writer.delete(&self.pending_deletes))
            .and_then(|_| self.reload());
        if let Err(error) = deleted {
            self.say(format!("{error:#}"));
            return;
        }
        self.pending_deletes.clear();
        self.index = self.index.min(self.entries(false).len().saturating_sub(1));
        self.say(format!(
            "deleted {count} connection{}",
            if count == 1 { "" } else { "s" }
        ));
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

    fn on_form_key(&mut self, key: Key) {
        let Some(form) = self.form.as_mut() else {
            return;
        };
        match form.on_key(key) {
            Step::Stay => {}
            Step::Cancel => self.close_form("nothing changed".to_string()),
            Step::Save => self.save_form(),
        }
    }

    fn save_form(&mut self) {
        let writer = self.writer();
        let Some(form) = self.form.as_mut() else {
            return;
        };
        let saved = writer.and_then(|writer| form.save(&writer));
        let refusal = match saved {
            Ok(refusal) => refusal,
            Err(error) => Some(format!("{error:#}")),
        };
        if let Some(refusal) = refusal {
            form.error = Some(refusal);
            return;
        }

        let creating = form.creating;
        let id = form.id;
        let name = form.name().to_string();
        let group = form.value("group_name").trim().to_string();
        if let Err(error) = self.reload() {
            self.say(format!("{error:#}"));
        }
        if !group.is_empty() && self.is_collapsed(&group) {
            self.collapsed.retain(|collapsed| *collapsed != group);
            self.remember_collapsed();
        }
        self.index = self.index_of(id).unwrap_or(0);
        self.close_form(format!(
            "{} {name}",
            if creating { "added" } else { "saved" }
        ));
    }

    fn close_form(&mut self, status: String) {
        self.form = None;
        self.say(status);
    }

    fn rows(&self, unfolded: bool) -> Vec<Row> {
        let now = now();
        self.entries(unfolded)
            .into_iter()
            .map(|entry| {
                let (index, group) = match entry {
                    Entry::Header {
                        group,
                        count,
                        collapsed,
                    } => {
                        return Row {
                            header: Some(Header {
                                group,
                                count,
                                collapsed,
                            }),
                            grouped: false,
                            name: String::new(),
                            tag: String::new(),
                            tag_color: None,
                            driver: String::new(),
                            place: String::new(),
                            used: String::new(),
                            marked: false,
                        };
                    }
                    Entry::Connection { index, group } => (index, group),
                };
                let connection = &self.connections[index];
                let tag = connection
                    .tag
                    .clone()
                    .unwrap_or_default()
                    .trim()
                    .to_string();
                Row {
                    header: None,
                    grouped: !group.is_empty(),
                    name: connection.name.clone(),
                    tag: if connection.read_only {
                        format!("{tag} ro").trim().to_string()
                    } else {
                        tag
                    },
                    tag_color: connection.tag_color().and_then(theme::named),
                    driver: connection.driver.clone(),
                    place: connection.describe_from(&self.places.home),
                    used: ago(connection.last_used_at.as_deref(), now),
                    marked: self.is_marked(connection.id),
                }
            })
            .collect()
    }

    fn widths(rows: &[Row], inner: usize) -> [usize; 4] {
        let count = |text: &str| text.chars().count();
        let mut name = rows
            .iter()
            .map(|row| match &row.header {
                Some(header) => count(&header_text(header)) - 1,
                None => count(&row.name) + 4 + if row.grouped { 2 } else { 0 },
            })
            .fold(8, usize::max)
            .min(32);
        let connections = rows.iter().filter(|row| row.header.is_none());
        let mut tag = connections
            .clone()
            .map(|row| count(&row.tag))
            .fold(3, usize::max)
            .min(16);
        let mut used = connections
            .map(|row| count(&row.used))
            .fold(9, usize::max)
            .min(20);

        let over = (name + tag + used + 21).saturating_sub(inner);
        let used_gives = over.min(used - 9);
        let tag_gives = (over - used_gives).min(tag - 3);
        let name_gives = (over - used_gives - tag_gives).min(name - 8);
        used -= used_gives;
        tag -= tag_gives;
        name -= name_gives;

        let place = inner.saturating_sub(name + tag + used + 11).max(10);
        [name, tag, place, used]
    }

    fn rule(widths: &[usize; 4], join: char, inner: usize) -> String {
        let body = widths
            .iter()
            .map(|width| "─".repeat(width + 2))
            .collect::<Vec<_>>()
            .join(&join.to_string());
        let used = body.chars().count();
        format!("{body}{}", "─".repeat(inner.saturating_sub(used)))
    }

    fn header_line(header: &Header, selected: bool, widths: &[usize; 4]) -> Line<'static> {
        let grid = theme::border(true, false);
        let arrow = if header.collapsed { "▶" } else { "▼" };
        let count = format!("({})", header.count);
        let room = widths[0] + 1;
        let name = truncate(&header.group, room.saturating_sub(width(&count) + 3).max(1));
        let shown = format!(" {arrow} {name} {count}");
        let plain = format!("{} ", pad(&shown, room));
        let rest: Vec<String> = widths[1..]
            .iter()
            .map(|width| " ".repeat(width + 2))
            .collect();

        if selected {
            let room = widths.iter().sum::<usize>() + 3 * widths.len() - 1;
            let text = std::iter::once(plain)
                .chain(rest)
                .collect::<Vec<_>>()
                .join("│");
            return Line::styled(pad(&text, room), theme::selection());
        }

        let mut spans = vec![
            Span::raw(" "),
            Span::styled(arrow, theme::dim()),
            Span::raw(" "),
            Span::styled(name, theme::bold()),
            Span::raw(" "),
            Span::styled(count, theme::dim()),
            Span::raw(" ".repeat(width(&plain).saturating_sub(width(&shown)))),
        ];
        for cell in rest {
            spans.push(Span::styled("│", grid));
            spans.push(Span::raw(cell));
        }
        Line::from(spans)
    }

    fn row_line(&self, row: &Row, selected: bool, widths: &[usize; 4]) -> Line<'static> {
        if let Some(header) = &row.header {
            return Self::header_line(header, selected, widths);
        }
        let grid = theme::border(true, false);
        let icon = icon(&row.driver);
        let marker = match selected && settings::with(|settings| settings.row_style == "marker") {
            true => "▸",
            false => " ",
        };
        let indent = if row.grouped { "  " } else { "" };
        let room = widths[0].saturating_sub(4 + indent.len());
        let label = pad(&truncate(&row.name, room), room);
        let cell = |text: &str, width: usize| format!(" {} ", pad(&truncate(text, width), width));

        if selected || row.marked {
            let text = format!(
                " {marker} {indent}{icon} {label} │{}│{}│{}",
                cell(&row.tag, widths[1]),
                cell(&row.place, widths[2]),
                cell(&row.used, widths[3])
            );
            let room = widths.iter().sum::<usize>() + 3 * widths.len() - 1;
            let style = match row.marked {
                true => {
                    theme::highlight(&settings::with(|settings| settings.color("deleted", "red")))
                }
                false => theme::selection(),
            };
            return Line::styled(pad(&text, room), style);
        }

        let shade = row.tag_color.unwrap_or_else(|| driver_color(&row.driver));
        let tag_style = row
            .tag_color
            .map(|color| Style::new().fg(color))
            .unwrap_or_else(theme::dim);
        Line::from(vec![
            Span::raw(format!(" {marker} {indent}")),
            Span::styled(icon.to_string(), Style::new().fg(shade)),
            Span::raw(format!(" {label} ")),
            Span::styled("│", grid),
            Span::styled(cell(&row.tag, widths[1]), tag_style),
            Span::styled("│", grid),
            Span::styled(cell(&row.place, widths[2]), theme::dim()),
            Span::styled("│", grid),
            Span::styled(cell(&row.used, widths[3]), theme::dim()),
        ])
    }

    fn status_line(&self) -> Line<'static> {
        if let Some(command) = &self.command {
            return Line::raw(format!(" :{command}█"));
        }
        let count = format!("{} connections", self.connections.len());
        match (&self.status, &self.notice) {
            (Some(status), _) => Line::styled(format!(" {status}"), theme::dim()),
            (None, None) => Line::styled(format!(" {count}"), theme::dim()),
            (None, Some(notice)) => Line::from(vec![
                Span::styled(format!(" {count} · "), theme::dim()),
                Span::styled(notice.clone(), Style::new().fg(Color::Cyan)),
            ]),
        }
    }

    fn type_into_command(&mut self, key: Key) -> Flow<Option<Connection>> {
        let Some(command) = self.command.as_mut() else {
            return Flow::Continue;
        };
        match key {
            Key::Escape => self.command = None,
            Key::Enter => {
                let command = command.trim().to_string();
                self.command = None;
                match command.as_str() {
                    "q" | "q!" | "quit" => return self.quit(),
                    "w" | "write" => self.write_pending(),
                    "new" => self.create(),
                    other => self.say(format!("unknown command: :{other}")),
                }
            }
            Key::Backspace if command.is_empty() => self.command = None,
            Key::Backspace => {
                command.pop();
            }
            Key::Char(character) => command.push(character),
            _ => {}
        }
        Flow::Continue
    }
}

fn header_text(header: &Header) -> String {
    format!(" ▼ {} ({})", header.group, header.count)
}

fn icon(driver: &str) -> char {
    match driver {
        "mysql" | "mariadb" => '\u{e704}',
        "pgsql" => '\u{e76e}',
        "sqlite" => '\u{e7c4}',
        _ => '\u{f1c0}',
    }
}

fn driver_color(driver: &str) -> Color {
    match driver {
        "mysql" | "mariadb" => Color::Yellow,
        "pgsql" => Color::Blue,
        "sqlite" => Color::Green,
        "sqlsrv" => Color::Magenta,
        _ => Color::DarkGray,
    }
}

impl Tui for Picker {
    type Output = Option<Connection>;

    fn on_key(&mut self, key: Key) -> Flow<Option<Connection>> {
        if self.form.is_some() {
            self.on_form_key(key);
            return Flow::Continue;
        }

        if self.command.is_some() {
            return self.type_into_command(key);
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
        if let Some(form) = self.form.as_mut() {
            form.on_paste(text);
        } else if let Some(command) = self.command.as_mut() {
            command.push_str(&single_line(text));
        }
        Flow::Continue
    }

    fn wants_mouse(&self) -> bool {
        crate::settings::with(|settings| settings.mouse)
    }

    fn on_mouse(&mut self, mouse: Mouse) -> Flow<Option<Connection>> {
        if self.form.is_some() {
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
        self.fade_status(Instant::now())
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
        let body_height = (area.height as usize).saturating_sub(7).max(3);
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
        let bar = if self.form.is_some() { FORM_BAR } else { BAR };
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
    }
}
