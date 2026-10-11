use super::*;

impl Browser {
    pub(in crate::tui::browser) fn open_record(&mut self) {
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
                self.connection.driver,
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

    pub(super) fn type_into_form(&mut self, key: Key) {
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

    pub(super) fn type_into_field(&mut self, key: Key) {
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

    pub(super) fn close_record(&mut self) {
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

    pub(super) fn save_record(&mut self) {
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
}
