use super::*;

impl Browser {
    pub(in crate::tui::browser) fn start_editing(&mut self, read_only: bool) {
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

    pub(in crate::tui::browser) fn type_into_edits(&mut self, key: Key) {
        if self.edits.form.is_some() {
            self.type_into_form(key);
        } else if self.edits.value.is_some() {
            self.type_into_value(key);
        }
    }

    pub(super) fn type_into_value(&mut self, key: Key) {
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

    pub(super) fn scroll_value(&mut self, key: Key) {
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

    pub(super) fn commit_edit(&mut self) {
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

        if !self.identifies_rows() {
            self.say("nothing to save");
            return;
        }
        let key_value = self.key_value(self.row_index).unwrap_or_default();
        self.edits.update_for(&key_value).put(&column, kept);
        self.say(self.edits.status());
    }

    pub(in crate::tui::browser) fn paste_into_edits(&mut self, text: &str) {
        if let Some(form) = self.edits.form.as_mut() {
            form.paste(text);
            return;
        }
        if let Some(value) = self.edits.value.as_mut()
            && value.editable()
        {
            value.buffer.paste(text);
        }
    }
}
