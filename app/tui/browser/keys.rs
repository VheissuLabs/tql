use super::*;
use crate::registry::{self, Name, Screen};

impl Browser {
    pub(super) fn toggle_tables(&mut self) {
        self.tables_hidden = !self.tables_hidden;
        if self.tables_hidden && self.focus == Focus::Sidebar {
            self.focus = Focus::Grid;
        }
        if self.tables_hidden {
            let key = keymap::spelled(Action::ToggleTables);
            self.say(format!("table list hidden · {key} shows it"));
        } else {
            self.hush();
        }
    }

    pub(super) fn sql_shown(&self) -> bool {
        self.mode == Mode::Query
            || !self.sql_hidden && settings::with(|settings| settings.sql_always)
    }

    pub(super) fn toggle_sql(&mut self) {
        if !self.sql_shown() {
            self.sql_hidden = false;
            self.open_query();
            return;
        }
        if self.mode == Mode::Query {
            self.leave_query();
        }
        self.sql_hidden = true;
        let key = keymap::spelled(Action::ToggleSql);
        self.say(format!("SQL editor hidden · {key} shows it"));
    }

    pub(super) fn toggle_focus(&mut self, forward: bool) {
        let sql_shown = self.sql_shown();
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
            .unwrap_or_else(|| {
                grid::natural_width(&self.headers, &self.rows, self.column_index)
                    .min(grid::COMFORTABLE)
            });
        let width = (current as isize + by).clamp(3, 120) as usize;
        self.width_overrides.insert(column.clone(), width);
        self.say(format!("{column} width {width}"));
    }

    pub(super) fn run_command(&mut self, command: &str) -> Flow<Exit> {
        self.command = None;
        match registry::parse(command) {
            Some((entry, argument)) if entry.works_on(Screen::Browser) => {
                self.run_named(entry.name, &argument)
            }
            _ => {
                self.say(registry::unavailable(Screen::Browser, command));
                Flow::Continue
            }
        }
    }

    pub fn run_named(&mut self, name: Name, argument: &str) -> Flow<Exit> {
        match name {
            Name::Quit => return self.leave(Exit::Quit),
            Name::Connections => return self.leave(Exit::Connections),
            Name::Tables => self.focus = Focus::Sidebar,
            Name::Rows => self.focus = Focus::Grid,
            Name::RunOrReload => self.run_or_reload(),
            Name::Reload => self.reload(),
            Name::Run => self.run_from_editor(),
            Name::Sql => self.open_query(),
            Name::History => self.open_history(),
            Name::Write => self.write_pending(),
            Name::Export => self.export(),
            Name::CreateDatabase => self.create_database(argument),
            Name::DropDatabase => self.drop_database(argument),
            Name::New => self.say(registry::unavailable(Screen::Browser, "new")),
            Name::Settings => {
                self.settings = Some(crate::tui::settings_modal::SettingsModal::open(
                    &crate::paths::config_file(),
                ))
            }
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
            Key::Backspace | Key::Ctrl('h') => {
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

    pub(super) fn type_into_browse(&mut self, key: Key) -> Flow<Exit> {
        if let Key::Char(digit) = key
            && digit.is_ascii_digit()
            && (digit != '0' || !self.count_prefix.is_empty())
        {
            self.count_prefix.push(digit);
            return Flow::Continue;
        }

        let typed = std::mem::take(&mut self.count_prefix);
        let count = if typed.is_empty() {
            0
        } else {
            typed.parse().unwrap_or(usize::MAX)
        };
        let Some(action) = keymap::action(key) else {
            return Flow::Continue;
        };
        let moves = matches!(
            action,
            Action::MoveUp | Action::MoveDown | Action::MoveLeft | Action::MoveRight
        );
        if count > 1 && moves {
            return self.move_by(action, count);
        }
        self.run(action)
    }

    fn move_by(&mut self, action: Action, count: usize) -> Flow<Exit> {
        if self.focus == Focus::Sidebar {
            let last = self.visible_tables().len().saturating_sub(1);
            match action {
                Action::MoveUp => self.select_table(self.table_index.saturating_sub(count)),
                Action::MoveDown => {
                    self.select_table(self.table_index.saturating_add(count).min(last))
                }
                other => return self.run(other),
            }
            return Flow::Continue;
        }

        let last_row = self.rows.len().saturating_sub(1);
        let last_column = self.headers.len().saturating_sub(1);
        match action {
            Action::MoveUp => self.row_index = self.row_index.saturating_sub(count),
            Action::MoveDown => self.row_index = self.row_index.saturating_add(count).min(last_row),
            Action::MoveLeft => self.column_index = self.column_index.saturating_sub(count),
            _ => self.column_index = self.column_index.saturating_add(count).min(last_column),
        }
        Flow::Continue
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
            Action::MoveLeft if sidebar => {}
            Action::MoveLeft if self.column_index == 0 => {
                self.tables_hidden = false;
                self.focus = Focus::Sidebar;
                self.mode = Mode::Browse;
            }
            Action::MoveLeft => self.column_index -= 1,
            Action::MoveRight if sidebar => self.toggle_focus(true),
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
            Action::ToggleSql => self.toggle_sql(),
            Action::SortColumn => self.sort_by_current_column(),
            Action::FilterRows => self.open_filters(),
            Action::FilterTables => {
                self.filtering = true;
                self.tables_hidden = false;
                self.focus = Focus::Sidebar;
                self.say("filtering tables · ↵ keeps it · esc clears it");
            }
            Action::NextPage | Action::PreviousPage => self.page(action == Action::NextPage),
            Action::Reload => self.reload(),
            Action::Connections => return self.leave(Exit::Connections),
            Action::Narrow => self.resize(Some(-4)),
            Action::Widen => self.resize(Some(4)),
            Action::ResetWidth => self.resize(None),
            Action::Help => {
                self.help_offset = 0;
                self.mode = Mode::Help;
            }
            Action::Quit => return self.leave(Exit::Quit),
            Action::Databases => self.open_databases(),
            Action::Activate | Action::EditValue => self.start_editing(false),
            Action::ViewValue => self.start_editing(true),
            Action::EditRow => self.open_record(),
            Action::MarkDelete => self.mark_delete(),
            Action::ClearMarks => self.clear_marks(),
            Action::NewRow => self.new_row(),
            Action::InspectRow => self.inspect_row(),
            Action::Structure => self.toggle_structure(),
            Action::FollowLink => self.follow_link(),
            Action::JumpBack => self.jump_back(),
            Action::YankValue => self.yank_value(),
            Action::YankRow => self.yank_row(),
            Action::Sql | Action::FocusSql => self.open_query(),
            Action::History => self.open_history(),
            Action::Palette => self.open_palette(),
            Action::Ask => self.open_question(),
            Action::Escape => self.escape(),
            Action::Redraw => {}
        }
        Flow::Continue
    }
}
