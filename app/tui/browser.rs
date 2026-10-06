use std::collections::HashMap;
use std::path::Path;
use std::time::{Duration, Instant};

use zerorust_tui::prelude::*;

use super::choice::{self, Choice};
use super::chrome::{self, Island, Modal, theme};
use super::grid::{self, Grid};
use super::help;
use super::keymap::{self, Action};
use super::sql;
use super::text::{self, pad, truncate, window};
use crate::database::{Database, Direction, Sort};
use crate::settings;
use crate::store::Connection;

pub const PAGE: usize = 100;
const BOTTOM_MARGIN: u16 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Exit {
    Quit,
    Connections,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    Sidebar,
    Grid,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Browse,
    Help,
}

pub struct Browser {
    pub connection: Connection,
    database: Box<dyn Database>,
    pub tables: Vec<String>,
    pub table_index: usize,
    pub filter: String,
    pub filtering: bool,
    pub tables_hidden: bool,
    pub sidebar_width: Option<usize>,
    pub focus: Focus,
    pub mode: Mode,
    pub headers: Vec<String>,
    pub rows: Vec<Vec<String>>,
    pub row_index: usize,
    pub column_index: usize,
    pub column_offset: usize,
    pub width_overrides: HashMap<String, usize>,
    pub offset: usize,
    pub has_more: bool,
    pub sort: Option<Sort>,
    status: Option<String>,
    status_since: Instant,
    status_fresh: bool,
    pub command: Option<String>,
    pub problem: Option<String>,
    pub active_database: Option<String>,
    pub databases: Option<Choice>,
    pub last_statement: Option<String>,
    hits: Hits,
    drag: Option<Drag>,
    last_click: Option<(usize, usize, Instant)>,
    help_offset: usize,
    help_hidden: usize,
    keys: HashMap<String, String>,
    pending: Option<bool>,
}

impl Browser {
    pub fn new(connection: Connection, mut database: Box<dyn Database>) -> anyhow::Result<Self> {
        let active_database = connection
            .database
            .clone()
            .filter(|database| !database.trim().is_empty());
        let undecided = connection.driver != "sqlite" && active_database.is_none();
        let (tables, keys) = if undecided {
            (Vec::new(), HashMap::new())
        } else {
            (
                database.tables()?,
                database.primary_keys().unwrap_or_default(),
            )
        };
        let mut browser = Browser {
            connection,
            database,
            tables,
            table_index: 0,
            filter: String::new(),
            filtering: false,
            tables_hidden: false,
            sidebar_width: None,
            focus: Focus::Sidebar,
            mode: Mode::Browse,
            headers: Vec::new(),
            rows: Vec::new(),
            row_index: 0,
            column_index: 0,
            column_offset: 0,
            width_overrides: HashMap::new(),
            offset: 0,
            has_more: false,
            sort: None,
            status: None,
            status_since: Instant::now(),
            status_fresh: false,
            command: None,
            problem: None,
            active_database,
            databases: None,
            last_statement: None,
            hits: Hits::default(),
            drag: None,
            last_click: None,
            help_offset: 0,
            help_hidden: 0,
            keys,
            pending: None,
        };
        if undecided {
            browser.open_databases();
        } else if !browser.tables.is_empty() {
            browser.load(false);
        }
        Ok(browser)
    }

    pub fn status(&self) -> &str {
        self.status.as_deref().unwrap_or_default()
    }

    fn say(&mut self, text: impl Into<String>) {
        self.status = Some(text.into());
        self.status_since = Instant::now();
        self.status_fresh = true;
    }

    fn hush(&mut self) {
        self.status = None;
        self.status_fresh = false;
    }

    pub fn visible_tables(&self) -> Vec<&String> {
        let needle = self.filter.to_lowercase();
        self.tables
            .iter()
            .filter(|table| table.to_lowercase().contains(&needle))
            .collect()
    }

    pub fn current_table(&self) -> Option<String> {
        self.visible_tables()
            .get(self.table_index)
            .map(|table| table.to_string())
    }

    pub fn tables_width(&self) -> usize {
        self.sidebar_width
            .unwrap_or_else(|| settings::with(|settings| settings.sidebar_width))
    }

    fn default_sort(&self) -> Option<Sort> {
        let column = self.keys.get(&self.current_table()?)?.clone();
        Some(Sort {
            column,
            direction: Direction::Asc,
        })
    }

    fn queue_load(&mut self, keep_cursor: bool) {
        self.pending = Some(keep_cursor || self.pending.unwrap_or(false));
    }

