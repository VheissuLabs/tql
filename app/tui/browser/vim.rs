use super::*;
use crate::tui::completion::Completion;
use crate::tui::vim::{Outcome, VimMode};

fn vim_editor() -> bool {
    settings::with(|settings| settings.sql_vim)
}

pub(super) fn query_hint() -> &'static str {
    if vim_editor() {
        "i types · :r runs it · esc returns"
    } else {
        "ctrl+r runs it · ↵ adds a line · esc returns"
    }
}

impl Browser {
    pub(super) fn enter_query_mode(&mut self) {
        if self.mode != Mode::Query {
            self.query.vim.reset();
        }
        self.mode = Mode::Query;
    }

    pub(super) fn type_into_vim(&mut self, key: Key, offered: Option<Completion>) -> Flow<Exit> {
        if let Some(offered) = offered
            && self.query.vim.mode == VimMode::Insert
            && self.take_completion_key(key, offered)
        {
            return Flow::Continue;
        }

        let outcome = self.query.vim.press(&mut self.query.editor, key);
        if self.query.vim.mode == VimMode::Insert {
            self.offer_completion_after(key);
        }
        self.settle_vim();

        match outcome {
            Outcome::Leave => self.leave_query(),
            Outcome::NextPane => self.toggle_focus(true),
            Outcome::PreviousPane => self.toggle_focus(false),
            Outcome::Command => self.command = Some(String::new()),
            Outcome::Nothing => {}
        }
        Flow::Continue
    }

    pub(super) fn complete_in_vim(&mut self, typed: usize, chosen: &str) {
        let keys = std::iter::repeat_n(Key::Backspace, typed).chain(chosen.chars().map(Key::Char));
        for key in keys {
            self.query.vim.press(&mut self.query.editor, key);
        }
        self.settle_vim();
    }

    fn settle_vim(&mut self) {
        if let Some(yanked) = self.query.vim.take_copied() {
            self.clipboard.copy(&yanked);
        }
        if let Some(message) = self.query.vim.message.clone() {
            self.say(message);
        }
    }

    pub(super) fn run_from_editor(&mut self) {
        let selected = self.query.vim.selected_text(&self.query.editor);
        self.query.vim.leave_visual();
        match selected {
            Some(selected) => self.run_query(&selected),
            None => self.run_statement_at_cursor(),
        }
    }

    pub fn inserts_at_terminal_cursor(&self) -> bool {
        self.mode == Mode::Query
            && vim_editor()
            && self.query.vim.mode == VimMode::Insert
            && self.command.is_none()
            && self.palette.is_none()
            && self.problem.is_none()
    }

    pub(super) fn vim_mode_label(&self) -> String {
        if self.mode != Mode::Query || !vim_editor() {
            return String::new();
        }
        format!(" · {}", self.query.vim.mode.label())
    }

    pub(super) fn vim_selection(&self) -> Option<std::ops::Range<usize>> {
        if self.mode != Mode::Query || !vim_editor() {
            return None;
        }
        let selection = self.query.vim.selection(&self.query.editor)?;
        let end = if selection.linewise {
            selection.end + 1
        } else {
            selection.end
        };
        Some(selection.start..end)
    }

    pub(super) fn vim_offers(&self) -> Option<Vec<(String, &'static str)>> {
        if self.mode != Mode::Query || !vim_editor() {
            return None;
        }
        let offers: &[(&str, &'static str)] = match self.query.vim.mode {
            VimMode::Insert => &[("↵", "New line"), ("tab", "Indent"), ("esc", "Normal")],
            mode if mode.is_visual() => &[
                (":r", "Run selection"),
                ("y", "Yank"),
                ("d", "Delete"),
                ("esc", "Normal"),
            ],
            _ => &[
                (":r", "Run"),
                ("i", "Insert"),
                ("u", "Undo"),
                ("esc", "Grid"),
            ],
        };
        let mut offers: Vec<(String, &'static str)> = offers
            .iter()
            .map(|(key, label)| (key.to_string(), *label))
            .collect();
        if self.query.vim.mode == VimMode::Normal {
            offers.push((keymap::key(Action::Palette, 0), "More"));
        }
        Some(offers)
    }
}
