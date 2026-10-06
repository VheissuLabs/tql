use std::path::PathBuf;
use std::sync::LazyLock;

use regex::Regex;

use super::*;
use crate::database::{self, order_by};
use crate::tui::completion::{Completion, Lookup, Schema};
use crate::tui::editor::QueryEditor;
use crate::tui::history::{self, Execution};
use crate::tui::statements;
use crate::tui::vim::Vim;

static FROM_TABLE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?i)\bfrom\s+[`"\[]?([A-Za-z0-9_]+)"#).expect("the from pattern is valid")
});

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EditKind {
    Newline,
    Delete,
    Type,
    Other,
}

pub struct Query {
    pub editor: QueryEditor,
    pub results: bool,
    pub table: Option<String>,
    pub completion: Option<Completion>,
    pub vim: Vim,
    last_edit: Option<EditKind>,
    columns: HashMap<String, Vec<String>>,
}

impl Default for Query {
    fn default() -> Self {
        Query {
            editor: QueryEditor::new(true),
            results: false,
            table: None,
            completion: None,
            vim: Vim::default(),
            last_edit: None,
            columns: HashMap::new(),
        }
    }
}

impl Query {
    pub(super) fn forget_schema(&mut self) {
        self.columns.clear();
    }
}

struct Cached<'a> {
    database: &'a mut dyn Database,
    columns: &'a mut HashMap<String, Vec<String>>,
}

impl Lookup for Cached<'_> {
    fn columns(&mut self, table: &str) -> Vec<String> {
        self.columns
            .entry(table.to_string())
            .or_insert_with(|| self.database.column_names(table).unwrap_or_default())
            .clone()
    }

    fn quote(&self, name: &str) -> String {
        self.database.quote(name)
    }
}

impl Browser {
    pub fn with_store(mut self, store: PathBuf) -> Self {
        self.store = Some(store);
        self
    }

    pub(super) fn fail(&mut self, message: String, notes: Vec<String>, title: &str) {
        self.problem = Some(message);
        self.problem_notes = notes;
        self.problem_offset = 0;
        self.problem_title = title.to_string();
    }

    pub(super) fn type_into_problem(&mut self, key: Key) {
        match key {
            Key::Up | Key::Char('k') => self.problem_offset = self.problem_offset.saturating_sub(1),
            Key::Down | Key::Char('j') => self.problem_offset += 1,
            Key::Char('y') => {
                let problem = self.problem.clone().unwrap_or_default();
                self.clipboard.copy(&problem);
                self.say("copied the error");
            }
            _ => {
                self.problem = None;
                self.problem_notes.clear();
                self.problem_offset = 0;
            }
        }
    }

    pub(super) fn open_query(&mut self) {
        self.enter_query_mode();
        if self.query.editor.is_empty()
            && let Some(statement) = self.last_statement.clone()
        {
            self.query.editor.set(&statement);
        }
        self.say(query_hint());
    }

    pub(super) fn use_statement(&mut self, statement: &str) {
        self.enter_query_mode();
        self.query.editor.set(statement);
        self.say(query_hint());
    }

    pub(super) fn type_into_query(&mut self, key: Key) -> Flow<Exit> {
        if self.command.is_some() {
            return self.type_into_command(key);
        }
        let leaves_the_editor = matches!(
            keymap::action(key).map(|binding| binding.action),
            Some(
                Action::Palette
                    | Action::History
                    | Action::FocusTables
                    | Action::FocusRows
                    | Action::FocusSql
            )
        );
        if leaves_the_editor && !matches!(key, Key::Char(_)) {
            let action = keymap::action(key).expect("the key has an action").action;
            return self.run(action);
        }

        let offered = self.query.completion.take();
        if settings::with(|settings| settings.sql_vim) {
            return self.type_into_vim(key, offered);
        }
        if let Some(offered) = offered
            && self.take_completion_key(key, offered)
        {
            return Flow::Continue;
        }

        match key {
            Key::Ctrl('r') => self.run_statement_at_cursor(),
            Key::Escape => self.leave_query(),
            Key::Ctrl('z') => self.undo_in_editor(),
            Key::Ctrl('y') => self.redo_in_editor(),
            Key::ShiftEnter | Key::Alt('\r' | '\n') => self.edit_with_undo(Key::Enter),
            other => self.edit_with_undo(other),
        }
        self.offer_completion_after(key);
        Flow::Continue
    }

    pub(super) fn take_completion_key(&mut self, key: Key, mut offered: Completion) -> bool {
        match key {
            Key::Tab => self.complete(&offered),
            Key::Up | Key::Ctrl('p') | Key::Down | Key::Ctrl('n') => {
                offered.move_by(matches!(key, Key::Down | Key::Ctrl('n')));
                self.query.completion = Some(offered);
            }
            Key::Escape => {}
            _ => return false,
        }
        true
    }

    fn complete(&mut self, offered: &Completion) {
        let Some(chosen) = offered.selected() else {
            return;
        };
        if offered.end != self.query.editor.cursor() {
            return;
        }
        if settings::with(|settings| settings.sql_vim) {
            self.complete_in_vim(offered.end - offered.start, &chosen.text);
            return;
        }
        let editor = &mut self.query.editor;
        editor.commit();
        editor.checkpoint();
        editor.replace(offered.start, offered.end, &chosen.text);
        editor.commit();
        self.query.last_edit = None;
    }

    pub(super) fn offer_completion_after(&mut self, key: Key) {
        let typed = matches!(key, Key::Char(_) | Key::Backspace | Key::Ctrl('h'));
        if !typed || !settings::with(|settings| settings.sql_complete) {
            return;
        }

        let open = self.current_table();
        let buffer = self.query.editor.buffer();
        let cursor = self.query.editor.cursor();
        let offered = Completion::at(
            &buffer,
            cursor,
            Schema {
                tables: &self.tables,
                open: open.as_deref(),
                lookup: &mut Cached {
                    database: self.database.as_mut(),
                    columns: &mut self.query.columns,
                },
            },
        );
        self.query.completion = offered;
    }

    fn edit_with_undo(&mut self, key: Key) {
        let kind = match key {
            Key::Enter => EditKind::Newline,
            Key::Backspace | Key::Ctrl('h') | Key::Delete => EditKind::Delete,
            Key::Char(_) => EditKind::Type,
            _ => EditKind::Other,
        };
        let breaks_a_word = matches!(key, Key::Enter | Key::Char(' '));
        if Some(kind) != self.query.last_edit || breaks_a_word {
            self.query.editor.commit();
        }
        self.query.last_edit = Some(kind);
        self.query.editor.checkpoint();
        self.query.editor.handle(key);
    }

    fn undo_in_editor(&mut self) {
        self.query.last_edit = None;
        if !self.query.editor.undo() {
            self.say("already at oldest change");
        }
    }

    fn redo_in_editor(&mut self) {
        self.query.last_edit = None;
        if !self.query.editor.redo() {
            self.say("already at newest change");
        }
    }

    pub(super) fn leave_query(&mut self) {
        self.query.completion = None;
        self.query.editor.commit();
        self.query.last_edit = None;
        self.mode = Mode::Browse;
        self.hush();
    }

    pub(super) fn run_or_reload(&mut self) {
        if self.mode == Mode::Query {
            self.run_from_editor();
        } else {
            self.reload();
        }
    }

    pub(super) fn run_statement_at_cursor(&mut self) {
        let buffer = self.query.editor.buffer();
        let Some(statement) = statements::at(&buffer, self.query.editor.cursor()) else {
            self.run_query(&buffer);
            return;
        };
        self.run_query(&statement.sql);

        if statement.of > 1 && self.query.results && self.problem.is_none() {
            let said = format!(
                "{} · statement {} of {}",
                self.status(),
                statement.number,
                statement.of
            );
            self.say(said);
        }
    }

    pub(super) fn run_query(&mut self, statement: &str) {
        if statement.trim().is_empty() {
            self.say("nothing to run");
            return;
        }
        let writes = !database::is_read_only(statement);
        if writes && self.connection.read_only {
            self.fail(
                "this connection is marked read-only, so the SQL editor only runs select, show, explain, describe, pragma or with, one statement at a time".to_string(),
                Vec::new(),
                "READ-ONLY",
            );
            return;
        }
        if writes {
            self.query.forget_schema();
            self.keys = self.database.primary_keys().unwrap_or_default();
        }

        let started = Instant::now();
        let ran = self.database.run(statement);
        let elapsed = started.elapsed().as_millis();
        let error = ran.as_ref().err().map(|error| error.to_string());
        let counted = ran
            .as_ref()
            .ok()
            .map(|outcome| outcome.affected.unwrap_or(outcome.rows.len()));
        if let Some(store) = &self.store {
            history::record(
                store,
                Execution {
                    connection_id: self.connection.id,
                    statement,
                    error: error.as_deref(),
                    rows: counted,
                    duration_ms: elapsed,
                },
            );
        }

        let outcome = match ran {
            Ok(outcome) => outcome,
            Err(error) => {
                self.fail(
                    format!("{error}\n\n{statement}"),
                    Vec::new(),
                    "THE DATABASE SAID NO",
                );
                return;
            }
        };

        self.headers = outcome.headers;
        self.rows = outcome
            .rows
            .iter()
            .map(|row| {
                row.iter()
                    .map(|value| text::cell(value.as_deref()))
                    .collect()
            })
            .collect();
        self.raw = outcome.rows;
        self.row_index = 0;
        self.column_index = 0;
        self.column_offset = 0;
        self.has_more = false;
        self.query.results = true;
        self.last_statement = Some(statement.to_string());
        self.sort = order_by::of(statement);
        self.follow_query_table(statement);

        let said = match outcome.affected {
            Some(1) => format!("1 row affected · {elapsed}ms"),
            Some(affected) => format!("{affected} rows affected · {elapsed}ms"),
            None => format!("{} rows · {elapsed}ms", self.rows.len()),
        };
        self.say(said);
    }

    fn follow_query_table(&mut self, statement: &str) {
        self.query.table = None;
        let Some(captures) = FROM_TABLE.captures(statement) else {
            return;
        };
        let named = &captures[1];
        let Some(index) = self
            .visible_tables()
            .iter()
            .position(|table| *table == named)
        else {
            return;
        };
        if index != self.table_index {
            self.edits.forget();
        }
        self.table_index = index;
        self.query.table = Some(named.to_string());
    }

    pub(super) fn back_to_table(&mut self) {
        if !self.query.results {
            return;
        }
        self.load(false);
        let said = format!("back to {}", self.current_table().unwrap_or_default());
        self.say(said);
    }

    pub(super) fn sort_query_results(&mut self, column: String) {
        let direction = match &self.sort {
            Some(sort) if sort.column != column => Some(Direction::Asc),
            None => Some(Direction::Asc),
            Some(sort) if sort.direction == Direction::Asc => Some(Direction::Desc),
            Some(_) => None,
        };
        let sort = direction.map(|direction| Sort { column, direction });
        let rewritten = order_by::apply(&self.query.editor.buffer(), sort.as_ref(), |name| {
            self.database.quote(name)
        });
        let Some(rewritten) = rewritten else {
            self.say("this query is too complex to sort — add an order by yourself");
            return;
        };

        let column = self.headers.get(self.column_index).cloned();
        let row = self.row_index;
        self.query.editor.set(&rewritten);
        self.run_query(&rewritten);
        if !self.query.results {
            return;
        }
        self.column_index = column
            .and_then(|column| self.headers.iter().position(|header| *header == column))
            .unwrap_or(0);
        self.row_index = row.min(self.rows.len().saturating_sub(1));
    }

    pub(super) fn click_editor(&mut self, local_row: isize, local_column: isize) {
        let entering = self.mode != Mode::Query;
        self.enter_query_mode();
        self.say(query_hint());
        if entering && local_row < 0 {
            return;
        }
        let Some(pane) = &self.hits.editor_pane else {
            return;
        };
        let (line, column) = pane.position_at(local_row, local_column);
        self.query.editor.move_to_line_column(line, column);
    }
}