    fn load(&mut self, keep_cursor: bool) {
        self.pending = None;
        let Some(table) = self.current_table() else {
            return;
        };
        if self.sort.is_none() {
            self.sort = self.default_sort();
        }

        let column = self.headers.get(self.column_index).cloned();
        let row = self.row_index;

        let started = Instant::now();
        let page = match self
            .database
            .rows(&table, PAGE + 1, self.offset, self.sort.as_ref())
        {
            Ok(page) => page,
            Err(error) => {
                self.say(error.to_string());
                self.headers.clear();
                self.rows.clear();
                return;
            }
        };
        let elapsed = started.elapsed().as_millis();

        let mut rows = page.rows;
        self.has_more = rows.len() > PAGE;
        rows.truncate(PAGE);

        self.last_statement = Some(sql::shown(&page.statement, PAGE));
        self.headers = page.headers;
        self.rows = rows
            .iter()
            .map(|row| {
                row.iter()
                    .map(|value| text::cell(value.as_deref()))
                    .collect()
            })
            .collect();
        self.row_index = 0;
        self.column_index = 0;
        self.column_offset = 0;

        if keep_cursor {
            self.column_index = column
                .and_then(|column| self.headers.iter().position(|header| *header == column))
                .unwrap_or(0);
            self.row_index = row.min(self.rows.len().saturating_sub(1));
        }

        let first = if self.rows.is_empty() {
            0
        } else {
            self.offset + 1
        };
        let more = if self.has_more { "+" } else { "" };
        self.say(format!(
            "rows {first}-{}{more} · {elapsed}ms",
            self.offset + self.rows.len()
        ));
    }

    fn select_table(&mut self, index: usize) {
        if index == self.table_index {
            return;
        }
        self.table_index = index;
        self.offset = 0;
        self.sort = None;
        self.queue_load(false);
    }

    fn reselect(&mut self) {
        let count = self.visible_tables().len();
        if count == 0 {
            return;
        }
        self.table_index = self.table_index.min(count - 1);
        self.offset = 0;
        self.sort = None;
        self.queue_load(false);
    }

    fn sort_by_current_column(&mut self) {
        if let Some(column) = self.headers.get(self.column_index).cloned() {
            self.sort_by(column);
        }
    }

    fn sort_by(&mut self, column: String) {
        self.sort = match self.sort.take() {
            Some(sort) if sort.column == column && sort.direction == Direction::Asc => Some(Sort {
                column,
                direction: Direction::Desc,
            }),
            Some(sort) if sort.column == column => self.default_sort(),
            _ => Some(Sort {
                column,
                direction: Direction::Asc,
            }),
        };

        self.offset = 0;
        self.load(true);
        let said = match &self.sort {
            Some(sort) => format!("sorted by {} {}", sort.column, sort.direction.keyword()),
            None => "sort cleared".to_string(),
        };
        self.say(said);
    }

    fn page(&mut self, forward: bool) {
        if forward && !self.has_more || !forward && self.offset == 0 {
            return;
        }
        self.offset = if forward {
            self.offset + PAGE
        } else {
            self.offset.saturating_sub(PAGE)
        };
        self.load(false);
    }

    fn reload(&mut self) {
        let table = self.current_table();
        self.keys = self.database.primary_keys().unwrap_or_default();
        match self.database.tables() {
            Ok(tables) => self.tables = tables,
            Err(error) => {
                self.problem = Some(error.to_string());
                return;
            }
        }

        let visible = self.visible_tables();
        self.table_index = table
            .and_then(|table| visible.iter().position(|candidate| **candidate == table))
            .unwrap_or_else(|| self.table_index.min(visible.len().saturating_sub(1)));
        self.load(true);
    }

    fn open_databases(&mut self) {
        if self.connection.driver == "sqlite" {
            self.say("a sqlite connection is one file");
            return;
        }
        let databases = self.database.databases().unwrap_or_default();
        if databases.is_empty() {
            self.say("no databases to switch to");
            return;
        }
        let chosen = self.active_database.clone().unwrap_or_default();
        self.databases = Some(Choice::new(
            "DATABASE",
            databases,
            &chosen,
            !self.connection.read_only,
        ));
    }

    fn type_into_databases(&mut self, key: Key) {
        let Some(choice) = self.databases.as_mut() else {
            return;
        };
        match key {
            Key::Escape => self.databases = None,
            Key::Up | Key::Ctrl('p') => choice.move_by(false),
            Key::Down | Key::Ctrl('n') => choice.move_by(true),
            Key::Enter if choice.creating() => self.say("creating databases is not ported yet"),
            Key::Ctrl('d') => self.say("dropping databases is not ported yet"),
            Key::Enter => {
                let chosen = choice.selected();
                self.use_database(chosen);
            }
            other => choice.type_key(other),
        }
    }

