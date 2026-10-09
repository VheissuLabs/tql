use std::borrow::Cow;

use super::*;
use crate::database::{Column, Named, Values};
use crate::tui::editor::QueryEditor;
use crate::tui::grid::Tone;
use crate::tui::record_form::RecordForm;
use crate::tui::value_editor::ValueView;
use crate::tui::{json, now};

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

    pub(super) fn start_editing(&mut self, read_only: bool) {
        if self.raw.is_empty() {
            self.say("nothing to open, this table has no rows");
            return;
        }
        self.focus = Focus::Grid;
        let reason = if read_only {
            Some("opened with I, press e to edit".to_string())
        } else {
            self.why_read_only()
        };
        let text = self
            .raw
            .get(self.row_index)
            .and_then(|row| row.get(self.column_index))
            .cloned()
            .flatten()
            .unwrap_or_default();
        let json = json::looks_like_json(Some(&text));
        let mut buffer = QueryEditor::holding(&if json { json::pretty(&text) } else { text });
        if json {
            buffer.move_to_start();
        }
        let column = self
            .headers
            .get(self.column_index)
            .cloned()
            .unwrap_or_default();
        let column_type = self
            .columns()
            .into_iter()
            .find(|candidate| candidate.name == column)
            .map(|candidate| candidate.type_name);

        self.edits.value = Some(ValueEditor {
            buffer,
            column,
            column_type,
            json,
            reason,
            anchor: None,
            count: String::new(),
        });
    }

    pub(super) fn type_into_edits(&mut self, key: Key) {
        if self.edits.form.is_some() {
            self.type_into_form(key);
        } else if self.edits.value.is_some() {
            self.type_into_value(key);
        }
    }

    fn type_into_value(&mut self, key: Key) {
        let Some(value) = self.edits.value.as_mut() else {
            return;
        };
        let editable = value.editable();

        if !editable && key == Key::Char('e') {
            self.edits.value = None;
            self.start_editing(false);
            return;
        }
        if !editable && !matches!(key, Key::Escape | Key::Char('q')) {
            self.scroll_value(key);
            return;
        }
        if key == Key::Escape && value.anchor.is_some() {
            value.anchor = None;
            self.say("selection cleared");
            return;
        }
        if key == Key::Escape || !editable && key == Key::Char('q') {
            self.edits.value = None;
            self.say("edit cancelled");
            return;
        }

        match key {
            Key::ShiftEnter | Key::Alt('\r' | '\n') => value.buffer.handle(Key::Enter),
            Key::Enter | Key::Ctrl('s' | 'd') => self.commit_edit(),
            Key::Ctrl('t') => {
                let typed = now::for_type(value.column_type.as_deref());
                value.buffer.set(&typed);
                self.say("now");
            }
            other => value.buffer.handle(other),
        }
    }

    fn scroll_value(&mut self, key: Key) {
        let Some(value) = self.edits.value.as_mut() else {
            return;
        };
        if let Key::Char(digit @ '0'..='9') = key
            && (digit != '0' || !value.count.is_empty())
        {
            value.count.push(digit);
            return;
        }

        let typed: usize = value.count.parse().unwrap_or(0);
        value.count.clear();
        match key {
            Key::Down | Key::Char('j') => {
                (0..typed.max(1)).for_each(|_| value.buffer.handle(Key::Down))
            }
            Key::Up | Key::Char('k') => {
                (0..typed.max(1)).for_each(|_| value.buffer.handle(Key::Up))
            }
            Key::Char('g') => value.buffer.move_to_line_column(0, 0),
            Key::Char('G') if typed == 0 => value.buffer.move_to_end(),
            Key::Char('G') => value.buffer.move_to_line_column(typed - 1, 0),
            Key::Char('V') => {
                value.anchor = match value.anchor {
                    Some(_) => None,
                    None => Some(value.buffer.cursor_line()),
                };
                let said = if value.anchor.is_some() {
                    "visual: move with j/k, y yanks, esc clears"
                } else {
                    "selection cleared"
                };
                self.say(said);
            }
            Key::Char('y') => {
                let (from, to) = value.selected_lines();
                let lines = &value.buffer.lines()[from..=to];
                value.anchor = None;
                let reached = self.clipboard.copy(&lines.join("\n"));
                let plural = if lines.len() == 1 { "" } else { "s" };
                self.say(format!(
                    "yanked {} line{plural} to the {} clipboard",
                    lines.len(),
                    reached.name()
                ));
            }
            _ => {}
        }
    }

    fn commit_edit(&mut self) {
        let Some(value) = self.edits.value.as_ref().filter(|value| value.editable()) else {
            return;
        };
        let typed = value.buffer.buffer();
        if value.json && !json::looks_like_json(Some(&typed)) {
            self.say("not valid json: fix it or press esc to cancel");
            return;
        }
        let typed = match (value.json, now::asked(&typed)) {
            (true, _) => json::compact(&typed),
            (false, true) => now::for_type(value.column_type.as_deref()),
            (false, false) => typed,
        };
        let kept = (!typed.is_empty()).then_some(typed);
        self.edits.value = None;

        let Some(column) = self.headers.get(self.column_index).cloned() else {
            self.say("nothing to save");
            return;
        };

        if self.on_added_row() {
            let at = self.row_index;
            match kept {
                Some(kept) => self.edits.inserts[at].put(&column, Some(kept)),
                None => self.edits.inserts[at].forget(&column),
            }
            self.show_added_row(at);
            self.say(self.edits.status());
            return;
        }

        if self.key_column().is_none() {
            self.say("nothing to save");
            return;
        }
        let key_value = self.key_value(self.row_index).unwrap_or_default();
        self.edits.update_for(&key_value).put(&column, kept);
        self.say(self.edits.status());
    }

    pub(super) fn mark_delete(&mut self) {
        if self.query.results {
            self.say("query results have no row to delete, open the table itself");
            return;
        }
        if self.key_column().is_none() {
            self.say("no primary key on this table, so a row cannot be deleted safely");
            return;
        }
        if self.connection.read_only {
            self.say(READ_ONLY);
            return;
        }
        self.focus = Focus::Grid;
        if self.on_added_row() {
            return;
        }
        let Some(key_value) = self.key_value(self.row_index) else {
            return;
        };

        match self
            .edits
            .deletes
            .iter()
            .position(|marked| *marked == key_value)
        {
            Some(at) => {
                self.edits.deletes.remove(at);
            }
            None => self.edits.deletes.push(key_value),
        }
        self.row_index = (self.row_index + 1).min(self.rows.len().saturating_sub(1));
        self.say(self.edits.status());
    }

    pub(super) fn clear_marks(&mut self) {
        if !self.edits.pending() {
            return;
        }
        let added = !self.edits.inserts.is_empty();
        self.edits.forget();
        if added {
            self.load(true);
        }
        self.say("pending changes dropped");
    }

    pub(super) fn leave(&mut self, exit: Exit) -> Flow<Exit> {
        let count = self.edits.deletes.len() + self.edits.updates.len() + self.edits.inserts.len();
        if count == 0 {
            return Flow::Exit(exit);
        }
        self.edits.forget();
        let plural = if count == 1 { "" } else { "s" };
        self.say(format!(
            "{count} unwritten change{plural} dropped, :q again to quit"
        ));
        Flow::Continue
    }

    fn filled_by_database(&self, name: &str, columns: &[Column]) -> bool {
        let Some(column) = columns.iter().find(|column| column.name == name) else {
            return false;
        };
        column.auto_increment
            || self.connection.driver == "sqlite"
                && self.key_column().as_deref() == Some(name)
                && column.type_name.eq_ignore_ascii_case("integer")
    }

    fn suggested_key(&mut self, table: &str, columns: &[Column]) -> Values {
        let Some(key) = self.key_column() else {
            return Values::new();
        };
        let Some(column) = columns.iter().find(|column| column.name == key) else {
            return Values::new();
        };
        let required = !self.filled_by_database(&key, columns)
            && !column.nullable
            && column.default.as_deref().unwrap_or_default().is_empty();
        let numeric = ["int", "serial", "numeric", "decimal"]
            .iter()
            .any(|shape| column.type_name.to_lowercase().contains(shape));
        if !required || !numeric {
            return Values::new();
        }
        let Ok(highest) = self.database.highest(table, &key) else {
            return Values::new();
        };
        let highest = highest
            .and_then(|highest| highest.trim().parse::<f64>().ok())
            .unwrap_or(0.0) as i64;
        vec![(key, Some((highest + 1).to_string()))]
    }

    pub(super) fn new_row(&mut self) {
        if self.query.results {
            self.say("query results have no table to add to");
            return;
        }
        let Some(table) = self.current_table().filter(|_| !self.headers.is_empty()) else {
            self.say("no table to add to");
            return;
        };
        if self.connection.read_only {
            self.say(READ_ONLY);
            return;
        }
        let columns = self.columns();
        let generated: Vec<String> = self
            .headers
            .iter()
            .filter(|header| self.filled_by_database(header, &columns))
            .cloned()
            .collect();
        let given = self.suggested_key(&table, &columns);
        self.edits.form = Some(RecordForm::adding(
            &table,
            &columns,
            &self.connection.driver,
            &generated,
            &given,
        ));
        self.hush();
    }

    pub(super) fn open_record(&mut self) {
        if self.raw.is_empty() {
            self.say("nothing to open, this table has no rows");
            return;
        }
        if let Some(reason) = self.why_read_only() {
            self.say(reason);
            return;
        }
        let table = self.current_table().unwrap_or_default();
        let columns = self.columns();

        let mut form = if self.on_added_row() {
            let generated: Vec<String> = self
                .headers
                .iter()
                .filter(|header| self.filled_by_database(header, &columns))
                .cloned()
                .collect();
            RecordForm::pending(
                &table,
                &columns,
                &self.connection.driver,
                &generated,
                &self.edits.inserts[self.row_index],
                self.row_index,
            )
        } else {
            let key = self.key_column().unwrap_or_default();
            let mut row: Values = self
                .headers
                .iter()
                .cloned()
                .zip(self.raw[self.row_index].iter().cloned())
                .collect();
            let key_value = self.key_value(self.row_index).unwrap_or_default();
            for (column, value) in self
                .edits
                .update_of(&key_value)
                .cloned()
                .unwrap_or_default()
            {
                row.put(&column, value);
            }
            RecordForm::editing(&table, &columns, &row, &key)
        };

        let header = self.headers.get(self.column_index);
        form.jump(
            form.fields
                .iter()
                .position(|field| Some(&field.name) == header)
                .unwrap_or(0),
        );
        self.edits.form = Some(form);
        self.focus = Focus::Grid;
        self.hush();
    }

    fn type_into_form(&mut self, key: Key) {
        let Some(form) = self.edits.form.as_mut() else {
            return;
        };
        if form.editor.is_some() {
            self.type_into_field(key);
            return;
        }
        if key != Key::Escape {
            form.discarding = false;
        }
        match key {
            Key::Escape => self.close_record(),
            Key::Ctrl('s') => self.save_record(),
            Key::Up | Key::Char('k') | Key::BackTab => form.move_by(-1),
            Key::Down | Key::Char('j') | Key::Tab => form.move_by(1),
            Key::Char('g') => form.jump(0),
            Key::Char('G') => form.jump(usize::MAX),
            Key::Enter | Key::Char('e') => form.start(),
            Key::Ctrl('n') => form.set_null(),
            Key::Backspace | Key::Ctrl('h') | Key::Delete => form.reset(),
            _ => {}
        }
    }

    fn type_into_field(&mut self, key: Key) {
        let Some(form) = self.edits.form.as_mut() else {
            return;
        };
        let inline = !form.expanded;
        let step = match key {
            Key::Escape => return form.abandon(),
            Key::Ctrl('t') => return form.now(),
            Key::Ctrl('s') => return self.save_record(),
            Key::ShiftEnter | Key::Alt('\r' | '\n') => {
                form.expanded = true;
                return form.type_key(Key::Enter);
            }
            Key::Enter | Key::Tab | Key::Ctrl('d') => 1,
            Key::BackTab => -1,
            Key::Down if inline => 1,
            Key::Up if inline => -1,
            other => return form.type_key(other),
        };
        if form.keep() {
            form.move_by(step);
        }
    }

    fn close_record(&mut self) {
        let Some(form) = self.edits.form.as_mut() else {
            return;
        };
        let dirty = form.dirty();
        if dirty && !form.discarding {
            form.discarding = true;
            return;
        }
        self.edits.form = None;
        if dirty {
            self.say("row thrown away");
        } else {
            self.hush();
        }
    }

    fn save_record(&mut self) {
        if !self.edits.form.as_mut().is_some_and(RecordForm::keep) {
            return;
        }
        let Some(form) = self.edits.form.take() else {
            return;
        };
        let values = form.values();
        self.focus = Focus::Grid;

        if let Some(at) = form.insert_at {
            self.edits.inserts[at] = values;
            self.load(true);
        } else if form.adds() {
            self.edits.inserts.insert(0, values);
            self.load(true);
            self.row_index = 0;
        } else if values.is_empty() {
            self.say("nothing changed");
            return;
        } else {
            let key_value = form.key_value.clone().unwrap_or_default();
            let update = self.edits.update_for(&key_value);
            for (column, value) in values {
                update.put(&column, value);
            }
        }
        self.say(self.edits.status());
    }

    pub(super) fn write_pending(&mut self) {
        if !self.edits.pending() {
            self.say("nothing to write");
            return;
        }
        let Some(table) = self.current_table() else {
            return;
        };
        let key = self.key_column();
        if key.is_none() && (!self.edits.updates.is_empty() || !self.edits.deletes.is_empty()) {
            return;
        }
        let (adds, edits, deletes) = (
            self.edits.inserts.len(),
            self.edits.updates.len(),
            self.edits.deletes.len(),
        );

        if let Err(failure) = self.write_all(&table, key.as_deref().unwrap_or_default()) {
            self.load(true);
            self.fail(
                failure.error.to_string(),
                vec![failure.note.to_string()],
                failure.title,
            );
            return;
        }

        self.load(true);
        let plural = |count: usize| if count == 1 { "" } else { "s" };
        let mut written = Vec::new();
        if adds > 0 {
            written.push(format!("{adds} row{} added", plural(adds)));
        }
        if edits > 0 {
            written.push(format!("{edits} row{} updated", plural(edits)));
        }
        if deletes > 0 {
            written.push(format!("{deletes} deleted"));
        }
        self.say(format!("wrote {}", written.join(" · ")));
    }

    fn write_all(&mut self, table: &str, key: &str) -> Result<(), Failure> {
        let failed = |title, error, note| Failure { title, error, note };

        while let Some(values) = self.edits.inserts.first() {
            self.database.insert(table, values).map_err(|error| {
                failed(
                    "COULD NOT ADD THE ROW",
                    error,
                    "The row is still here. Fix it and :w again, or u to drop it.",
                )
            })?;
            self.edits.inserts.remove(0);
        }

        while let Some((key_value, columns)) = self.edits.updates.first() {
            for (column, value) in columns {
                self.database
                    .update(table, column, key, key_value, value.as_deref())
                    .map_err(|error| {
                        failed(
                            "COULD NOT WRITE",
                            error,
                            "The change is still here. Fix it and :w again, or u to drop it.",
                        )
                    })?;
            }
            self.edits.updates.remove(0);
        }

        if !self.edits.deletes.is_empty() {
            self.database
                .delete(table, key, &self.edits.deletes)
                .map_err(|error| {
                    failed(
                        "COULD NOT DELETE",
                        error,
                        "The rows are still marked. u clears them.",
                    )
                })?;
            self.edits.deletes.clear();
        }
        Ok(())
    }

    fn added_row(&self, values: &Values) -> (Vec<Option<String>>, Vec<String>) {
        self.headers
            .iter()
            .map(|header| match values.named(header) {
                None => (None, String::new()),
                Some(None) => (None, text::cell(None)),
                Some(Some(value)) => (Some(value.clone()), text::cell(Some(value))),
            })
            .unzip()
    }

    fn show_added_row(&mut self, at: usize) {
        let (raw, shown) = self.added_row(&self.edits.inserts[at]);
        self.raw[at] = raw;
        self.rows[at] = shown;
    }

    pub(super) fn prepend_pending_rows(&mut self) {
        let (raw, shown): (Vec<_>, Vec<_>) = self
            .edits
            .inserts
            .iter()
            .map(|values| self.added_row(values))
            .unzip();
        self.raw.splice(0..0, raw);
        self.rows.splice(0..0, shown);
    }

    pub(super) fn shown_rows(&self) -> Cow<'_, [Vec<String>]> {
        if self.edits.updates.is_empty() || self.key_column().is_none() {
            return Cow::Borrowed(&self.rows);
        }
        let mut rows = self.rows.clone();
        for (index, row) in rows.iter_mut().enumerate() {
            let Some(values) = self
                .key_value(index)
                .and_then(|key_value| self.edits.update_of(&key_value))
            else {
                continue;
            };
            for (column, value) in values {
                if let Some(position) = self.headers.iter().position(|header| header == column) {
                    row[position] = text::cell(value.as_deref());
                }
            }
        }
        Cow::Owned(rows)
    }

    pub(super) fn tones(&self) -> Vec<Option<Tone>> {
        if !self.edits.pending() {
            return Vec::new();
        }
        (0..self.rows.len())
            .map(|index| {
                if !self.query.results && index < self.edits.inserts.len() {
                    return Some(Tone::Added);
                }
                let key_value = self.key_value(index)?;
                if self.edits.deletes.contains(&key_value) {
                    return Some(Tone::Marked);
                }
                self.edits.update_of(&key_value).map(|_| Tone::Edited)
            })
            .collect()
    }

    pub(super) fn pending_offers(&self) -> Vec<(String, &'static str)> {
        if !self.edits.pending() {
            return Vec::new();
        }
        vec![
            (":w".into(), "Write"),
            (keymap::key(Action::ClearMarks, 0), "Undo"),
        ]
    }

    pub(super) fn edit_offers(&self) -> Vec<(String, &'static str)> {
        vec![
            (keymap::key(Action::EditValue, 0), "Edit"),
            (keymap::key(Action::NewRow, 0), "Add"),
            (keymap::key(Action::MarkDelete, 0), "Mark"),
        ]
    }

    pub(super) fn editing_offers(&self) -> Option<Vec<(String, &'static str)>> {
        let offers: &[(&str, &'static str)] = match (&self.edits.form, &self.edits.value) {
            (Some(form), _) if form.editor.is_some() => {
                &[("↵", "Keep"), ("tab", "Next"), ("esc", "Put back")]
            }
            (Some(_), _) => &[
                ("↑↓", "Field"),
                ("↵", "Edit"),
                ("ctrl+s", "Keep row"),
                ("esc", "Cancel"),
            ],
            (None, Some(value)) if value.editable() => {
                &[("↵", "Keep"), ("⇧↵", "New line"), ("esc", "Cancel")]
            }
            (None, Some(_)) => &[
                ("V", "Select"),
                ("y", "Yank"),
                ("e", "Edit"),
                ("esc", "Close"),
            ],
            (None, None) => return None,
        };
        Some(
            offers
                .iter()
                .map(|(key, label)| (key.to_string(), *label))
                .collect(),
        )
    }

    pub(super) fn editing_status(&self, width: usize) -> Option<Line<'static>> {
        let (heading, hint) = match (&self.edits.form, &self.edits.value) {
            (Some(form), _) => (
                if form.adds() {
                    "new row"
                } else {
                    "editing row"
                }
                .to_string(),
                format!(
                    "   ctrl+s keeps it pending    :w writes it{}",
                    if form.expanded {
                        "    ⇧↵ adds a line"
                    } else {
                        ""
                    }
                ),
            ),
            (None, Some(value)) => self.value_status(value),
            (None, None) => return None,
        };
        let plain = format!(" {heading}{hint}");
        if text::width(&plain) > width {
            return Some(Line::raw(truncate(&plain, width)));
        }
        Some(Line::from(vec![
            Span::raw(" "),
            Span::styled(heading, theme::bold()),
            Span::styled(hint, theme::dim()),
        ]))
    }

    fn value_status(&self, value: &ValueEditor) -> (String, String) {
        let column = self
            .headers
            .get(self.column_index)
            .cloned()
            .unwrap_or_else(|| "?".to_string());
        if !value.editable() && value.anchor.is_some() {
            let (from, to) = value.selected_lines();
            return (
                "visual".to_string(),
                format!("   lines {}-{}    y yanks    esc clears", from + 1, to + 1),
            );
        }
        if !value.editable() {
            return (
                format!("viewing {column}"),
                "   j/k moves    V selects    y yanks    g/G top/bottom    esc closes".to_string(),
            );
        }
        let time = if now::suits(value.column_type.as_deref()) {
            format!("    ctrl+t now {}", now::label())
        } else {
            String::new()
        };
        let json = if value.json {
            "    json is validated"
        } else {
            ""
        };
        (
            format!("editing {column}"),
            format!("   ↵ keeps it{time}    ⇧↵ adds a line    esc cancels{json}"),
        )
    }

    pub(super) fn pending_callout(&self) -> String {
        if self.edits.pending() {
            self.edits.status()
        } else {
            String::new()
        }
    }

    pub(super) fn pending_style(&self) -> Option<Style> {
        let (key, fallback) = match (self.edits.deletes.is_empty(), self.edits.updates.is_empty()) {
            (false, _) => ("deleted", "red"),
            (true, false) => ("edited", "yellow"),
            (true, true) => return None,
        };
        Some(theme::paint(&settings::with(|settings| {
            settings.color(key, fallback)
        })))
    }

    pub(super) fn callout_style(&self) -> Style {
        let (key, fallback) = if self.edits.deletes.is_empty() {
            ("edited", "yellow")
        } else {
            ("deleted", "red")
        };
        theme::bold().patch(theme::paint(&settings::with(|settings| {
            settings.color(key, fallback)
        })))
    }

    pub(super) fn draw_edits(&mut self, frame: &mut Frame, frame_height: u16) {
        let frame_width = frame.area().width;
        let room = frame_width.saturating_sub(4) as usize;
        self.edits.viewer = None;

        if let Some(value) = &self.edits.value {
            let title = if value.editable() {
                value.column.clone()
            } else {
                format!("{}  ·  read-only", value.column)
            };
            let view = ValueView {
                title,
                buffer: &value.buffer,
                json: value.json,
                show_cursor: value.editable(),
                selection: value.anchor.map(|_| value.selected_lines()),
            };
            let area = draw_view(frame, &view, room, frame_height);
            self.edits.viewer = Some((area, view.first_line(area.height as usize - 2)));
        }

        if let Some(form) = self.edits.form.as_mut() {
            let width = room.min(form.natural_width()) as u16;
            let height = frame_height.saturating_sub(2).min(form.rows() as u16);
            let island = Island {
                title: form.title(),
                content: form.content(width as usize - 2, height as usize - 2),
                ..Island::default()
            };
            Modal::new(frame_width, 1, frame_height, width).draw(
                frame.buffer_mut(),
                island,
                height,
            );

            if form.expanded
                && let Some(editor) = &form.editor
            {
                let view = ValueView {
                    title: form.current().name.clone(),
                    buffer: editor,
                    json: form.json,
                    show_cursor: true,
                    selection: None,
                };
                draw_view(frame, &view, room, frame_height);
            }
        }
    }

    pub(super) fn mouse_in_edits(&mut self, mouse: Mouse, row: u16) {
        let viewer = self.edits.viewer;
        let Some(value) = self.edits.value.as_mut().filter(|value| !value.editable()) else {
            return;
        };
        let line = viewer.and_then(|(area, first)| {
            let local = area.local_row(row);
            (local >= 0 && local < area.height as isize - 2)
                .then_some(first + local as usize)
                .filter(|line| *line < value.buffer.lines().len())
        });

        match (mouse.button, line) {
            (Button::WheelUp, _) => value.buffer.handle(Key::Up),
            (Button::WheelDown, _) => value.buffer.handle(Key::Down),
            (Button::Drag, Some(line)) => {
                value.anchor = value.anchor.or(Some(value.buffer.cursor_line()));
                value.buffer.move_to_line_column(line, 0);
            }
            (Button::Left, Some(line)) => {
                value.anchor = None;
                value.buffer.move_to_line_column(line, 0);
            }
            _ => {}
        }
    }
}

fn draw_view(frame: &mut Frame, view: &ValueView, room: usize, frame_height: u16) -> Area {
    let frame_width = frame.area().width;
    let width = room.min(view.natural_width()) as u16;
    let height = frame_height.saturating_sub(2).min(view.rows() as u16);
    let island = view.island(width as usize - 2, height as usize - 2);
    Modal::new(frame_width, 1, frame_height, width).draw(frame.buffer_mut(), island, height);
    Area {
        x: (frame_width.saturating_sub(width) / 2 + 1).max(1),
        y: 1 + frame_height.saturating_sub(height) / 2,
        width,
        height,
    }
}
