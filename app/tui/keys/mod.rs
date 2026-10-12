use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::time::Instant;

use crate::keystore::{Change, Contents, KeyKind, KeyStore, Reply};
use crate::registry::{self, Name};
use crate::settings;
use crate::store::Connection;
use crate::terminal::prelude::*;
use crate::terminal::{CursorShape, Flow, Tui};

use super::browser::Exit;
use super::choice::Choice;
use super::editor::QueryEditor;
use super::filters::LineEditor;
use super::palette::{Item, Kind, Palette};
use super::settings_modal::SettingsModal;
use super::updating::{self, Updating};

mod cli;
mod edit;
mod render;
pub mod tree;

pub use tree::Row;

pub const LIMIT: usize = 100_000;
const BATCH: usize = 1000;
const PAGE: usize = 200;
const FOLDED_OVER: usize = 200;
pub const READ_ONLY: &str = "this connection is read-only, so nothing here changes it";
const KINDS: [&str; 5] = ["string", "hash", "list", "set", "zset"];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    Keys,
    Value,
}

enum Batch {
    Total(u64),
    Keys(Vec<(String, KeyKind)>),
    Done { capped: bool },
    Failed(String),
}

pub struct Shown {
    pub key: String,
    pub kind: KeyKind,
    pub ttl: Option<i64>,
    pub contents: Contents,
    pub row: usize,
    pub offset: usize,
}

pub struct KeyBrowser {
    connection: Connection,
    store: Box<dyn KeyStore>,
    server: String,
    database: u32,
    keys: Vec<String>,
    kinds: std::collections::HashMap<String, KeyKind>,
    total: u64,
    capped: bool,
    scan: Option<Receiver<Batch>>,
    pattern: String,
    filtering: Option<String>,
    opened: HashSet<String>,
    closed: HashSet<String>,
    index: usize,
    focus: Focus,
    shown: Option<Shown>,
    pending: Vec<Change>,
    prompt: Option<edit::Prompt>,
    kinds_choice: Option<(String, Choice)>,
    databases: Option<Choice>,
    command: Option<String>,
    cli: Option<cli::Cli>,
    palette: Option<Palette>,
    settings: Option<SettingsModal>,
    help: bool,
    status: Option<String>,
    status_since: Instant,
    failed: bool,
    store_path: Option<PathBuf>,
    updating: Option<Updating>,
}

impl KeyBrowser {
    pub fn new(connection: Connection, mut store: Box<dyn KeyStore>) -> anyhow::Result<Self> {
        let database = connection
            .database
            .as_deref()
            .and_then(|database| database.trim().parse().ok())
            .unwrap_or(0);
        let server = store.server();
        let mut browser = KeyBrowser {
            connection,
            store,
            server,
            database,
            keys: Vec::new(),
            kinds: Default::default(),
            total: 0,
            capped: false,
            scan: None,
            pattern: String::new(),
            filtering: None,
            opened: HashSet::new(),
            closed: HashSet::new(),
            index: 0,
            focus: Focus::Keys,
            shown: None,
            pending: Vec::new(),
            prompt: None,
            kinds_choice: None,
            databases: None,
            command: None,
            cli: None,
            palette: None,
            settings: None,
            help: false,
            status: None,
            status_since: Instant::now(),
            failed: false,
            store_path: None,
            updating: None,
        };
        browser.rescan();
        Ok(browser)
    }

    pub fn with_store(mut self, store: PathBuf) -> Self {
        self.store_path = Some(store);
        self
    }

    pub fn scanning(&self) -> bool {
        self.scan.is_some()
    }

    pub fn key_count(&self) -> usize {
        self.keys.len()
    }

    pub fn pending(&self) -> &[Change] {
        &self.pending
    }

    pub fn status(&self) -> &str {
        self.status.as_deref().unwrap_or_default()
    }

    pub fn tell(&mut self, text: impl Into<String>) {
        self.say(text);
    }

    fn say(&mut self, text: impl Into<String>) {
        self.status = Some(text.into());
        self.status_since = Instant::now();
        self.failed = false;
    }

    fn fail(&mut self, text: impl Into<String>) {
        self.say(text);
        self.failed = true;
    }