    fn use_database(&mut self, chosen: Option<String>) {
        self.databases = None;
        let Some(chosen) = chosen else {
            return;
        };
        if self.active_database.as_deref() == Some(chosen.as_str()) {
            return;
        }
        if let Err(error) = self.database.use_database(&chosen) {
            self.say(error.to_string());
            return;
        }

        self.active_database = Some(chosen.clone());
        self.filter.clear();
        self.sort = None;
        self.offset = 0;
        self.table_index = 0;
        self.headers.clear();
        self.rows.clear();
        self.keys = self.database.primary_keys().unwrap_or_default();
        self.tables = match self.database.tables() {
            Ok(tables) => tables,
            Err(error) => {
                self.say(error.to_string());
                return;
            }
        };

        if self.tables.is_empty() {
            self.say(format!("{chosen} has no tables"));
            return;
        }
        self.load(false);
        self.say(format!("using {chosen}"));
    }

    fn toggle_tables(&mut self) {
        self.tables_hidden = !self.tables_hidden;
        if self.tables_hidden && self.focus == Focus::Sidebar {
            self.focus = Focus::Grid;
        }
        if self.tables_hidden {
            let key = keymap::key(Action::ToggleTables, 0);
            self.say(format!("table list hidden · {key} shows it"));
        } else {
            self.hush();
        }
    }

    fn toggle_focus(&mut self, forward: bool) {
        let panes: &[Focus] = if self.tables_hidden {
            &[Focus::Grid]
        } else {
            &[Focus::Sidebar, Focus::Grid]
        };
        let at = panes
            .iter()
            .position(|pane| *pane == self.focus)
            .unwrap_or(0);
        let step = if forward { 1 } else { panes.len() - 1 };
        self.focus = panes[(at + step) % panes.len()];
        self.mode = Mode::Browse;
    }

    fn resize(&mut self, by: Option<isize>) {
        if self.focus == Focus::Sidebar {
            self.sidebar_width =
                by.map(|by| (self.tables_width() as isize + by).clamp(8, 80) as usize);
            self.say(format!("table list width {}", self.tables_width()));
            return;
        }

        let Some(column) = self.headers.get(self.column_index).cloned() else {
            return;
        };
        let Some(by) = by else {
            self.width_overrides.remove(&column);
            self.say(format!("{column} width reset"));
            return;
        };

        let current = self
            .width_overrides
            .get(&column)
            .copied()
            .unwrap_or_else(|| grid::natural_width(&self.headers, &self.rows, self.column_index));
        let width = (current as isize + by).clamp(3, 120) as usize;
        self.width_overrides.insert(column.clone(), width);
        self.say(format!("{column} width {width}"));
    }

    fn run_command(&mut self, command: &str) -> Flow<Exit> {
        self.command = None;
        match command {
            "q" | "q!" | "quit" => return Flow::Exit(Exit::Quit),
            "c" | "connections" => return Flow::Exit(Exit::Connections),
            "tables" => self.focus = Focus::Sidebar,
            "rows" => self.focus = Focus::Grid,
            "r" | "reload" => self.reload(),
            "w" | "write" => self.say("nothing to write"),
            "run" | "sql" | "history" | "export" | "export sql" => {
                self.say(format!(":{command} is not ported yet"));
            }
            _ => self.say(format!("unknown command: :{command}")),
        }
        Flow::Continue
    }

    fn type_into_command(&mut self, key: Key) -> Flow<Exit> {
        let Some(command) = self.command.as_mut() else {
            return Flow::Continue;
        };
        match key {
            Key::Enter => {
                let command = command.trim().to_string();
                return self.run_command(&command);
            }
            Key::Escape => self.command = None,
            Key::Backspace if command.is_empty() => self.command = None,
            Key::Backspace => {
                command.pop();
            }
            Key::Char(character) => command.push(character),
            _ => {}
        }
        Flow::Continue
    }

    fn type_into_filter(&mut self, key: Key) {
        match key {
            Key::Escape => {
                self.filtering = false;
                self.filter.clear();
                self.hush();
                self.reselect();
            }
            Key::Enter => {
                self.filtering = false;
                if self.filter.is_empty() {
                    self.hush();
                } else {
                    let said = format!("filtered by \"{}\"", self.filter);
                    self.say(said);
                }
            }
            Key::Backspace | Key::Ctrl('h') => {
                self.filter.pop();
                self.reselect();
            }
            Key::Char(character) => {
                self.filter.push(character);
                self.reselect();
            }
            _ => {}
        }
    }

