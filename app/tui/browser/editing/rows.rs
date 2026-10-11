use super::*;

impl Browser {
    pub(in crate::tui::browser) fn mark_delete(&mut self) {
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

    pub(in crate::tui::browser) fn clear_marks(&mut self) {
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

    pub(in crate::tui::browser) fn leave(&mut self, exit: Exit) -> Flow<Exit> {
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

    pub(super) fn filled_by_database(&self, name: &str, columns: &[Column]) -> bool {
        let Some(column) = columns.iter().find(|column| column.name == name) else {
            return false;
        };
        column.auto_increment
            || self.connection.driver == "sqlite"
                && self.key_column().as_deref() == Some(name)
                && column.type_name.eq_ignore_ascii_case("integer")
    }

    pub(super) fn suggested_key(&mut self, table: &str, columns: &[Column]) -> Values {
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
        let statement = format!(
            "select max({}) as highest from {}",
            self.database.quote(&key),
            self.database.quote(table)
        );
        let started = Instant::now();
        let highest = self.database.highest(table, &key);
        let counted = highest
            .as_ref()
            .map(|_| ())
            .map_err(|error| anyhow::anyhow!("{error:#}"));
        self.remember_write(&statement, started, &counted, 1);
        let Ok(highest) = highest else {
            return Values::new();
        };
        let highest = highest.map(|highest| whole(&highest)).unwrap_or(0);
        vec![(key, Some(highest.saturating_add(1).to_string()))]
    }

    pub(in crate::tui::browser) fn new_row(&mut self) {
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

    pub(super) fn added_row(&self, values: &Values) -> (Vec<Option<String>>, Vec<String>) {
        self.headers
            .iter()
            .map(|header| match values.named(header) {
                None => (None, String::new()),
                Some(None) => (None, text::cell(None)),
                Some(Some(value)) => (Some(value.clone()), text::cell(Some(value))),
            })
            .unzip()
    }

    pub(super) fn show_added_row(&mut self, at: usize) {
        let (raw, shown) = self.added_row(&self.edits.inserts[at]);
        self.raw[at] = raw;
        self.rows[at] = shown;
    }

    pub(in crate::tui::browser) fn prepend_pending_rows(&mut self) {
        let (raw, shown): (Vec<_>, Vec<_>) = self
            .edits
            .inserts
            .iter()
            .map(|values| self.added_row(values))
            .unzip();
        let added: Vec<Vec<Kind>> = raw.iter().map(|row| vec![Kind::Text; row.len()]).collect();
        if !self.kinds.is_empty() {
            self.kinds.splice(0..0, added);
        }
        self.raw.splice(0..0, raw);
        self.rows.splice(0..0, shown);
    }

    pub(in crate::tui::browser) fn shown_rows(&self) -> Cow<'_, [Vec<String>]> {
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

    pub(in crate::tui::browser) fn tones(&self) -> Vec<Option<Tone>> {
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
}
