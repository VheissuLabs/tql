use std::collections::HashSet;
use std::path::{Path, PathBuf};

use toml::{Table, Value};

use super::filters::LineEditor;
use crate::config_file::kinds::{self, Kind};
use crate::config_file::template::{self, SETTINGS, Setting};
use crate::config_file::{self, file};
use crate::settings;
use crate::terminal::Key;

mod render;

pub fn offers() -> Vec<(String, &'static str)> {
    vec![
        ("↑↓".into(), "Move"),
        ("↵".into(), "Change"),
        ("←→".into(), "Choose"),
        ("x".into(), "Default"),
        ("/".into(), "Find"),
        ("esc".into(), "Close"),
    ]
}

pub const BAR: &str = " ↑↓ Move    ↵ Change    ←→ Choose    x Default    / Find    esc Close";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Row {
    Header(&'static str),
    Setting(&'static Setting),
}

pub struct SettingsModal {
    path: PathBuf,
    values: Table,
    folded: HashSet<&'static str>,
    index: usize,
    filter: String,
    finding: bool,
    editing: Option<LineEditor>,
    message: Option<Message>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Message {
    Said(String),
    Failed(String),
}

pub fn title(section: &str) -> &'static str {
    match section {
        "ui" => "UI",
        "theme" => "Theme",
        "updates" => "Updates",
        "ai" => "AI",
        "mcp" => "MCP",
        "icons" => "Icons",
        _ => "Other",
    }
}

fn parsed(literal: &str) -> Value {
    toml::from_str::<Table>(&format!("value = {literal}"))
        .ok()
        .and_then(|mut table| table.remove("value"))
        .unwrap_or(Value::String(String::new()))
}

impl SettingsModal {
    pub fn open(path: &Path) -> Self {
        let mut modal = SettingsModal {
            path: path.to_path_buf(),
            values: Table::new(),
            folded: HashSet::new(),
            index: 0,
            filter: String::new(),
            finding: false,
            editing: None,
            message: None,
        };
        modal.reread();
        modal
    }

    fn reread(&mut self) {
        let table = std::fs::read_to_string(&self.path)
            .ok()
            .and_then(|source| toml::from_str::<Table>(&source).ok())
            .unwrap_or_default();
        self.values = file::hoist(table).0;
    }

    pub fn rows(&self) -> Vec<Row> {
        let needle = self.filter.trim().to_lowercase();
        let mut rows = Vec::new();
        for section in template::sections() {
            let matching: Vec<&'static Setting> = SETTINGS
                .iter()
                .filter(|setting| setting.section == section)
                .filter(|setting| needle.is_empty() || setting.key.contains(&needle))
                .collect();
            if matching.is_empty() {
                continue;
            }
            rows.push(Row::Header(section));
            if needle.is_empty() && self.folded.contains(section) {
                continue;
            }
            rows.extend(matching.into_iter().map(Row::Setting));
        }
        rows
    }

    pub fn current(&self) -> Option<Row> {
        self.rows().get(self.index).copied()
    }

    pub fn value(&self, setting: &Setting) -> Value {
        self.values
            .get(setting.section)
            .and_then(Value::as_table)
            .and_then(|section| section.get(setting.key))
            .cloned()
            .unwrap_or_else(|| parsed(setting.default))
    }

    pub fn is_default(&self, setting: &Setting) -> bool {
        self.value(setting) == parsed(setting.default)
    }

    pub fn message(&self) -> Option<&Message> {
        self.message.as_ref()
    }

    pub fn is_folded(&self, section: &str) -> bool {
        self.folded.contains(section)
    }

    pub fn on_paste(&mut self, text: &str) {
        let text = super::text::single_line(text);
        if let Some(editor) = self.editing.as_mut() {
            text.chars()
                .for_each(|character| editor.handle(Key::Char(character)));
        } else if self.finding {
            self.filter.push_str(&text);
            self.index = 0;
        }
    }

    pub fn on_key(&mut self, key: Key) -> bool {
        if self.editing.is_some() {
            self.type_into_value(key);
            return true;
        }
        if self.finding {
            self.type_into_filter(key);
            return true;
        }
        let rows = self.rows();
        let row = rows.get(self.index).copied();
        match key {
            Key::Escape | Key::Char('q') if !self.filter.is_empty() => {
                self.filter.clear();
                self.index = 0;
            }
            Key::Escape | Key::Char('q') => return false,
            Key::Up | Key::Char('k') => self.index = self.index.saturating_sub(1),
            Key::Down | Key::Char('j') => {
                self.index = (self.index + 1).min(rows.len().saturating_sub(1))
            }
            Key::Char('/') => {
                self.finding = true;
                self.message = None;
            }
            Key::Char('x') => {
                if let Some(Row::Setting(setting)) = row {
                    self.save(setting, parsed(setting.default));
                }
            }
            Key::Enter | Key::Char(' ') => match row {
                Some(Row::Header(section)) => self.fold(section, !self.is_folded(section)),
                Some(Row::Setting(setting)) => self.change(setting),
                None => {}
            },
            Key::Left | Key::Char('h') | Key::Right | Key::Char('l') => {
                let forward = matches!(key, Key::Right | Key::Char('l'));
                match row {
                    Some(Row::Header(section)) => self.fold(section, !forward),
                    Some(Row::Setting(setting)) => self.step(setting, forward),
                    None => {}
                }
            }
            _ => {}
        }
        true
    }

    fn fold(&mut self, section: &'static str, folded: bool) {
        if !self.filter.is_empty() {
            return;
        }
        if folded {
            self.folded.insert(section);
        } else {
            self.folded.remove(section);
        }
        self.index = self
            .rows()
            .iter()
            .position(|row| *row == Row::Header(section))
            .unwrap_or(0);
    }

    fn type_into_filter(&mut self, key: Key) {
        match key {
            Key::Escape => {
                self.finding = false;
                self.filter.clear();
            }
            Key::Enter => self.finding = false,
            Key::Backspace | Key::Ctrl('h') => {
                if self.filter.pop().is_none() {
                    self.finding = false;
                }
            }
            Key::Char(character) => self.filter.push(character),
            _ => return,
        }
        self.index = self
            .rows()
            .iter()
            .position(|row| matches!(row, Row::Setting(_)))
            .unwrap_or(0);
    }

    fn change(&mut self, setting: &'static Setting) {
        match kinds::kind(setting) {
            Some(Kind::Flag) => {
                let on = self.value(setting).as_bool().unwrap_or_default();
                self.save(setting, Value::Boolean(!on));
            }
            Some(Kind::Choice(_)) => self.step(setting, true),
            Some(Kind::Secret) => self.editing = Some(LineEditor::default()),
            Some(_) => {
                let shown = match self.value(setting) {
                    Value::String(text) => text,
                    other => other.to_string(),
                };
                self.editing = Some(LineEditor::holding(&shown));
            }
            None => {}
        }
        if self.editing.is_some() {
            self.message = None;
        }
    }

    fn step(&mut self, setting: &'static Setting, forward: bool) {
        let Some(Kind::Choice(choices)) = kinds::kind(setting) else {
            return;
        };
        let current = self.value(setting);
        let at = choices
            .iter()
            .position(|choice| Some(*choice) == current.as_str());
        let next = match (at, forward) {
            (None, _) => 0,
            (Some(at), true) => (at + 1) % choices.len(),
            (Some(at), false) => (at + choices.len() - 1) % choices.len(),
        };
        self.save(setting, Value::String(choices[next].to_string()));
    }

    fn type_into_value(&mut self, key: Key) {
        let Some(editor) = self.editing.as_mut() else {
            return;
        };
        match key {
            Key::Escape => self.editing = None,
            Key::Enter => {
                let typed = editor.text();
                let Some(Row::Setting(setting)) = self.current() else {
                    self.editing = None;
                    return;
                };
                match read(setting, typed.trim()) {
                    Ok(value) => {
                        self.editing = None;
                        self.save(setting, value);
                    }
                    Err(refusal) => self.message = Some(Message::Failed(refusal)),
                }
            }
            other => editor.handle(other),
        }
    }

    pub fn editing(&self) -> Option<&LineEditor> {
        self.editing.as_ref()
    }

    pub fn filter(&self) -> Option<&str> {
        (self.finding || !self.filter.is_empty()).then_some(self.filter.as_str())
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    fn save(&mut self, setting: &'static Setting, value: Value) {
        if let Err(error) = file::write(&self.path, setting, &value.to_string()) {
            self.message = Some(Message::Failed(format!(
                "could not write {}: {error}",
                self.path.display()
            )));
            return;
        }
        self.reread();
        let loaded = config_file::load(&self.path);
        if let Some(error) = loaded.error {
            self.message = Some(Message::Failed(error));
            return;
        }
        settings::install(loaded.settings);
        let shown = render::shown(setting, &self.value(setting));
        let later = if kinds::applies_at_start(setting) {
            " · applies next start"
        } else {
            ""
        };
        self.message = Some(Message::Said(format!("{} is {shown}{later}", setting.key)));
    }
}

fn read(setting: &Setting, typed: &str) -> Result<Value, String> {
    match kinds::kind(setting) {
        Some(Kind::Whole { least, most }) => typed
            .parse::<i64>()
            .ok()
            .filter(|number| (least..=most).contains(number))
            .map(Value::Integer)
            .ok_or_else(|| {
                format!(
                    "{} takes a whole number from {least} to {most}",
                    setting.key
                )
            }),
        Some(Kind::Seconds { most }) => typed
            .parse::<f64>()
            .ok()
            .filter(|seconds| (0.0..=most).contains(seconds))
            .map(|seconds| match seconds.fract() == 0.0 {
                true => Value::Integer(seconds as i64),
                false => Value::Float(seconds),
            })
            .ok_or_else(|| format!("{} takes seconds from 0 to {most}", setting.key)),
        _ => Ok(Value::String(typed.to_string())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setting(key: &str) -> &'static Setting {
        SETTINGS.iter().find(|setting| setting.key == key).unwrap()
    }

    #[test]
    fn reads_a_typed_value_by_its_kind_and_refuses_one_out_of_range() {
        assert_eq!(read(setting("sidebar_width"), "40"), Ok(Value::Integer(40)));
        assert_eq!(
            read(setting("sidebar_width"), "4"),
            Err("sidebar_width takes a whole number from 8 to 500".to_string())
        );
        assert_eq!(
            read(setting("mouse_row_offset"), "-2"),
            Ok(Value::Integer(-2))
        );
        assert_eq!(
            read(setting("status_seconds"), "2.5"),
            Ok(Value::Float(2.5))
        );
        assert_eq!(read(setting("status_seconds"), "3"), Ok(Value::Integer(3)));
        assert!(read(setting("status_seconds"), "soon").is_err());
        assert_eq!(
            read(setting("time_zone"), "America/Toronto"),
            Ok(Value::String("America/Toronto".to_string()))
        );
    }
}