    fn type_into_help(&mut self, key: Key) {
        let hidden = self.help_hidden;
        match key {
            Key::Escape | Key::Char('?' | 'q') => {
                self.mode = Mode::Browse;
                self.help_offset = 0;
            }
            Key::Down | Key::Char('j') => self.help_offset = (self.help_offset + 1).min(hidden),
            Key::Up | Key::Char('k') => self.help_offset = self.help_offset.saturating_sub(1),
            Key::Char('g') => self.help_offset = 0,
            Key::Char('G') => self.help_offset = hidden,
            Key::Char('n') => self.help_offset = (self.help_offset + 10).min(hidden),
            Key::Char('p') => self.help_offset = self.help_offset.saturating_sub(10),
            _ => {}
        }
    }

    fn run(&mut self, action: Action) -> Flow<Exit> {
        let sidebar = self.focus == Focus::Sidebar;
        match action {
            Action::MoveUp if sidebar => self.select_table(self.table_index.saturating_sub(1)),
            Action::MoveDown if sidebar => {
                let last = self.visible_tables().len().saturating_sub(1);
                self.select_table((self.table_index + 1).min(last));
            }
            Action::MoveUp => self.row_index = self.row_index.saturating_sub(1),
            Action::MoveDown => {
                self.row_index = (self.row_index + 1).min(self.rows.len().saturating_sub(1))
            }
            Action::MoveLeft => self.column_index = self.column_index.saturating_sub(1),
            Action::MoveRight => {
                self.column_index =
                    (self.column_index + 1).min(self.headers.len().saturating_sub(1));
            }
            Action::Activate if sidebar => {
                self.offset = 0;
                self.load(false);
                self.focus = Focus::Grid;
            }
            Action::Command => self.command = Some(String::new()),
            Action::NextPane | Action::PreviousPane => {
                self.toggle_focus(action == Action::NextPane)
            }
            Action::FocusTables => {
                self.tables_hidden = false;
                self.focus = Focus::Sidebar;
                self.mode = Mode::Browse;
            }
            Action::FocusRows => {
                self.focus = Focus::Grid;
                self.mode = Mode::Browse;
            }
            Action::ToggleTables => self.toggle_tables(),
            Action::SortColumn => self.sort_by_current_column(),
            Action::FilterTables => {
                self.filtering = true;
                self.tables_hidden = false;
                self.focus = Focus::Sidebar;
                self.say("filtering tables · ↵ keeps it · esc clears it");
            }
            Action::NextPage | Action::PreviousPage => self.page(action == Action::NextPage),
            Action::Reload => self.reload(),
            Action::Connections => return Flow::Exit(Exit::Connections),
            Action::Narrow => self.resize(Some(-4)),
            Action::Widen => self.resize(Some(4)),
            Action::ResetWidth => self.resize(None),
            Action::Help => {
                self.help_offset = 0;
                self.mode = Mode::Help;
            }
            Action::Quit => return Flow::Exit(Exit::Quit),
            Action::Databases => self.open_databases(),
            Action::Activate => self.say("editing is not ported yet"),
            Action::Escape | Action::Redraw => {}
            other => {
                let description = keymap::binding(other).description;
                self.say(format!("{description}: not ported yet"));
            }
        }
        Flow::Continue
    }

    fn offers(&self) -> Vec<(String, &'static str)> {
        let key = |action, which| keymap::key(action, which);
        if self.problem.is_some() {
            return vec![("esc".into(), "Close")];
        }
        if self.databases.is_some() {
            return vec![
                ("↑↓".into(), "Move"),
                ("↵".into(), "Choose"),
                ("esc".into(), "Cancel"),
            ];
        }
        if self.command.is_some() {
            return vec![("↵".into(), "Run"), ("esc".into(), "Cancel")];
        }
        if self.filtering {
            return vec![("↵".into(), "Keep"), ("esc".into(), "Clear")];
        }
        if self.mode == Mode::Help {
            return vec![("j k".into(), "Scroll"), ("esc".into(), "Close")];
        }
        let database_offer = if self.connection.driver == "sqlite" {
            String::new()
        } else {
            key(Action::Databases, 0)
        };
        if self.focus == Focus::Sidebar {
            return vec![
                ("↵".into(), "Open"),
                (key(Action::FilterTables, 0), "Filter"),
                (key(Action::ToggleTables, 0), "Hide"),
                (
                    format!("{}{}", key(Action::Narrow, 1), key(Action::Widen, 1)),
                    "Width",
                ),
                (database_offer, "Database"),
                (key(Action::FocusRows, 0), "Rows"),
                (key(Action::Help, 0), "Help"),
            ];
        }

        let mut offers = Vec::new();
        if self.has_more || self.offset > 0 {
            offers.push((
                format!(
                    "{}/{}",
                    key(Action::NextPage, 0),
                    key(Action::PreviousPage, 0)
                ),
                "Page",
            ));
        }
        offers.push((key(Action::SortColumn, 0), "Sort"));
        offers.push((key(Action::Help, 0), "Help"));
        offers
    }

