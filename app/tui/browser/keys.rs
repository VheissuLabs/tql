use super::*;

impl Browser {
    pub(super) fn toggle_tables(&mut self) {
        self.tables_hidden = !self.tables_hidden;
        if self.tables_hidden && self.focus == Focus::Sidebar {
            self.focus = Focus::Grid;
        }
        if self.tables_hidden {
            let key = keymap::key(Action::ToggleTables, 0);
            self.say(format!("table list hidden · {key} shows it"));
        } else {
            self.hush();
        }
    }

    pub(super) fn toggle_focus(&mut self, forward: bool) {
        let sql_shown = self.mode == Mode::Query || settings::with(|settings| settings.sql_always);
        let panes: Vec<Option<Focus>> = [
            (!self.tables_hidden).then_some(Some(Focus::Sidebar)),
            Some(Some(Focus::Grid)),
            sql_shown.then_some(None),
        ]
        .into_iter()
        .flatten()
        .collect();
        let current = if self.mode == Mode::Query {
            None
        } else {
            Some(self.focus)
        };
        let at = panes.iter().position(|pane| *pane == current).unwrap_or(0);
        let step = if forward { 1 } else { panes.len() - 1 };
        let Some(next) = panes[(at + step) % panes.len()] else {
            self.open_query();
            return;
        };
        self.focus = next;
        self.mode = Mode::Browse;
    }

    pub(super) fn resize(&mut self, by: Option<isize>) {
        if self.focus == Focus::Sidebar {
            self.sidebar_width =
                by.map(|by| (self.tables_width() as isize + by).clamp(8, 80) as usize);
            self.say(format!("table list width {}", self.tables_width()));
            return;
        }

        let Some(column) = self.headers.get(self.column_index).cloned() else {
            return;
        };
        let Some(by) = by else {
            self.width_overrides.remove(&column);
            self.say(format!("{column} width reset"));
            return;
        };

        let current = self
            .width_overrides
            .get(&column)
            .copied()
            .unwrap_or_else(|| grid::natural_width(&self.headers, &self.rows, self.column_index));
        let width = (current as isize + by).clamp(3, 120) as usize;
        self.width_overrides.insert(column.clone(), width);
        self.say(format!("{column} width {width}"));
    }

    pub(super) fn run_command(&mut self, command: &str) -> Flow<Exit> {
        self.command = None;
        match command {
            "q" | "q!" | "quit" => return Flow::Exit(Exit::Quit),
            "c" | "connections" => return Flow::Exit(Exit::Connections),
            "tables" => self.focus = Focus::Sidebar,
            "rows" => self.focus = Focus::Grid,
            "r" => self.run_or_reload(),
            "reload" => self.reload(),
            "run" => self.run_statement_at_cursor(),
            "sql" => self.open_query(),
            "history" => self.open_history(),
            "w" | "write" => self.say("nothing to write"),
            "export" | "export sql" | "create database" | "drop database" => {
                self.say(format!(":{command} is not ported yet"));
            }
            _ => self.say(format!("unknown command: :{command}")),
        }
        Flow::Continue
    }

    pub(super) fn type_into_command(&mut self, key: Key) -> Flow<Exit> {
        let Some(command) = self.command.as_mut() else {
            return Flow::Continue;
        };
        match key {
            Key::Enter => {
                let command = command.trim().to_string();
                return self.run_command(&command);
            }
            Key::Escape => self.command = None,
            Key::Backspace if command.is_empty() => self.command = None,
            Key::Backspace => {
                command.pop();
            }
            Key::Char(character) => command.push(character),
            _ => {}
        }
        Flow::Continue
    }

    pub(super) fn type_into_filter(&mut self, key: Key) {
        match key {
            Key::Escape => {
                self.filtering = false;
                self.filter.clear();
                self.hush();
                self.reselect();
            }
            Key::Enter => {
                self.filtering = false;
                if self.filter.is_empty() {
                    self.hush();
                } else {
                    let said = format!("filtered by \"{}\"", self.filter);
                    self.say(said);
                }
            }
            Key::Backspace | Key::Ctrl('h') => {
                self.filter.pop();
                self.reselect();
            }
            Key::Char(character) => {
                self.filter.push(character);
                self.reselect();
            }
            _ => {}
        }
    }

    pub(super) fn type_into_help(&mut self, key: Key) {
        let hidden = self.help_hidden;
        match key {
            Key::Escape | Key::Char('?' | 'q') => {
                self.mode = Mode::Browse;
                self.help_offset = 0;
            }
            Key::Down | Key::Char('j') => self.help_offset = (self.help_offset + 1).min(hidden),
            Key::Up | Key::Char('k') => self.help_offset = self.help_offset.saturating_sub(1),
            Key::Char('g') => self.help_offset = 0,
            Key::Char('G') => self.help_offset = hidden,
            Key::Char('n') => self.help_offset = (self.help_offset + 10).min(hidden),
            Key::Char('p') => self.help_offset = self.help_offset.saturating_sub(10),
            _ => {}
        }
    }

    pub(super) fn run(&mut self, action: Action) -> Flow<Exit> {
        let sidebar = self.focus == Focus::Sidebar;
        match action {
            Action::MoveUp if sidebar => self.select_table(self.table_index.saturating_sub(1)),
            Action::MoveDown if sidebar => {
                let last = self.visible_tables().len().saturating_sub(1);
                self.select_table((self.table_index + 1).min(last));
            }
            Action::MoveUp => self.row_index = self.row_index.saturating_sub(1),
            Action::MoveDown => {
                self.row_index = (self.row_index + 1).min(self.rows.len().saturating_sub(1))
            }
            Action::MoveLeft => self.column_index = self.column_index.saturating_sub(1),
            Action::MoveRight => {
                self.column_index =
                    (self.column_index + 1).min(self.headers.len().saturating_sub(1));
            }
            Action::Activate if sidebar => {
                self.offset = 0;
                self.load(false);
                self.focus = Focus::Grid;
            }
            Action::Command => self.command = Some(String::new()),
            Action::NextPane | Action::PreviousPane => {
                self.toggle_focus(action == Action::NextPane)
            }
            Action::FocusTables => {
                self.tables_hidden = false;
                self.focus = Focus::Sidebar;
                self.mode = Mode::Browse;
            }
            Action::FocusRows => {
                self.focus = Focus::Grid;
                self.mode = Mode::Browse;
            }
            Action::ToggleTables => self.toggle_tables(),
            Action::SortColumn => self.sort_by_current_column(),
            Action::FilterTables => {
                self.filtering = true;
                self.tables_hidden = false;
                self.focus = Focus::Sidebar;
                self.say("filtering tables · ↵ keeps it · esc clears it");
            }
            Action::NextPage | Action::PreviousPage => self.page(action == Action::NextPage),
            Action::Reload => self.reload(),
            Action::Connections => return Flow::Exit(Exit::Connections),
            Action::Narrow => self.resize(Some(-4)),
            Action::Widen => self.resize(Some(4)),
            Action::ResetWidth => self.resize(None),
            Action::Help => {
                self.help_offset = 0;
                self.mode = Mode::Help;
            }
            Action::Quit => return Flow::Exit(Exit::Quit),
            Action::Databases => self.open_databases(),
            Action::Activate => self.say("editing is not ported yet"),
            Action::Sql | Action::FocusSql => self.open_query(),
            Action::History => self.open_history(),
            Action::Palette => self.open_palette(),
            Action::Escape => self.back_to_table(),
            Action::Redraw => {}
            other => {
                let description = keymap::binding(other).description;
                self.say(format!("{description}: not ported yet"));
            }
        }
        Flow::Continue
    }
}