    pub fn rows(&self) -> Vec<Row> {
        let mut keys = self.keys.clone();
        for change in &self.pending {
            if let Change::NewKey { key, .. } = change
                && !keys.contains(key)
            {
                keys.push(key.clone());
            }
        }
        tree::rows(
            &keys,
            &tree::Folds {
                opened: &self.opened,
                closed: &self.closed,
                open_by_default: self.keys.len() <= FOLDED_OVER,
            },
        )
    }

    fn current(&self) -> Option<Row> {
        self.rows().get(self.index).cloned()
    }

    fn current_key(&self) -> Option<String> {
        match self.current()? {
            Row::Key { key, .. } => Some(key),
            Row::Space { .. } => None,
        }
    }

    pub fn select_key(&mut self, key: &str) {
        for space in tree::spaces_of(key) {
            self.closed.remove(&space);
            self.opened.insert(space);
        }
        if let Some(at) = self
            .rows()
            .iter()
            .position(|row| matches!(row, Row::Key { key: here, .. } if here == key))
        {
            self.index = at;
        }
        self.focus = Focus::Keys;
    }

    pub fn open_key(&mut self, key: &str) {
        self.select_key(key);
        self.load(key, 0);
        self.focus = Focus::Value;
    }

    fn load(&mut self, key: &str, offset: usize) {
        let kind = match self.kinds.get(key).cloned() {
            Some(kind) => kind,
            None => match self.store.kinds(&[key.to_string()]) {
                Ok(mut kinds) => kinds.pop().unwrap_or(KeyKind::String),
                Err(error) => return self.fail(format!("{error:#}")),
            },
        };
        let contents = match self.store.read(key, &kind, offset, PAGE) {
            Ok(contents) => contents,
            Err(error) => return self.fail(format!("{error:#}")),
        };
        let ttl = self.store.ttl(key).unwrap_or(None);
        self.shown = Some(Shown {
            key: key.to_string(),
            kind,
            ttl,
            contents,
            row: 0,
            offset,
        });
    }

    fn reload_shown(&mut self) {
        if let Some((key, offset)) = self
            .shown
            .as_ref()
            .map(|shown| (shown.key.clone(), shown.offset))
        {
            self.load(&key, offset);
        }
    }

    pub fn rescan(&mut self) {
        self.keys.clear();
        self.kinds.clear();
        self.total = 0;
        self.capped = false;
        self.index = 0;
        let mut forked = match self.store.fork() {
            Ok(forked) => forked,
            Err(error) => return self.fail(format!("{error:#}")),
        };
        let pattern = self.pattern.clone();
        let (sender, receiver) = mpsc::channel();
        std::thread::spawn(move || {
            if let Ok(total) = forked.size() {
                let _ = sender.send(Batch::Total(total));
            }
            let mut cursor = 0;
            let mut seen = 0;
            loop {
                let scanned = match forked.scan(cursor, &pattern, BATCH) {
                    Ok(scanned) => scanned,
                    Err(error) => {
                        let _ = sender.send(Batch::Failed(format!("{error:#}")));
                        return;
                    }
                };
                let room = LIMIT - seen;
                let keys: Vec<String> = scanned.keys.into_iter().take(room).collect();
                seen += keys.len();
                let kinds = forked.kinds(&keys).unwrap_or_default();
                let batch = keys
                    .into_iter()
                    .zip(kinds.into_iter().chain(std::iter::repeat(KeyKind::String)))
                    .collect();
                if sender.send(Batch::Keys(batch)).is_err() {
                    return;
                }
                cursor = scanned.cursor;
                if cursor == 0 || seen >= LIMIT {
                    let _ = sender.send(Batch::Done {
                        capped: cursor != 0,
                    });
                    return;
                }
            }
        });
        self.scan = Some(receiver);
    }

