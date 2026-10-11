use super::*;
use crate::driver::Driver;
use crate::tui::history::{self, Execution};

const SYSTEM_DATABASES: [&str; 8] = [
    "information_schema",
    "mysql",
    "performance_schema",
    "sys",
    "master",
    "model",
    "msdb",
    "tempdb",
];

const REFUSED: &str = "THE DATABASE SAID NO";

fn database_name(typed: &str) -> String {
    typed
        .trim_matches([' ', '\t', '`', '"', '\'', '[', ']'])
        .to_string()
}

impl Browser {
    pub(super) fn open_databases(&mut self) {
        if self.connection.driver == Driver::Sqlite {
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
            Key::Enter if choice.creating() => {
                let name = choice.new_option().unwrap_or_default();
                if self.asks_before_creating() {
                    self.databases = None;
                    self.command = Some(format!("create database {}", database_name(&name)));
                } else {
                    self.create_database(&name);
                }
            }
            Key::Ctrl('d') if !choice.creating() => {
                let chosen = choice.selected();
                self.ask_to_drop(chosen);
            }
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
        self.forget_database();
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

    fn forget_database(&mut self) {
        self.edits.forget();
        self.query.forget_schema();
        self.filter.clear();
        self.sort = None;
        self.filters = None;
        self.offset = 0;
        self.table_index = 0;
        self.tables.clear();
        self.headers.clear();
        self.rows.clear();
        self.raw.clear();
        self.kinds.clear();
        self.forget_links();
    }

    fn cannot_change_databases(&self) -> Option<&'static str> {
        if self.connection.driver == Driver::Sqlite {
            return Some("a sqlite connection is one file");
        }
        if self.connection.read_only {
            return Some("this connection is marked read-only");
        }
        None
    }

    fn refuse_database_change(&mut self) -> bool {
        match self.cannot_change_databases() {
            Some(reason) => {
                self.say(reason);
                true
            }
            None => {
                self.hush();
                false
            }
        }
    }

    fn asks_before_creating(&self) -> bool {
        self.connection.is_production()
            && settings::with(|settings| settings.confirm_create_on_production)
    }

    pub(super) fn create_database(&mut self, typed: &str) {
        self.databases = None;
        if self.refuse_database_change() {
            return;
        }
        let name = database_name(typed);
        if name.is_empty() {
            self.command = Some("create database ".to_string());
            return;
        }

        let statement = format!("create database {}", self.database.quote(&name));
        if let Err(error) = self.run_on_server(&statement) {
            self.fail(format!("{error:#}"), Vec::new(), REFUSED);
            return;
        }

        self.use_database(Some(name.clone()));
        self.say(format!("created {name} · using it"));
    }

    fn ask_to_drop(&mut self, name: Option<String>) {
        self.databases = None;
        if self.refuse_database_change() {
            return;
        }
        if let Some(name) = name {
            self.command = Some(format!("drop database {name}"));
        }
    }

    pub(super) fn drop_database(&mut self, typed: &str) {
        if self.refuse_database_change() {
            return;
        }
        let name = database_name(typed);
        if name.is_empty() {
            self.command = Some("drop database ".to_string());
            return;
        }

        let in_use = self.active_database.as_deref() == Some(name.as_str());
        if in_use {
            let Some(step_aside) = self.somewhere_other_than(&name) else {
                self.say(format!(
                    "tql is using {name} and there is no other database to drop it from"
                ));
                return;
            };
            if let Err(error) = self.database.use_database(&step_aside) {
                self.fail(format!("{error:#}"), Vec::new(), REFUSED);
                return;
            }
        }

        let statement = format!("drop database {}", self.database.quote(&name));
        if let Err(error) = self.run_on_server(&statement) {
            if in_use {
                let _ = self.database.use_database(&name);
            }
            self.fail(format!("{error:#}"), Vec::new(), REFUSED);
            return;
        }

        if in_use {
            self.pick_after_dropping();
        }
        self.say(format!("dropped {name}"));
    }

    fn run_on_server(&mut self, statement: &str) -> anyhow::Result<()> {
        let started = Instant::now();
        let ran = self.database.run_server_wide(statement);
        if let Some(store) = &self.store {
            let error = ran.as_ref().err().map(|error| format!("{error:#}"));
            history::record(
                store,
                "tui",
                Execution {
                    connection_id: self.connection.id,
                    statement,
                    error: error.as_deref(),
                    rows: None,
                    duration_ms: started.elapsed().as_millis(),
                },
            );
        }
        ran.map(|_| ())
    }

    fn pick_after_dropping(&mut self) {
        let databases = self.database.databases().unwrap_or_default();
        self.active_database = None;
        self.forget_database();
        self.keys.clear();
        self.databases = (!databases.is_empty())
            .then(|| Choice::new("DATABASE", databases, "", !self.connection.read_only));
    }

    fn somewhere_other_than(&mut self, name: &str) -> Option<String> {
        let saved = self.connection.database.clone().unwrap_or_default();
        if !saved.is_empty() && saved != name {
            return Some(saved);
        }

        let others: Vec<String> = self
            .database
            .databases()
            .unwrap_or_default()
            .into_iter()
            .filter(|database| database != name)
            .collect();
        others
            .iter()
            .find(|database| !SYSTEM_DATABASES.contains(&database.as_str()))
            .or_else(|| others.first())
            .cloned()
    }
}