    fn status_line(&self, width: usize) -> Line<'static> {
        if let Some(command) = &self.command {
            let hint = if command == "r" {
                "  reloads the table"
            } else {
                ""
            };
            return Line::from(vec![
                Span::raw(format!(" :{command}")),
                Span::raw("█"),
                Span::styled(hint, theme::dim()),
            ]);
        }
        if self.filtering {
            return Line::from(vec![
                Span::raw(format!(" /{}", self.filter)),
                Span::raw("█"),
            ]);
        }
        if self.mode == Mode::Help {
            return Line::from(vec![
                Span::raw(" "),
                Span::styled("help", theme::bold()),
                Span::styled("   ? or esc closes", theme::dim()),
            ]);
        }

        let name = match self.connection.tag_color().and_then(theme::named) {
            Some(color) => Span::styled(self.connection.name.clone(), theme::bold().fg(color)),
            None => Span::styled(self.connection.name.clone(), theme::dim()),
        };
        let tag = self
            .connection
            .tag
            .as_ref()
            .map(|tag| Span::styled(format!(" [{tag}]"), theme::dim()))
            .unwrap_or_default();
        let position = if self.headers.is_empty() {
            String::new()
        } else {
            format!(" · col {}/{}", self.column_index + 1, self.headers.len())
        };
        let left = Line::from(vec![
            Span::raw(" "),
            name,
            tag,
            Span::styled(position, theme::dim()),
        ]);

        let Some(said) = self.status.as_deref().filter(|said| !said.is_empty()) else {
            return left;
        };
        let room = width.saturating_sub(1);
        let said = if text::width(said) > room.saturating_sub(2) {
            truncate(said, room.saturating_sub(2))
        } else {
            said.to_string()
        };
        let style = if self.status_fresh {
            theme::fresh()
        } else {
            theme::dim()
        };
        let space = width.saturating_sub(text::width(&said) + 1);
        let left_text = left.to_string();
        let left = if text::width(&left_text) > space.saturating_sub(2) {
            Line::raw(truncate(&left_text, space.saturating_sub(2)))
        } else {
            left
        };
        let gap = space.saturating_sub(left.width()).max(1);

        let mut spans = left.spans;
        spans.push(Span::raw(" ".repeat(gap)));
        spans.push(Span::styled(said, style));
        spans.push(Span::raw(" "));
        Line::from(spans)
    }

    fn sidebar(&self, tables_width: usize, frame_height: u16) -> Island {
        let mut island = Island::placed(1, 1, tables_width as u16 + 2, frame_height);
        island.focused = self.focus == Focus::Sidebar;

        let name = match self.connection.driver.as_str() {
            "sqlite" => Path::new(self.connection.database.as_deref().unwrap_or_default())
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_default(),
            _ => self.active_database.clone().unwrap_or_default(),
        };
        let name = if name.trim().is_empty() {
            "TABLES".to_string()
        } else {
            name
        };
        let filter = if self.filter.is_empty() {
            String::new()
        } else {
            format!(" /{}", self.filter)
        };
        let pane = "[1] ";
        let room = tables_width.saturating_sub(4 + pane.len());
        island.title = format!(
            "{pane}{}{filter}",
            truncate(&name, room.saturating_sub(text::width(&filter)).max(4))
        );

        let inner_width = island.inner_width();
        let inner_height = island.inner_height();
        let tables = self.visible_tables();
        island.content = if tables.is_empty() {
            vec![Line::styled(" nothing matches", theme::dim())]
        } else {
            let start = window(self.table_index, tables.len(), inner_height);
            tables
                .iter()
                .enumerate()
                .skip(start)
                .take(inner_height)
                .map(|(position, table)| {
                    let label = format!(" {}", truncate(table, inner_width.saturating_sub(2)));
                    if position == self.table_index {
                        Line::styled(pad(&label, inner_width), theme::selection())
                    } else {
                        Line::styled(label, theme::dim())
                    }
                })
                .collect()
        };
        island
    }

    fn grid(&mut self, x: u16, y: u16, width: u16, height: u16) -> Island {
        let mut island = Island::placed(x, y, width, height);
        island.focused = self.focus == Focus::Grid;
        island.title = format!(
            "[2] {}",
            self.current_table().unwrap_or_else(|| "ROWS".to_string())
        );

        if self.pending.is_some() {
            island.content = vec![Line::styled(" loading…", theme::dim())];
            return island;
        }

        let grid = Grid {
            headers: &self.headers,
            rows: &self.rows,
            row_index: self.row_index,
            column_index: self.column_index,
            column_offset: self.column_offset,
            sort: self.sort.as_ref(),
            focused: island.focused,
            overrides: &self.width_overrides,
        };
        let (offset, widths, content) = grid.lines(island.inner_width(), island.inner_height());
        self.column_offset = offset;
        self.hits.widths = widths.clone();
        self.hits.row_start = window(
            self.row_index,
            self.rows.len(),
            island.inner_height().saturating_sub(2).max(1),
        );
        if !self.headers.is_empty() {
            island.joins = grid::joins(&widths);
            island.rules = vec![1];
        }
        island.content = content;
        island
    }
}