    fn hear_from_scan(&mut self) -> bool {
        let Some(receiver) = self.scan.as_ref() else {
            return false;
        };
        let mut heard = false;
        loop {
            match receiver.try_recv() {
                Ok(Batch::Total(total)) => self.total = total,
                Ok(Batch::Keys(batch)) => {
                    for (key, kind) in batch {
                        self.kinds.insert(key.clone(), kind);
                        self.keys.push(key);
                    }
                }
                Ok(Batch::Done { capped }) => {
                    self.capped = capped;
                    self.keys.sort();
                    self.scan = None;
                    return true;
                }
                Ok(Batch::Failed(error)) => {
                    self.scan = None;
                    self.fail(error);
                    return true;
                }
                Err(TryRecvError::Empty) => return heard,
                Err(TryRecvError::Disconnected) => {
                    self.scan = None;
                    return true;
                }
            }
            heard = true;
        }
    }

    fn move_by(&mut self, steps: isize) {
        match self.focus {
            Focus::Keys => {
                let last = self.rows().len().saturating_sub(1);
                self.index = self.index.saturating_add_signed(steps).min(last);
            }
            Focus::Value => {
                let count = self
                    .overlaid()
                    .map(|(_, rows, _)| rows.len())
                    .unwrap_or_default();
                if let Some(shown) = self.shown.as_mut() {
                    shown.row = shown
                        .row
                        .saturating_add_signed(steps)
                        .min(count.saturating_sub(1));
                }
            }
        }
    }

    fn fold(&mut self, open: Option<bool>) {
        match self.current() {
            Some(Row::Space {
                path, open: now, ..
            }) => {
                let open = open.unwrap_or(!now);
                if open {
                    self.closed.remove(&path);
                    self.opened.insert(path);
                } else {
                    self.opened.remove(&path);
                    self.closed.insert(path);
                }
            }
            Some(Row::Key { key, .. }) if open == Some(false) => {
                if let Some(parent) = tree::spaces_of(&key).pop() {
                    self.opened.remove(&parent);
                    self.closed.insert(parent.clone());
                    if let Some(at) = self
                        .rows()
                        .iter()
                        .position(|row| matches!(row, Row::Space { path, .. } if *path == parent))
                    {
                        self.index = at;
                    }
                }
            }
            Some(Row::Key { key, .. }) => {
                self.load(&key, 0);
                self.focus = Focus::Value;
            }
            None => {}
        }
    }

    fn page(&mut self, forward: bool) {
        let Some((key, offset, more)) = self.shown.as_ref().map(|shown| {
            (
                shown.key.clone(),
                shown.offset,
                matches!(shown.contents, Contents::Rows { more: true, .. }),
            )
        }) else {
            return;
        };
        match forward {
            true if more => self.load(&key, offset + PAGE),
            false if offset > 0 => self.load(&key, offset.saturating_sub(PAGE)),
            _ => {}
        }
    }

    fn open_databases(&mut self) {
        match self.store.databases() {
            Ok(databases) => {
                let options: Vec<String> = databases
                    .iter()
                    .map(|(number, keys)| {
                        format!(
                            "db{number}  {keys} {}",
                            if *keys == 1 { "key" } else { "keys" }
                        )
                    })
                    .collect();
                let chosen = options
                    .iter()
                    .find(|option| option.starts_with(&format!("db{} ", self.database)))
                    .cloned()
                    .unwrap_or_default();
                self.databases = Some(Choice::new("DATABASE", options, &chosen, false));
            }
            Err(error) => self.fail(format!("{error:#}")),
        }
    }

    fn use_database(&mut self, chosen: &str) {
        let Some(number) = chosen
            .trim_start_matches("db")
            .split_whitespace()
            .next()
            .and_then(|number| number.parse().ok())
        else {
            return;
        };
        if !self.pending.is_empty() {
            return self.say("write or drop the pending changes before switching database");
        }
        if let Err(error) = self.store.select(number) {
            return self.fail(format!("{error:#}"));
        }
        self.database = number;
        self.shown = None;
        self.opened.clear();
        self.closed.clear();
        self.rescan();
        self.say(format!("using db{number}"));
    }

    fn type_into_filter(&mut self, key: Key) {
        let Some(typed) = self.filtering.as_mut() else {
            return;
        };
        match key {
            Key::Escape => self.filtering = None,
            Key::Enter => {
                let typed = typed.trim().to_string();
                self.filtering = None;
                self.pattern = match typed.contains(['*', '?', '[']) {
                    true => typed,
                    false if typed.is_empty() => String::new(),
                    false => format!("*{typed}*"),
                };
                self.shown = None;
                self.rescan();
            }
            Key::Backspace | Key::Ctrl('h') => {
                typed.pop();
            }
            Key::Char(character) => typed.push(character),
            _ => {}
        }
    }

