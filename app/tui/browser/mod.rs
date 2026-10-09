use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use crate::terminal::prelude::*;

mod command_palette;
mod databases;
mod editing;
mod export;
mod filters;
mod inspect;
mod keys;
mod links;
mod loading;
mod mouse;
mod paste;
mod query;
mod render;
mod structure;
mod vim;
mod yank;

pub use editing::Edits;
pub use inspect::Inspector;
use links::{Jump, LinkPicker};
use mouse::{Area, Drag, Hits};
pub use query::Query;
use structure::StructureView;
use vim::query_hint;

use super::choice::{self, Choice};
use super::chrome::{self, Island, Modal, theme};
use super::clipboard::{Clipboard, Recorded};
use super::error::{self, ErrorBox};
use super::filters::FilterForm;
use super::grid::{self, Grid};
use super::help;
use super::keymap::{self, Action};
use super::palette::{self, Palette};
use super::schema::Schema;
use super::sql;
use super::text::{self, pad, single_line, truncate, window};
use crate::database::{Database, Direction, Filters, Kind, Sort};
use crate::settings;
use crate::store::Connection;

pub const PAGE: usize = 100;
const BOTTOM_MARGIN: u16 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Exit {
    Quit,
    Connections,
    Open(i64),
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
    Query,
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
    pub raw: Vec<Vec<Option<String>>>,
    kinds: Vec<Vec<Kind>>,
    loaded: Option<String>,
    pub edits: Edits,
    pub row_index: usize,
    pub column_index: usize,
    pub column_offset: usize,
    pub width_overrides: HashMap<String, usize>,
    pub offset: usize,
    pub has_more: bool,
    pub sort: Option<Sort>,
    pub filters: Option<Filters>,
    pub filter_form: Option<FilterForm>,
    status: Option<String>,
    status_since: Instant,
    repaint: bool,
    status_fresh: bool,
    pub command: Option<String>,
    pub count_prefix: String,
    pub problem: Option<String>,
    pub problem_title: String,
    pub problem_notes: Vec<String>,
    pub problem_offset: usize,
    pub query: Query,
    pub palette: Option<Palette>,
    store: Option<PathBuf>,
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
    schema: Schema,
    pub structure: Option<StructureView>,
    pub inspector: Option<Inspector>,
    jumps: Vec<Jump>,
    link_picker: Option<LinkPicker>,
    clipboard: Box<dyn Clipboard>,
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
            raw: Vec::new(),
            kinds: Vec::new(),
            loaded: None,
            edits: Edits::default(),
            row_index: 0,
            column_index: 0,
            column_offset: 0,
            width_overrides: HashMap::new(),
            offset: 0,
            has_more: false,
            sort: None,
            filters: None,
            filter_form: None,
            status: None,
            status_since: Instant::now(),
            repaint: false,
            status_fresh: false,
            command: None,
            count_prefix: String::new(),
            problem: None,
            problem_title: "ERROR".to_string(),
            problem_notes: Vec::new(),
            problem_offset: 0,
            query: Query::default(),
            palette: None,
            store: None,
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
            schema: Schema::default(),
            structure: None,
            inspector: None,
            jumps: Vec::new(),
            link_picker: None,
            clipboard: Box::new(Recorded::default()),
        };
        if undecided {
            browser.open_databases();
        } else if !browser.tables.is_empty() {
            browser.load(false);
        }
        Ok(browser)
    }

    pub fn with_clipboard(mut self, clipboard: impl Clipboard + 'static) -> Self {
        self.clipboard = Box::new(clipboard);
        self
    }

    pub fn with_status(mut self, status: Option<String>) -> Self {
        if let Some(status) = status {
            self.say(status);
        }
        self
    }

    pub fn status(&self) -> &str {
        self.status.as_deref().unwrap_or_default()
    }

    pub(super) fn say(&mut self, text: impl Into<String>) {
        let text = text.into();
        self.status_fresh = self.status.as_deref() != Some(text.as_str());
        if self.status_fresh {
            self.status_since = Instant::now();
        }
        self.status = Some(text);
    }

    pub fn status_is_fresh(&self) -> bool {
        self.status_fresh
    }

    pub fn fade_status(&mut self, now: Instant) -> bool {
        let after = settings::with(|settings| settings.status_seconds);
        if after <= 0.0 || self.status.as_deref().is_none_or(str::is_empty) {
            return false;
        }
        if now.saturating_duration_since(self.status_since) < Duration::from_secs_f64(after) {
            return false;
        }
        self.hush();
        true
    }

    pub(super) fn hush(&mut self) {
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
}

