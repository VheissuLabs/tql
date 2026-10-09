use std::collections::HashMap;
use std::env;
use std::path::PathBuf;

use crate::terminal::prelude::*;
use ratatui::buffer::Buffer;

use super::choice::Choice;
use super::chrome::{Island, theme};
use super::filters::LineEditor;
use super::key_files;
use super::text::{pad, single_line, truncate, width};
use crate::encrypter::Encrypter;
use crate::paths;
use crate::store::Connection;
use crate::store_writer::{Field, Fields, StoreWriter};
use crate::tag;

pub const DRIVERS: [&str; 4] = ["sqlite", "mysql", "pgsql", "sqlsrv"];
pub const SSL_MODES: [&str; 6] = [
    "",
    "disable",
    "prefer",
    "require",
    "verify-ca",
    "verify-full",
];
pub const TYPE_IT: &str = "type a path…";
pub const NO_GROUP: &str = "none";

const YES_NO: [&str; 2] = ["no", "yes"];
const VALUE_WIDTH: usize = 34;
const PICKER_WIDTH: u16 = 52;
const SSH_FIELDS: [(&str, &str); 5] = [
    ("ssh_host", "Server"),
    ("ssh_port", "Port"),
    ("ssh_user", "User"),
    ("ssh_key", "Key"),
    ("ssh_password", "Password"),
];
const FILES: [&str; 4] = ["ssh_key", "ssl_ca", "ssl_cert", "ssl_key"];
const SECRETS: [&str; 2] = ["password", "ssh_password"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Places {
    pub home: PathBuf,
    pub cwd: PathBuf,
}

impl Places {
    pub fn here() -> Self {
        Places {
            home: paths::home(),
            cwd: env::current_dir().unwrap_or_default(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    Stay,
    Cancel,
    Save,
}

pub struct ConnectionForm {
    pub id: Option<i64>,
    pub creating: bool,
    pub index: usize,
    pub editor: Option<LineEditor>,
    pub values: HashMap<&'static str, String>,
    pub error: Option<String>,
    pub picker: Option<Choice>,
    unreadable: Vec<&'static str>,
    places: Places,
    groups: Vec<String>,
}

impl ConnectionForm {
    pub fn creating(places: Places) -> Self {
        let mut form = ConnectionForm::blank(places);
        form.creating = true;
        form.values.insert("driver", "sqlite".to_string());
        form
    }

    pub fn editing(connection: &Connection, encrypter: Option<&Encrypter>, places: Places) -> Self {
        let mut form = ConnectionForm::blank(places);
        form.id = Some(connection.id);
        let text = |value: &Option<String>| value.clone().unwrap_or_default();
        let number = |value: Option<u16>| value.map(|port| port.to_string()).unwrap_or_default();
        for (key, value) in [
            ("name", connection.name.clone()),
            ("database", text(&connection.database)),
            ("host", text(&connection.host)),
            ("port", number(connection.port)),
            ("username", text(&connection.username)),
            ("ssl_mode", text(&connection.ssl_mode)),
            ("ssl_ca", text(&connection.ssl_ca)),
            ("ssl_cert", text(&connection.ssl_cert)),
            ("ssl_key", text(&connection.ssl_key)),
            ("group_name", text(&connection.group_name)),
            ("tag", text(&connection.tag)),
            ("ssh_host", text(&connection.ssh_host)),
            ("ssh_port", number(connection.ssh_port)),
            ("ssh_user", text(&connection.ssh_user)),
            ("ssh_key", text(&connection.ssh_key)),
        ] {
            form.values.insert(key, value);
        }
        for (key, payload) in [
            ("password", &connection.password),
            ("ssh_password", &connection.ssh_password),
        ] {
            let Some(payload) = payload else {
                continue;
            };
            match encrypter.map(|encrypter| encrypter.decrypt_string(payload)) {
                Some(Ok(plain)) => {
                    form.values.insert(key, plain);
                }
                _ => form.unreadable.push(key),
            }
        }
        let driver = if connection.driver.is_empty() {
            "sqlite".to_string()
        } else {
            connection.driver.clone()
        };
        form.values.insert("driver", driver);
        form.values
            .insert("read_only", yes_or_no(connection.read_only));
        form.values
            .insert("over_ssh", yes_or_no(connection.uses_ssh()));
        form
    }

    fn blank(places: Places) -> Self {
        ConnectionForm {
            id: None,
            creating: false,
            index: 0,
            editor: None,
            values: HashMap::from([
                ("read_only", "no".to_string()),
                ("over_ssh", "no".to_string()),
            ]),
            error: None,
            picker: None,
            unreadable: Vec::new(),
            places,
            groups: Vec::new(),
        }
    }

    pub fn with_groups(mut self, groups: Vec<String>) -> Self {
        self.groups = groups;
        self
    }

    pub fn choice_options(&self) -> Vec<String> {
        self.picker
            .as_ref()
            .map(|picker| picker.options.clone())
            .unwrap_or_default()
    }

    pub fn value(&self, key: &str) -> &str {
        self.values.get(key).map(String::as_str).unwrap_or_default()
    }

    fn set(&mut self, key: &'static str, value: String) {
        self.values.insert(key, value);
    }

    pub fn driver(&self) -> &str {
        match self.value("driver") {
            "" => "sqlite",
            driver => driver,
        }
    }

    pub fn over_ssh(&self) -> bool {
        self.value("over_ssh") == "yes"
    }

    pub fn editing_field(&self) -> bool {
        self.editor.is_some()
    }

    pub fn fields(&self) -> Vec<(&'static str, &'static str)> {
        let mut fields = Vec::new();
        if self.creating {
            fields.push(("driver", "Driver"));
        }
        fields.push(("name", "Name"));
        if self.driver() == "sqlite" {
            fields.push(("database", "Path"));
        } else {
            fields.extend([
                ("host", "Host"),
                ("port", "Port"),
                ("database", "Database"),
                ("username", "Username"),
                ("password", "Password"),
                ("ssl_mode", "SSL mode"),
            ]);
            if !self.value("ssl_mode").trim().is_empty() {
                fields.extend([
                    ("ssl_ca", "SSL CA cert"),
                    ("ssl_cert", "SSL cert"),
                    ("ssl_key", "SSL key"),
                ]);
            }
            fields.push(("over_ssh", "Over SSH"));
            if self.over_ssh() {
                fields.extend(SSH_FIELDS);
            }
        }
        fields.extend([
            ("group_name", "Group"),
            ("tag", "Tag"),
            ("read_only", "Read only"),
        ]);
        fields
    }

    pub fn keys(&self) -> Vec<&'static str> {
        self.fields().into_iter().map(|(key, _)| key).collect()
    }

    pub fn current_key(&self) -> &'static str {
        self.keys().get(self.index).copied().unwrap_or("name")
    }

    pub fn choices(&self, key: &str) -> Option<Vec<&'static str>> {
        match key {
            "driver" => Some(DRIVERS.to_vec()),
            "ssl_mode" => Some(SSL_MODES.to_vec()),
            "read_only" | "over_ssh" => Some(YES_NO.to_vec()),
            _ => None,
        }
    }

    fn picked(key: &str) -> bool {
        FILES.contains(&key) || key == "tag" || key == "group_name"
    }

    pub fn move_by(&mut self, steps: isize) {
        let last = self.keys().len().saturating_sub(1);
        self.index = self.index.saturating_add_signed(steps).min(last);
    }

    pub fn cycle(&mut self, forward: bool) {
        let key = self.current_key();
        let Some(choices) = self.choices(key) else {
            return;
        };
        if key == "driver" {
            self.cycle_driver(forward);
            return;
        }
        let next = stepped(&choices, self.value(key), forward);
        self.set(key, next.to_string());
    }

    pub fn cycle_driver(&mut self, forward: bool) {
        let was = default_port(self.driver()).to_string();
        let next = stepped(&DRIVERS, self.driver(), forward);
        self.set("driver", next.to_string());
        if self.value("port") == was {
            self.set("port", String::new());
        }
        self.apply_driver_defaults();
        self.index = self.index.min(self.keys().len().saturating_sub(1));
    }

    fn apply_driver_defaults(&mut self) {
        if self.driver() == "sqlite" {
            return;
        }
        if self.value("host").is_empty() {
            self.set("host", "127.0.0.1".to_string());
        }
        if self.value("port").is_empty() {
            self.set("port", default_port(self.driver()).to_string());
        }
    }

    pub fn group(key: &str) -> &'static str {
        match key {
            "driver" | "name" => "",
            "over_ssh" => "ssh",
            "group_name" | "tag" | "read_only" => "labels",
            ssl if ssl.starts_with("ssl_") => "tls",
            ssh if ssh.starts_with("ssh_") => "ssh",
            _ => "where",
        }
    }

    fn starts_group(&self, key: &str) -> bool {
        let keys = self.keys();
        let Some(position) = keys.iter().position(|field| *field == key) else {
            return false;
        };
        position == 0 || Self::group(keys[position - 1]) != Self::group(key)
    }

    pub fn display(&self, key: &str) -> String {
        let value = self.value(key);
        if key == self.current_key()
            && let Some(editor) = &self.editor
        {
            return editor.text();
        }
        match key {
            "ssh_port" if value.is_empty() => "22".to_string(),
            "ssh_key" if value.is_empty() => "agent or ~/.ssh/config".to_string(),
            "ssl_mode" if value.is_empty() => "driver default".to_string(),
            "tag" if value.is_empty() => tag::NONE.to_string(),
            "group_name" if value.is_empty() => NO_GROUP.to_string(),
            "database" if value.is_empty() && self.driver() != "sqlite" => {
                "ask on connect".to_string()
            }
            "over_ssh" => yes_or_no(self.over_ssh()),
            "read_only" => yes_or_no(matches!(value, "1" | "yes")),
            secret if SECRETS.contains(&secret) && self.unreadable.contains(&secret) => {
                "•".repeat(8)
            }
            secret if SECRETS.contains(&secret) => "•".repeat(value.chars().count().min(8)),
            _ => value.to_string(),
        }
    }

    pub fn is_placeholder(&self, key: &str) -> bool {
        self.value(key).is_empty()
            && !["over_ssh", "read_only", "tag", "group_name"].contains(&key)
            && !SECRETS.contains(&key)
            && !self.display(key).is_empty()
    }

    pub fn on_key(&mut self, key: Key) -> Step {
        if self.picker.is_some() {
            self.on_picker_key(key);
            return Step::Stay;
        }

        if let Some(editor) = self.editor.as_mut() {
            match key {
                Key::Ctrl('s') => {
                    self.commit();
                    return Step::Save;
                }
                Key::Escape => self.editor = None,
                Key::Enter => self.commit(),
                other => editor.handle(other),
            }
            return Step::Stay;
        }

        let cycles = self.choices(self.current_key()).is_some();
        match key {
            Key::Escape | Key::Char('q') => return Step::Cancel,
            Key::Ctrl('s') => return Step::Save,
            Key::Up | Key::Char('k') => self.move_by(-1),
            Key::Down | Key::Char('j') => self.move_by(1),
            Key::Left | Key::Char('h') if cycles => self.cycle(false),
            Key::Right | Key::Char('l' | ' ') | Key::Enter if cycles => self.cycle(true),
            Key::Enter | Key::Char('i') if Self::picked(self.current_key()) => self.open_picker(),
            Key::Enter | Key::Char('i') if !cycles => self.start(),
            _ => {}
        }
        Step::Stay
    }

    pub fn on_paste(&mut self, text: &str) {
        let line = single_line(text);
        if let Some(picker) = self.picker.as_mut() {
            line.chars()
                .for_each(|character| picker.type_key(Key::Char(character)));
            return;
        }
        if let Some(editor) = self.editor.as_mut() {
            line.chars()
                .for_each(|character| editor.handle(Key::Char(character)));
        }
    }

    fn on_picker_key(&mut self, key: Key) {
        let Some(picker) = self.picker.as_mut() else {
            return;
        };
        match key {
            Key::Escape => self.picker = None,
            Key::Enter => self.choose(),
            Key::Up | Key::Ctrl('p') => picker.move_by(false),
            Key::Down | Key::Ctrl('n') => picker.move_by(true),
            other => picker.type_key(other),
        }
    }

    pub fn open_picker(&mut self) {
        let key = self.current_key();
        if !Self::picked(key) {
            return;
        }
        let title = key.replace('_', " ").to_uppercase();
        let chosen = self.value(key).to_string();
        self.picker = Some(match key {
            "tag" => {
                let chosen = if chosen.is_empty() {
                    tag::NONE.to_string()
                } else {
                    chosen
                };
                Choice::new(&title, tag::choices(), &chosen, false).with_colors(tag::colors())
            }
            "group_name" => {
                let chosen = if chosen.is_empty() {
                    NO_GROUP.to_string()
                } else {
                    chosen
                };
                let mut groups = vec![NO_GROUP.to_string()];
                groups.extend(self.groups.iter().cloned());
                Choice::new("GROUP", groups, &chosen, true)
            }
            _ => Choice::new(&title, self.files(key), &chosen, false),
        });
    }

    fn files(&self, key: &str) -> Vec<String> {
        let mut found = if key == "ssh_key" {
            key_files::ssh_keys(&self.places.home)
        } else {
            key_files::certificates(&self.places.home, &self.places.cwd)
        };
        found.push(TYPE_IT.to_string());
        found
    }

    fn choose(&mut self) {
        let chosen = self.picker.take().and_then(|picker| picker.selected());
        let Some(chosen) = chosen else {
            return;
        };
        if chosen == TYPE_IT {
            self.start();
            return;
        }
        let key = self.current_key();
        let value = match key {
            "tag" => tag::value(&chosen),
            "group_name" if chosen == NO_GROUP => String::new(),
            "group_name" => chosen.trim().to_string(),
            _ => chosen,
        };
        self.set(key, value);
    }

    pub fn start(&mut self) {
        self.editor = Some(LineEditor::holding(self.value(self.current_key())));
    }

    pub fn commit(&mut self) {
        let Some(editor) = self.editor.take() else {
            return;
        };
        let key = self.current_key();
        self.unreadable.retain(|unreadable| *unreadable != key);
        self.set(key, editor.text());
    }

    pub fn buffer(&self) -> String {
        self.editor
            .as_ref()
            .map(LineEditor::text)
            .unwrap_or_default()
    }

    pub fn cursor(&self) -> usize {
        self.editor
            .as_ref()
            .map(LineEditor::cursor)
            .unwrap_or_default()
    }

    pub fn save(&mut self, writer: &StoreWriter) -> anyhow::Result<Option<String>> {
        if self.value("name").trim().is_empty() {
            return Ok(Some("A name is required.".to_string()));
        }
        let sqlite = self.driver() == "sqlite";
        if sqlite && self.value("database").trim().is_empty() {
            return Ok(Some("A path to the .sqlite file is required.".to_string()));
        }
        if sqlite {
            let resolved =
                paths::resolve(self.value("database"), &self.places.home, &self.places.cwd);
            self.set("database", resolved);
        }
        if writer.name_taken(self.value("name"), self.id)? {
            return Ok(Some(
                "Another connection is already called that.".to_string(),
            ));
        }

        let fields = self.fields_to_save();
        match self.id {
            Some(id) => {
                writer.update(id, &fields)?;
            }
            None => self.id = Some(writer.create(&fields)?),
        }
        Ok(None)
    }

    fn fields_to_save(&self) -> Fields {
        let over_ssh = self.over_ssh();
        let mut fields: Fields = vec![("driver", Field::Text(self.driver().to_string()))];
        for key in self.keys() {
            if key == "driver" || key == "over_ssh" {
                continue;
            }
            if self.unreadable.contains(&key) {
                continue;
            }
            let value = self.value(key);
            let field = match key {
                "port" | "ssh_port" => value
                    .trim()
                    .parse()
                    .map(Field::Number)
                    .unwrap_or(Field::Null),
                "read_only" => Field::Flag(matches!(value, "yes" | "1")),
                "group_name" if value.trim().is_empty() => Field::Null,
                "group_name" => Field::Text(value.trim().to_string()),
                secret if SECRETS.contains(&secret) && value.is_empty() => Field::Null,
                secret if SECRETS.contains(&secret) => Field::Secret(value.to_string()),
                _ => Field::Text(value.to_string()),
            };
            fields.push((key, field));
        }
        if !over_ssh {
            fields.retain(|(key, _)| !key.starts_with("ssh_"));
            fields.extend(SSH_FIELDS.iter().map(|(key, _)| (*key, Field::Null)));
        }
        fields
    }

    pub fn name(&self) -> &str {
        self.value("name")
    }

    pub fn draw(&self, buffer: &mut Buffer, frame_width: u16, frame_lines: u16) {
        match &self.picker {
            Some(picker) => draw_picker(picker, buffer, frame_width, frame_lines),
            None => self.draw_form(buffer, frame_width, frame_lines),
        }
    }

    fn draw_form(&self, buffer: &mut Buffer, frame_width: u16, frame_lines: u16) {
        let lines = self.lines();
        let modal_width = lines.first().map(Line::width).unwrap_or_default() as u16;
        let left = centred(frame_width, modal_width);
        let top = frame_lines.saturating_sub(lines.len() as u16) / 2;
        let visible = (lines.len() as u16).min(frame_lines.saturating_sub(top));
        Island {
            bare: true,
            content: lines,
            ..Island::placed(left, top + 1, modal_width, visible)
        }
        .draw(buffer);
    }

    fn lines(&self) -> Vec<Line<'static>> {
        let fields = self.fields();
        let label_width = fields
            .iter()
            .map(|(_, label)| label.chars().count())
            .max()
            .unwrap_or_default();
        let inner = label_width + VALUE_WIDTH + 5;
        let edge = theme::border(true, false);
        let row = |spans: Vec<Span<'static>>| -> Line<'static> {
            let used: usize = spans.iter().map(Span::width).sum();
            let mut all = vec![Span::styled("│", edge)];
            all.extend(spans);
            all.push(Span::raw(" ".repeat(inner.saturating_sub(used))));
            all.push(Span::styled("│", edge));
            Line::from(all)
        };

        let title = if self.creating {
            " NEW CONNECTION "
        } else {
            " EDIT CONNECTION "
        };
        let mut lines = vec![
            Line::from(vec![
                Span::styled("╭─", edge),
                Span::styled(title, theme::title(true, false)),
                Span::styled(
                    format!("{}╮", "─".repeat(inner.saturating_sub(width(title) + 1))),
                    edge,
                ),
            ]),
            row(Vec::new()),
        ];

        let current = self.current_key();
        for (key, label) in fields {
            if self.starts_group(key) && !Self::group(key).is_empty() {
                lines.push(row(Vec::new()));
            }
            let focused = key == current;
            let label_style = if focused { theme::bold() } else { theme::dim() };
            let mut spans = vec![
                Span::raw("  "),
                Span::styled(pad(label, label_width), label_style),
                Span::raw("  "),
            ];
            if key == "tag"
                && let Some(color) = tag::color_of(Some(self.value("tag")))
            {
                spans.push(Span::styled("●", theme::paint(color)));
                spans.push(Span::raw(" "));
            }
            if focused && self.editor.is_some() {
                spans.extend(typing(&self.buffer(), self.cursor(), VALUE_WIDTH));
            } else {
                let shown = truncate(&self.display(key), VALUE_WIDTH);
                let shown = if focused && self.choices(key).is_some() {
                    format!("← {shown} →")
                } else {
                    shown
                };
                let style = if self.is_placeholder(key) && !focused {
                    theme::dim()
                } else {
                    Style::new()
                };
                spans.push(Span::styled(shown, style));
            }
            lines.push(row(spans));
        }

        lines.push(row(Vec::new()));
        let hint = match &self.error {
            Some(error) => Span::styled(
                error.clone(),
                theme::paint("red").add_modifier(Modifier::DIM),
            ),
            None => Span::styled(
                if self.editor.is_some() {
                    "↵ keeps it    esc drops it"
                } else if self.choices(current).is_some() {
                    "← → changes it    ctrl+s save    esc cancel"
                } else {
                    "↵ change    ctrl+s save    esc cancel"
                },
                theme::dim(),
            ),
        };
        lines.push(row(vec![Span::raw("  "), hint]));
        lines.push(Line::styled(format!("╰{}╯", "─".repeat(inner)), edge));
        lines
    }
}

