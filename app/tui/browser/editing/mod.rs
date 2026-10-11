use std::borrow::Cow;

use super::*;
use crate::database::{self, Column, Named, Values};
use crate::tui::editor::QueryEditor;
use crate::tui::grid::Tone;
use crate::tui::history::{self, Execution};
use crate::tui::record_form::RecordForm;
use crate::tui::value_editor::ValueView;
use crate::tui::{json, now};

mod record;
mod render;
mod rows;
mod value;
mod write;

const READ_ONLY: &str = "this connection is marked read-only";

#[derive(Default)]
pub struct Edits {
    pub deletes: Vec<String>,
    pub updates: Vec<(String, Values)>,
    pub inserts: Vec<Values>,
    value: Option<ValueEditor>,
    form: Option<RecordForm>,
    viewer: Option<(Area, usize)>,
}

struct ValueEditor {
    buffer: QueryEditor,
    column: String,
    column_type: Option<String>,
    json: bool,
    reason: Option<String>,
    anchor: Option<usize>,
    count: String,
}

impl ValueEditor {
    fn editable(&self) -> bool {
        self.reason.is_none()
    }

    fn selected_lines(&self) -> (usize, usize) {
        let cursor = self.buffer.cursor_line();
        let anchor = self.anchor.unwrap_or(cursor);
        (anchor.min(cursor), anchor.max(cursor))
    }
}

struct Failure {
    title: &'static str,
    error: anyhow::Error,
    note: &'static str,
}

impl Edits {
    pub fn pending(&self) -> bool {
        !self.deletes.is_empty() || !self.updates.is_empty() || !self.inserts.is_empty()
    }

    pub fn editing(&self) -> Option<String> {
        self.value.as_ref().map(|value| value.buffer.buffer())
    }

    pub fn editable(&self) -> bool {
        self.value.as_ref().is_some_and(ValueEditor::editable)
    }

    pub fn read_only_reason(&self) -> Option<&str> {
        self.value.as_ref()?.reason.as_deref()
    }

    pub fn form(&self) -> Option<&RecordForm> {
        self.form.as_ref()
    }

    pub(super) fn captures_keys(&self) -> bool {
        self.value.is_some() || self.form.is_some()
    }

    pub(super) fn forget(&mut self) {
        self.deletes.clear();
        self.updates.clear();
        self.inserts.clear();
    }

    fn update_for(&mut self, key_value: &str) -> &mut Values {
        let at = match self
            .updates
            .iter()
            .position(|(candidate, _)| candidate == key_value)
        {
            Some(at) => at,
            None => {
                self.updates.push((key_value.to_string(), Values::new()));
                self.updates.len() - 1
            }
        };
        &mut self.updates[at].1
    }

    fn update_of(&self, key_value: &str) -> Option<&Values> {
        self.updates
            .iter()
            .find(|(candidate, _)| candidate == key_value)
            .map(|(_, values)| values)
    }

    fn status(&self) -> String {
        let plural = |count: usize| if count == 1 { "" } else { "s" };
        let (adds, edits, deletes) = (self.inserts.len(), self.updates.len(), self.deletes.len());
        if adds + edits + deletes == 0 {
            return "nothing pending".to_string();
        }
        let mut parts = Vec::new();
        if adds > 0 {
            parts.push(format!("{adds} row{} added", plural(adds)));
        }
        if edits > 0 {
            parts.push(format!("{edits} row{} edited", plural(edits)));
        }
        if deletes > 0 {
            parts.push(format!("{deletes} marked for deletion"));
        }
        format!("{} · :w writes · u clears", parts.join(" · "))
    }
}

impl Browser {
    pub(super) fn key_column(&self) -> Option<String> {
        self.keys.get(&self.current_table()?).cloned()
    }

    fn key_value(&self, row: usize) -> Option<String> {
        let key = self.key_column()?;
        let position = self.headers.iter().position(|header| *header == key)?;
        self.raw.get(row)?.get(position)?.clone()
    }

    fn on_added_row(&self) -> bool {
        !self.query.results && self.row_index < self.edits.inserts.len()
    }

    fn why_read_only(&self) -> Option<String> {
        if self.connection.read_only {
            return Some(READ_ONLY.to_string());
        }
        if self.query.results {
            return Some("query results have no row to write back to".to_string());
        }
        if self.on_added_row() || self.key_column().is_some() {
            return None;
        }
        Some(format!(
            "{} has no single-column primary key",
            self.current_table().unwrap_or_default()
        ))
    }

    fn columns(&mut self) -> Vec<Column> {
        let Some(table) = self.current_table() else {
            return Vec::new();
        };
        self.database.columns(&table).unwrap_or_default()
    }
}

fn question(_: usize) -> String {
    "?".to_string()
}

// A number as tql reads one: whole numbers exactly, however large, and
// anything with a fraction or an exponent through a float.
fn whole(text: &str) -> i64 {
    let text = text.trim();
    text.parse::<i64>()
        .ok()
        .or_else(|| text.parse::<f64>().ok().map(|number| number as i64))
        .unwrap_or(0)
}