    fn type_into_command(&mut self, key: Key) -> Flow<Exit> {
        let Some(command) = self.command.as_mut() else {
            return Flow::Continue;
        };
        match key {
            Key::Escape => self.command = None,
            Key::Backspace if command.is_empty() => self.command = None,
            Key::Backspace | Key::Ctrl('h') => {
                command.pop();
            }
            Key::Char(character) => command.push(character),
            Key::Enter => {
                let line = command.trim().to_string();
                self.command = None;
                return self.run_command(&line);
            }
            _ => {}
        }
        Flow::Continue
    }

    fn run_command(&mut self, line: &str) -> Flow<Exit> {
        if matches!(line, "cli" | "sql") {
            self.open_cli();
            return Flow::Continue;
        }
        match registry::parse(line) {
            Some((entry, argument)) => self.run_named(entry.name, &argument),
            None => {
                self.say(format!("{line} is not a tql command"));
                Flow::Continue
            }
        }
    }

    pub fn run_named(&mut self, name: Name, argument: &str) -> Flow<Exit> {
        match name {
            Name::Quit => return Flow::Exit(Exit::Quit),
            Name::Connections => return Flow::Exit(Exit::Connections),
            Name::Write => self.write(),
            Name::Settings => {
                self.settings = Some(SettingsModal::open(&crate::paths::config_file()))
            }
            Name::Version => self.say(registry::version()),
            Name::Help => self.help = true,
            Name::Update => {
                if self.updating.is_none() {
                    self.updating = Some(Updating::start());
                }
                self.say(updating::CHECKING);
            }
            Name::Reload | Name::RunOrReload => {
                self.shown = None;
                self.rescan();
            }
            Name::Sql | Name::Run => self.open_cli(),
            Name::Open => return self.open_typed(argument),
            other => {
                let word = registry::entry_for(other).word();
                self.say(format!(":{word} works on a SQL connection"));
            }
        }
        Flow::Continue
    }

    fn open_typed(&mut self, typed: &str) -> Flow<Exit> {
        if typed.trim().is_empty() {
            self.command = Some("open ".to_string());
            return Flow::Continue;
        }
        let Some(store) = self.store_path.clone() else {
            self.say("there is no connection list to open it into");
            return Flow::Continue;
        };
        match crate::opener::saved(typed, &store, &crate::paths::key()) {
            Ok(connection) => Flow::Exit(Exit::Open(connection.id)),
            Err(error) => {
                self.fail(format!("{error:#}"));
                Flow::Continue
            }
        }
    }

    fn open_palette(&mut self) {
        let mut items: Vec<Item> = [
            Name::Write,
            Name::Reload,
            Name::Settings,
            Name::Version,
            Name::Update,
            Name::Open,
            Name::Help,
        ]
        .into_iter()
        .map(|name| {
            let entry = registry::entry_for(name);
            Item::new(
                Kind::Command(name),
                super::text::sentence(entry.description),
                format!(":{}", entry.word()),
                entry.word(),
            )
        })
        .collect();
        items.push(Item::new(
            Kind::Command(Name::Sql),
            "Run a Redis command",
            "s",
            "cli",
        ));
        items.extend(
            self.keys
                .iter()
                .take(5000)
                .map(|key| Item::new(Kind::Table, key.clone(), "key", key.clone())),
        );
        self.palette = Some(Palette::new("COMMANDS", items));
    }

    fn type_into_palette(&mut self, key: Key) -> Flow<Exit> {
        let Some(palette) = self.palette.as_mut() else {
            return Flow::Continue;
        };
        match key {
            Key::Escape => self.palette = None,
            Key::Up | Key::Ctrl('p') => palette.move_by(false),
            Key::Down | Key::Ctrl('n') => palette.move_by(true),
            Key::Enter => {
                let chosen = palette.selected();
                self.palette = None;
                match chosen.map(|item| (item.kind, item.target)) {
                    Some((Kind::Command(Name::Sql), _)) => self.open_cli(),
                    Some((Kind::Command(name), _)) => return self.run_named(name, ""),
                    Some((Kind::Table, key)) => self.open_key(&key),
                    _ => {}
                }
            }
            other => palette.type_key(other),
        }
        Flow::Continue
    }