fn draw_picker(picker: &Choice, buffer: &mut Buffer, frame_width: u16, frame_lines: u16) {
    let box_width = frame_width.saturating_sub(6).min(PICKER_WIDTH);
    let box_height = picker.rows();
    let top = frame_lines.saturating_sub(box_height) / 2;
    let left = centred(frame_width, box_width);

    let first = top.saturating_sub(1);
    let last = (top + box_height).min(frame_lines.saturating_sub(1));
    Island {
        bare: true,
        ..Island::placed(
            left.saturating_sub(2).max(1),
            first + 1,
            box_width + 4,
            last.saturating_sub(first) + 1,
        )
    }
    .draw(buffer);

    let content = picker.content(
        box_width.saturating_sub(2) as usize,
        box_height.saturating_sub(2) as usize,
    );
    let visible = (content.len() as u16 + 2).min(frame_lines.saturating_sub(top));
    Island {
        title: picker.title.clone(),
        focused: true,
        modal: true,
        content,
        ..Island::placed(left, top + 1, box_width, visible)
    }
    .draw(buffer);
}

fn centred(frame_width: u16, box_width: u16) -> u16 {
    ((frame_width as i32 - box_width as i32) / 2 + 1).max(1) as u16
}

fn typing(text: &str, cursor: usize, room: usize) -> Vec<Span<'static>> {
    let characters: Vec<char> = text.chars().collect();
    let start = (cursor + 1).saturating_sub(room);
    let mut visible: Vec<char> = characters.iter().skip(start).take(room).copied().collect();
    if start > 0
        && let Some(first) = visible.first_mut()
    {
        *first = '…';
    }
    let at = cursor - start;
    let before: String = visible.iter().take(at).collect();
    let under = visible.get(at).map_or(" ".to_string(), char::to_string);
    let after: String = visible.iter().skip(at + 1).collect();
    vec![
        Span::raw(before),
        Span::styled(under, Style::new().add_modifier(Modifier::REVERSED)),
        Span::raw(after),
    ]
}

fn stepped<'a>(choices: &[&'a str], current: &str, forward: bool) -> &'a str {
    let at = choices
        .iter()
        .position(|choice| *choice == current)
        .unwrap_or(0);
    let count = choices.len();
    let next = if forward {
        (at + 1) % count
    } else {
        (at + count - 1) % count
    };
    choices[next]
}

fn default_port(driver: &str) -> u16 {
    crate::dsn::default_port(driver)
}

fn yes_or_no(flag: bool) -> String {
    YES_NO[usize::from(flag)].to_string()
}