#[derive(Debug, Clone, Copy, Default)]
struct Area {
    x: u16,
    y: u16,
    width: u16,
    height: u16,
}

impl Area {
    fn of(island: &Island) -> Self {
        Area {
            x: island.x,
            y: island.y,
            width: island.width,
            height: island.height,
        }
    }

    fn contains(&self, column: u16, row: u16) -> bool {
        column >= self.x
            && column < self.x + self.width
            && row >= self.y
            && row < self.y + self.height
    }

    fn local_row(&self, row: u16) -> isize {
        row as isize - self.y as isize - 1
    }

    fn local_column(&self, column: u16) -> isize {
        column as isize - self.x as isize - 1
    }

    fn contains_content(&self, column: u16, row: u16) -> bool {
        let (local_row, local_column) = (self.local_row(row), self.local_column(column));
        self.contains(column, row)
            && local_row >= 0
            && local_row < self.height as isize - 2
            && local_column >= 0
            && local_column < self.width as isize - 2
    }
}

#[derive(Debug, Default)]
struct Hits {
    sidebar: Option<Area>,
    sidebar_start: usize,
    table: Option<Area>,
    row_start: usize,
    widths: Vec<usize>,
    editor: Option<Area>,
}

#[derive(Debug, Clone)]
enum Drag {
    Tables,
    Column { name: String, index: usize },
}

impl Browser {
    fn press(&mut self, column: u16, row: u16) {
        if self
            .hits
            .sidebar
            .is_some_and(|area| area.contains(column, row))
        {
            self.focus = Focus::Sidebar;
        } else if self
            .hits
            .table
            .is_some_and(|area| area.contains(column, row))
        {
            self.focus = Focus::Grid;
        }

        if let Some(sidebar) = self.hits.sidebar
            && column == sidebar.x + sidebar.width - 1
            && row >= sidebar.y
            && row < sidebar.y + sidebar.height
        {
            self.drag = Some(Drag::Tables);
            return;
        }

        if let Some(table) = self.hits.table
            && row == table.y + 1
            && column >= table.x
            && column < table.x + table.width
        {
            if let Some(index) = self.handle_near(column)
                && let Some(name) = self.headers.get(index).cloned()
            {
                self.drag = Some(Drag::Column { name, index });
                self.column_index = index;
                return;
            }
            if let Some(index) = self.column_index_for(table.local_column(column))
                && let Some(name) = self.headers.get(index).cloned()
            {
                self.sort_by(name);
            }
            return;
        }

        self.click(column, row);
    }

    fn click(&mut self, column: u16, row: u16) {
        if let Some(sidebar) = self.hits.sidebar
            && sidebar.contains_content(column, row)
        {
            let target = self.hits.sidebar_start + sidebar.local_row(row) as usize;
            if target < self.visible_tables().len() {
                self.select_table(target);
            }
            return;
        }

        if let Some(table) = self.hits.table
            && table.contains_content(column, row)
        {
            let target = self.hits.row_start as isize + table.local_row(row) - 2;
            if target < 0 || target as usize >= self.rows.len() {
                return;
            }
            self.focus = Focus::Grid;
            self.row_index = target as usize;
            if let Some(index) = self.column_index_for(table.local_column(column)) {
                self.column_index = index;
            }
            if self.is_double_click() {
                self.say("editing is not ported yet");
            }
            return;
        }

        if self
            .hits
            .editor
            .is_some_and(|editor| editor.contains(column, row))
        {
            self.say("the SQL editor is not ported yet");
        }
    }

