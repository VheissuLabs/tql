use super::*;

impl ConnectionForm {
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
        let mut found = match key {
            "ssh_key" => key_files::ssh_keys(&self.places.home),
            "private_key" => key_files::snowflake_keys(&self.places.home, &self.places.cwd),
            _ => key_files::certificates(&self.places.home, &self.places.cwd),
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
}