    fn type_into_choices(&mut self, key: Key) {
        if let Some(choice) = self.databases.as_mut() {
            match key {
                Key::Escape => self.databases = None,
                Key::Up => choice.move_by(false),
                Key::Down => choice.move_by(true),
                Key::Enter => {
                    let chosen = choice.selected();
                    self.databases = None;
                    if let Some(chosen) = chosen {
                        self.use_database(&chosen);
                    }
                }
                other => choice.type_key(other),
            }
            return;
        }
        if let Some((name, choice)) = self.kinds_choice.as_mut() {
            match key {
                Key::Escape => self.kinds_choice = None,
                Key::Up => choice.move_by(false),
                Key::Down => choice.move_by(true),
                Key::Enter => {
                    let chosen = choice.selected();
                    let name = name.clone();
                    self.kinds_choice = None;
                    if let Some(chosen) = chosen {
                        self.ask_first_values(name, KeyKind::of(&chosen));
                    }
                }
                other => choice.type_key(other),
            }
        }
    }

    fn hear_from_update(&mut self) -> bool {
        let Some((said, done)) = self.updating.as_mut().and_then(Updating::heard) else {
            return false;
        };
        if done {
            self.updating = None;
        }
        self.say(said);
        true
    }

    fn on_keys_key(&mut self, key: Key) -> Flow<Exit> {
        match key {
            Key::Char('q') => return Flow::Exit(Exit::Quit),
            Key::Char('c') => return Flow::Exit(Exit::Connections),
            Key::Char(':') => self.command = Some(String::new()),
            Key::Char('/') => self.filtering = Some(String::new()),
            Key::Char('?') => self.help = true,
            Key::Char('b') => self.open_databases(),
            Key::Char('r') => {
                self.shown = None;
                self.rescan();
            }
            Key::Char('s') => self.open_cli(),
            Key::Char('u') => self.drop_pending(),
            Key::Char('d') => self.mark_key(),
            Key::Char('N') => self.new_key(),
            Key::Char('T') => self.ask_ttl(),
            Key::Char('R') => self.ask_rename(),
            Key::Tab => {
                if self.shown.is_some() {
                    self.focus = Focus::Value;
                }
            }
            Key::Up | Key::Char('k') => self.move_by(-1),
            Key::Down | Key::Char('j') => self.move_by(1),
            Key::Left | Key::Char('h') => self.fold(Some(false)),
            Key::Right | Key::Char('l') => self.fold(Some(true)),
            Key::Enter | Key::Char(' ') => self.fold(None),
            Key::Escape if !self.pattern.is_empty() => {
                self.pattern.clear();
                self.rescan();
            }
            _ => {}
        }
        Flow::Continue
    }

    fn on_value_key(&mut self, key: Key) -> Flow<Exit> {
        match key {
            Key::Char('q') => return Flow::Exit(Exit::Quit),
            Key::Char(':') => self.command = Some(String::new()),
            Key::Char('?') => self.help = true,
            Key::Char('s') => self.open_cli(),
            Key::Char('u') => self.drop_pending(),
            Key::Char('e') | Key::Enter => self.edit_here(),
            Key::Char('N') => self.add_here(),
            Key::Char('d') => self.mark_here(),
            Key::Char('n') => self.page(true),
            Key::Char('p') => self.page(false),
            Key::Up | Key::Char('k') => self.move_by(-1),
            Key::Down | Key::Char('j') => self.move_by(1),
            Key::Tab | Key::Escape | Key::Left | Key::Char('h') => self.focus = Focus::Keys,
            _ => {}
        }
        Flow::Continue
    }
}

impl Tui for KeyBrowser {
    type Output = Exit;

