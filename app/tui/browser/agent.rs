use serde_json::{Map, Value, json};

use super::*;
use crate::agent;

pub const FROM_AGENT: &str = "SQL from your agent · nothing has run";

pub const READ_BY_AGENT: &str = "your agent looked at your screen";

impl Browser {
    pub(super) fn used(&mut self) {
        if let Some(live) = self.live.as_mut() {
            live.used();
        }
    }

    pub(super) fn take_from_agent(&mut self) -> bool {
        let Some(sql) = self.live.as_ref().and_then(Live::take) else {
            return false;
        };
        self.write_from_agent(&sql);
        true
    }

    pub fn write_from_agent(&mut self, sql: &str) {
        self.replace_query(sql);
        if !self.busy_elsewhere() {
            self.enter_query_mode();
        }
        if settings::with(|settings| settings.sql_vim) {
            self.query.vim.rest(&mut self.query.editor);
        }
        self.say(FROM_AGENT);
    }

    pub(super) fn answer_agent(&mut self) -> bool {
        if !self.live.as_ref().is_some_and(Live::asked) {
            return false;
        }
        let answer = match self.refusal() {
            Some((refusal, setting)) => {
                self.say(format!(
                    "your agent asked to see your screen · {setting} is off in [mcp]"
                ));
                json!({ "refused": refusal })
            }
            None => {
                self.say(READ_BY_AGENT);
                self.screen()
            }
        };
        if let Some(live) = self.live.as_ref() {
            live.answer(&answer);
        }
        true
    }

    fn refusal(&self) -> Option<(String, &'static str)> {
        let (shared, production) =
            settings::with(|settings| (settings.mcp_read_screen, settings.mcp_read_production));
        let name = &self.connection.name;
        if !shared {
            return Some((
                format!(
                    "tql on [{name}] does not share its screen. Ask the user to set read_screen = true under [mcp] in config.toml and restart tql."
                ),
                "read_screen",
            ));
        }
        if self.connection.is_production() && !production {
            return Some((
                format!(
                    "[{name}] is tagged production, and tql does not share a production screen. Ask the user to set read_production = true under [mcp] in config.toml and restart tql if they want it."
                ),
                "read_production",
            ));
        }
        None
    }

    pub fn screen(&self) -> Value {
        let row = |at: usize| -> Value {
            let mut object = Map::new();
            for (column, header) in self.headers.iter().enumerate() {
                let kind = self
                    .kinds
                    .get(at)
                    .and_then(|kinds| kinds.get(column))
                    .copied()
                    .unwrap_or_default();
                let value = self
                    .raw
                    .get(at)
                    .and_then(|values| values.get(column))
                    .cloned()
                    .flatten();
                object.insert(header.clone(), agent::typed(value.as_deref(), kind));
            }
            Value::Object(object)
        };
        let focus = match (self.mode, self.focus) {
            (Mode::Query, _) => "sql editor",
            (_, Focus::Sidebar) => "tables",
            (_, Focus::Grid) => "rows",
        };
        let table = match self.query.results {
            true => self.query.table.clone(),
            false => self.current_table(),
        };
        json!({
            "connection": self.connection.name,
            "driver": self.connection.driver,
            "tag": self.connection.tag,
            "database": self.active_database.clone().or_else(|| self.connection.database.clone()),
            "showing": if self.query.results { "query results" } else { "table" },
            "table": table,
            "focus": focus,
            "statement": self.last_statement,
            "sql_editor": self.query.editor.buffer(),
            "sort": self.sort.as_ref().map(|sort| json!({
                "column": sort.column,
                "direction": match sort.direction {
                    Direction::Asc => "asc",
                    Direction::Desc => "desc",
                },
            })),
            "filtered": self.filters.is_some(),
            "cursor": {
                "row": (!self.raw.is_empty()).then_some(self.offset + self.row_index + 1),
                "column": self.headers.get(self.column_index),
                "record": (self.row_index < self.raw.len()).then(|| row(self.row_index)),
            },
            "rows": {
                "first": self.offset + 1,
                "last": self.offset + self.raw.len(),
                "more": self.has_more,
                "columns": self.headers,
                "records": (0..self.raw.len()).map(row).collect::<Vec<_>>(),
            },
            "unwritten": {
                "edits": self.edits.updates.len(),
                "new_rows": self.edits.inserts.len(),
                "deletes": self.edits.deletes.len(),
            },
            "status": self.status,
        })
    }

    fn busy_elsewhere(&self) -> bool {
        self.problem.is_some()
            || self.edits.form().is_some()
            || self.edits.captures_keys()
            || self.palette.is_some()
            || self.databases.is_some()
            || self.link_picker.is_some()
            || self.filter_form.is_some()
            || self.question.is_some()
            || self.command.is_some()
            || self.structure.is_some()
            || self.inspector.is_some()
            || self.filtering
            || self.mode == Mode::Help
    }
}
