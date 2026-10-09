use super::*;

pub const FROM_AGENT: &str = "SQL from your agent · nothing has run";

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

    fn busy_elsewhere(&self) -> bool {
        self.problem.is_some()
            || self.edits.form().is_some()
            || self.edits.captures_keys()
            || self.palette.is_some()
            || self.databases.is_some()
            || self.link_picker.is_some()
            || self.filter_form.is_some()
            || self.command.is_some()
            || self.structure.is_some()
            || self.inspector.is_some()
            || self.filtering
            || self.mode == Mode::Help
    }
}