    fn on_key(&mut self, key: Key) -> Flow<Exit> {
        if key == Key::Ctrl('c') {
            return Flow::Exit(Exit::Quit);
        }
        if let Some(modal) = self.settings.as_mut() {
            if !modal.on_key(key) {
                self.settings = None;
            }
            return Flow::Continue;
        }
        if self.help {
            if matches!(key, Key::Escape | Key::Char('?') | Key::Char('q')) {
                self.help = false;
            }
            return Flow::Continue;
        }
        if self.palette.is_some() {
            return self.type_into_palette(key);
        }
        if self.prompt.is_some() {
            self.type_into_prompt(key);
            return Flow::Continue;
        }
        if self.databases.is_some() || self.kinds_choice.is_some() {
            self.type_into_choices(key);
            return Flow::Continue;
        }
        if self.command.is_some() {
            return self.type_into_command(key);
        }
        if self.filtering.is_some() {
            self.type_into_filter(key);
            return Flow::Continue;
        }
        if key == Key::Ctrl('k') {
            self.open_palette();
            return Flow::Continue;
        }
        if self.cli.is_some() {
            return self.type_into_cli(key);
        }
        match self.focus {
            Focus::Keys => self.on_keys_key(key),
            Focus::Value => self.on_value_key(key),
        }
    }

    fn on_paste(&mut self, text: &str) -> Flow<Exit> {
        let text = super::text::single_line(text);
        if let Some(prompt) = self.prompt.as_mut() {
            text.chars()
                .for_each(|character| prompt.editor.handle(Key::Char(character)));
        } else if let Some(cli) = self.cli.as_mut() {
            cli.editor.paste(&text);
        } else if let Some(command) = self.command.as_mut() {
            command.push_str(&text);
        } else if let Some(typed) = self.filtering.as_mut() {
            typed.push_str(&text);
        } else if let Some(modal) = self.settings.as_mut() {
            modal.on_paste(&text);
        }
        Flow::Continue
    }

    fn cursor_shape(&self) -> CursorShape {
        match self.settings.as_ref().and_then(|modal| modal.editing()) {
            Some(_) => CursorShape::Bar,
            None => CursorShape::Default,
        }
    }

    fn idle(&mut self) -> bool {
        if self.hear_from_scan() {
            return true;
        }
        if self.hear_from_update() {
            return true;
        }
        let after = settings::with(|settings| settings.status_seconds);
        if self.status.is_some()
            && !self.failed
            && after > 0.0
            && self.status_since.elapsed().as_secs_f64() >= after
        {
            self.status = None;
            return true;
        }
        false
    }

    fn top_margin(&self) -> u16 {
        settings::with(|settings| settings.top_margin)
    }

    fn left_margin(&self) -> u16 {
        settings::with(|settings| settings.left_margin)
    }

    fn right_margin(&self) -> u16 {
        settings::with(|settings| settings.right_margin)
    }

    fn render(&mut self, frame: &mut Frame) {
        self.draw(frame);
    }
}

pub fn reply_rows(reply: &Reply) -> (Vec<String>, Vec<Vec<String>>) {
    fn text(reply: &Reply) -> String {
        match reply {
            Reply::Nil => "(nil)".to_string(),
            Reply::Text(text) => text.clone(),
            Reply::Number(number) => number.to_string(),
            Reply::Error(error) => error.clone(),
            Reply::List(items) => {
                serde_json::Value::Array(items.iter().map(|item| text(item).into()).collect())
                    .to_string()
            }
            Reply::Map(pairs) => serde_json::Value::Object(
                pairs
                    .iter()
                    .map(|(key, value)| (text(key), text(value).into()))
                    .collect(),
            )
            .to_string(),
        }
    }
    match reply {
        Reply::List(items) => (
            vec!["#".to_string(), "value".to_string()],
            items
                .iter()
                .enumerate()
                .map(|(at, item)| vec![(at + 1).to_string(), text(item)])
                .collect(),
        ),
        Reply::Map(pairs) => (
            vec!["field".to_string(), "value".to_string()],
            pairs
                .iter()
                .map(|(key, value)| vec![text(key), text(value)])
                .collect(),
        ),
        other => (vec!["reply".to_string()], vec![vec![text(other)]]),
    }
}

pub fn kinds() -> Vec<String> {
    KINDS.iter().map(|kind| kind.to_string()).collect()
}

pub fn editor_for(text: &str) -> LineEditor {
    LineEditor::selecting(text)
}

pub fn fresh_editor() -> QueryEditor {
    QueryEditor::new(false)
}