    fn is_double_click(&mut self) -> bool {
        let window = Duration::from_millis(settings::with(|settings| settings.double_click_ms));
        let here = (self.row_index, self.column_index);
        let double = self
            .last_click
            .is_some_and(|(row, column, at)| (row, column) == here && at.elapsed() <= window);
        self.last_click = if double {
            None
        } else {
            Some((here.0, here.1, Instant::now()))
        };
        double
    }

    fn column_index_for(&self, local_column: isize) -> Option<usize> {
        let local = local_column - grid::GUTTER as isize;
        let mut offset = 0isize;
        for (position, width) in self.hits.widths.iter().enumerate() {
            let width = *width as isize;
            if local >= offset && local < offset + width + 2 {
                return Some(self.column_offset + position);
            }
            offset += width + 3;
        }
        None
    }

    fn handles(&self) -> Vec<(usize, u16)> {
        let Some(table) = self.hits.table else {
            return Vec::new();
        };
        let mut x = table.x + 1 + grid::GUTTER as u16;
        let mut handles = Vec::new();
        for (position, width) in self.hits.widths.iter().enumerate() {
            x += *width as u16 + 2;
            handles.push((self.column_offset + position, x));
            x += 1;
        }
        handles
    }

    fn handle_near(&self, column: u16) -> Option<usize> {
        self.handles()
            .into_iter()
            .find(|(_, x)| column.abs_diff(*x) <= 1)
            .map(|(index, _)| index)
    }

    fn cell_start(&self, index: usize) -> Option<u16> {
        let table = self.hits.table?;
        let mut x = table.x + 1 + grid::GUTTER as u16;
        for (position, width) in self.hits.widths.iter().enumerate() {
            if self.column_offset + position == index {
                return Some(x);
            }
            x += *width as u16 + 3;
        }
        None
    }

    fn drag_to(&mut self, column: u16) {
        match self.drag.clone() {
            Some(Drag::Tables) => {
                let width = (column as usize).saturating_sub(2).clamp(8, 80);
                self.sidebar_width = Some(width);
                self.say(format!("table list width {width}"));
            }
            Some(Drag::Column { name, index }) => {
                let Some(start) = self.cell_start(index) else {
                    return;
                };
                let width = (column as isize - start as isize - 2).clamp(3, 200) as usize;
                self.width_overrides.insert(name.clone(), width);
                self.say(format!("{name} width {width}"));
            }
            None => {}
        }
    }
}

impl Tui for Browser {
    type Output = Exit;

    fn on_key(&mut self, key: Key) -> Flow<Exit> {
        if self.problem.is_some() {
            if matches!(key, Key::Escape | Key::Enter | Key::Char('q')) {
                self.problem = None;
            }
            return Flow::Continue;
        }
        if key == Key::Ctrl('c') {
            return Flow::Exit(Exit::Quit);
        }
        if self.mode == Mode::Help {
            self.type_into_help(key);
            return Flow::Continue;
        }
        if self.databases.is_some() {
            self.type_into_databases(key);
            return Flow::Continue;
        }
        if self.command.is_some() {
            return self.type_into_command(key);
        }
        if self.filtering {
            self.type_into_filter(key);
            return Flow::Continue;
        }

        match keymap::action(key) {
            Some(binding) => self.run(binding.action),
            None => Flow::Continue,
        }
    }

    fn wants_mouse(&self) -> bool {
        settings::with(|settings| settings.mouse)
    }

    fn on_mouse(&mut self, mouse: Mouse) -> Flow<Exit> {
        let (row_offset, column_offset) =
            settings::with(|settings| (settings.mouse_row_offset, settings.mouse_column_offset));
        let column = (mouse.column as i64 - column_offset).max(0) as u16;
        let row = (mouse.row as i64 - row_offset).max(0) as u16;

        if self.problem.is_some() || self.databases.is_some() || self.mode == Mode::Help {
            return Flow::Continue;
        }

        match mouse.button {
            Button::Release => self.drag = None,
            Button::Drag => self.drag_to(column),
            Button::WheelUp => return self.run(Action::MoveUp),
            Button::WheelDown => return self.run(Action::MoveDown),
            Button::Left => self.press(column, row),
            Button::Other => {}
        }
        Flow::Continue
    }