impl Tui for Browser {
    type Output = Exit;

    fn on_key(&mut self, key: Key) -> Flow<Exit> {
        self.status_fresh = false;
        if self.problem.is_some() {
            self.type_into_problem(key);
            return Flow::Continue;
        }
        if key == Key::Ctrl('c') {
            return Flow::Exit(Exit::Quit);
        }
        if key == Key::Ctrl('l') {
            self.repaint = true;
            self.say("redrawn");
            return Flow::Continue;
        }
        if self.edits.form().is_some() {
            self.type_into_edits(key);
            return Flow::Continue;
        }
        if self.palette.is_some() {
            return self.type_into_palette(key);
        }
        if self.mode == Mode::Help {
            self.type_into_help(key);
            return Flow::Continue;
        }
        if self.structure.is_some() {
            self.type_into_structure(key);
            return Flow::Continue;
        }
        if self.inspector.is_some() {
            self.type_into_inspector(key);
            return Flow::Continue;
        }
        if self.mode == Mode::Query {
            return self.type_into_query(key);
        }
        if self.edits.captures_keys() {
            self.type_into_edits(key);
            return Flow::Continue;
        }
        if self.databases.is_some() {
            self.type_into_databases(key);
            return Flow::Continue;
        }
        if self.link_picker.is_some() {
            self.type_into_link_picker(key);
            return Flow::Continue;
        }
        if self.filter_form.is_some() {
            self.type_into_filter_form(key);
            return Flow::Continue;
        }
        if self.command.is_some() {
            return self.type_into_command(key);
        }
        if self.filtering {
            self.type_into_filter(key);
            return Flow::Continue;
        }

        self.type_into_browse(key)
    }

    fn cursor_shape(&self) -> CursorShape {
        if self.inserts_at_terminal_cursor() {
            CursorShape::Bar
        } else {
            CursorShape::Default
        }
    }

    fn wants_mouse(&self) -> bool {
        settings::with(|settings| settings.mouse)
    }

    fn top_margin(&self) -> u16 {
        settings::with(|settings| settings.top_margin)
    }

    fn take_repaint(&mut self) -> bool {
        std::mem::take(&mut self.repaint)
    }

    fn on_paste(&mut self, text: &str) -> Flow<Exit> {
        self.paste(text);
        Flow::Continue
    }

    fn on_mouse(&mut self, mouse: Mouse) -> Flow<Exit> {
        if !self.wants_mouse() {
            return Flow::Continue;
        }
        let (row_offset, column_offset) =
            settings::with(|settings| (settings.mouse_row_offset, settings.mouse_column_offset));
        let column = (mouse.column as i64 - column_offset).max(0) as u16;
        let row = (mouse.row as i64 - row_offset).max(0) as u16;

        if self.problem.is_some()
            || self.palette.is_some()
            || self.databases.is_some()
            || self.filter_form.is_some()
            || self.mode == Mode::Help
            || self.modal_over_grid()
        {
            return Flow::Continue;
        }
        if self.edits.captures_keys() {
            self.mouse_in_edits(mouse, row);
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
        if self.load_related() {
            return true;
        }
        if let Some(keep_cursor) = self.pending {
            self.load(keep_cursor);
            return true;
        }
        if self.fade_status(Instant::now()) {
            return true;
        }
        false
    }

    fn render(&mut self, frame: &mut Frame) {
        self.draw(frame);
    }
}
