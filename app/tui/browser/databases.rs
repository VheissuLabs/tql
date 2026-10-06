use super::*;

impl Browser {
    pub(super) fn open_databases(&mut self) {
        if self.connection.driver == "sqlite" {
            self.say("a sqlite connection is one file");
            return;
        }
        let databases = self.database.databases().unwrap_or_default();
        if databases.is_empty() {
            self.say("no databases to switch to");
            return;
        }
        let chosen = self.active_database.clone().unwrap_or_default();
        self.databases = Some(Choice::new(
            "DATABASE",
            databases,
            &chosen,
            !self.connection.read_only,
        ));
    }

    pub(super) fn type_into_databases(&mut self, key: Key) {
        let Some(choice) = self.databases.as_mut() else {
            return;
        };
        match key {
            Key::Escape => self.databases = None,
            Key::Up | Key::Ctrl('p') => choice.move_by(false),
            Key::Down | Key::Ctrl('n') => choice.move_by(true),
            Key::Enter if choice.creating() => self.say("creating databases is not ported yet"),
            Key::Ctrl('d') => self.say("dropping databases is not ported yet"),
            Key::Enter => {
                let chosen = choice.selected();
                self.use_database(chosen);
            }
            other => choice.type_key(other),
        }
    }

    pub(super) fn use_database(&mut self, chosen: Option<String>) {
        self.databases = None;
        let Some(chosen) = chosen else {
            return;
        };
        if self.active_database.as_deref() == Some(chosen.as_str()) {
            return;
        }
        if let Err(error) = self.database.use_database(&chosen) {
            self.say(error.to_string());
            return;
        }

        self.active_database = Some(chosen.clone());
        self.filter.clear();
        self.sort = None;
        self.filters = None;
        self.offset = 0;
        self.table_index = 0;
        self.headers.clear();
        self.rows.clear();
        self.keys = self.database.primary_keys().unwrap_or_default();
        self.tables = match self.database.tables() {
            Ok(tables) => tables,
            Err(error) => {
                self.say(error.to_string());
                return;
            }
        };

        if self.tables.is_empty() {
            self.say(format!("{chosen} has no tables"));
            return;
        }
        self.load(false);
        self.say(format!("using {chosen}"));
    }
}