    fn idle(&mut self) -> bool {
        if let Some(keep_cursor) = self.pending {
            self.load(keep_cursor);
            return true;
        }
        let fade = Duration::from_secs_f64(settings::with(|settings| settings.status_seconds));
        if self.status_fresh && self.status_since.elapsed() >= fade {
            self.status_fresh = false;
            return true;
        }
        false
    }

    fn render(&mut self, frame: &mut Frame) {
        if chrome::too_small(frame) {
            return;
        }

        let area = frame.area();
        let (hotkey_bar, sql_always, sql_at_bottom) = settings::with(|settings| {
            (
                settings.hotkey_bar,
                settings.sql_always,
                settings.sql_at_bottom,
            )
        });
        let chrome_lines = if hotkey_bar { 2 } else { 1 };
        let frame_height = area
            .height
            .saturating_sub(chrome_lines + BOTTOM_MARGIN)
            .max(6);
        let tables_width = self
            .tables_width()
            .min((area.width as usize).saturating_sub(30).max(8));

        let sidebar = self.sidebar(tables_width, frame_height);
        self.hits = Hits {
            sidebar: (!self.tables_hidden).then(|| Area::of(&sidebar)),
            sidebar_start: window(
                self.table_index,
                self.visible_tables().len(),
                sidebar.inner_height(),
            ),
            ..Hits::default()
        };
        let right_x = if self.tables_hidden {
            1
        } else {
            sidebar.x + sidebar.width + 1
        };
        let right_width = area.width.saturating_sub(right_x).max(20);

        let editor_height = if sql_always {
            settings::with(|settings| settings.sql_height(frame_height as usize)) as u16
        } else {
            0
        };
        let table_height = frame_height - editor_height;
        let (table_y, editor_y) = if sql_at_bottom {
            (1, 1 + table_height)
        } else {
            (1 + editor_height, 1)
        };
        let grid = self.grid(right_x, table_y, right_width, table_height.max(5));

        if !self.tables_hidden {
            sidebar.draw(frame.buffer_mut());
        }
        grid.draw(frame.buffer_mut());

        self.hits.table = Some(Area::of(&grid));
        if sql_always {
            let mut editor = Island::placed(right_x, editor_y, right_width, editor_height);
            self.hits.editor = Some(Area::of(&editor));
            editor.title = "[3] SQL".to_string();
            editor.content = sql::lines(
                self.last_statement.as_deref().unwrap_or_default(),
                editor.inner_width(),
                editor.inner_height(),
            );
            editor.draw(frame.buffer_mut());
        }

        if self.mode == Mode::Help {
            let help_width = help::width_for(area.width.saturating_sub(4) as usize) as u16;
            let height = frame_height
                .saturating_sub(2)
                .min(help::natural_height(help_width.saturating_sub(2) as usize) as u16 + 2);
            let (content, hidden) = help::content(
                help_width.saturating_sub(2) as usize,
                height.saturating_sub(2) as usize,
                self.help_offset,
            );
            let island = Island {
                title: "HELP".to_string(),
                content,
                ..Island::default()
            };
            self.help_hidden = hidden;
            Modal::new(area.width, 1, frame_height, help_width).draw(
                frame.buffer_mut(),
                island,
                height,
            );
        }

        if let Some(choice) = &self.databases {
            let width = area.width.saturating_sub(4).min(choice::WIDTH);
            let height = choice.rows();
            let island = Island {
                title: choice.title.clone(),
                content: choice.content(
                    width.saturating_sub(2) as usize,
                    height.saturating_sub(2) as usize,
                ),
                ..Island::default()
            };
            Modal::new(area.width, 1, frame_height, width).draw(frame.buffer_mut(), island, height);
        }

        if let Some(problem) = &self.problem {
            let island = Island {
                title: "ERROR".to_string(),
                content: vec![Line::styled(format!(" {problem}"), theme::problem())],
                ..Island::default()
            };
            Modal::new(
                area.width,
                1,
                frame_height,
                area.width.saturating_sub(4).min(70),
            )
            .draw(frame.buffer_mut(), island, 3);
        }

        if hotkey_bar {
            chrome::line_at(
                frame,
                frame_height,
                chrome::hotkeys(&self.offers(), area.width as usize),
            );
        }
        chrome::line_at(
            frame,
            frame_height + chrome_lines - 1,
            self.status_line(area.width as usize),
        );
    }
}
