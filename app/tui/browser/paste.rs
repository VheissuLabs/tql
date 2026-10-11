use super::*;

impl Browser {
    pub(super) fn paste(&mut self, text: &str) {
        if self.problem.is_some() {
            return;
        }
        if self.edits.form().is_some() {
            return self.paste_into_edits(text);
        }
        if let Some(palette) = self.palette.as_mut() {
            single_line(text)
                .chars()
                .for_each(|character| palette.type_key(Key::Char(character)));
            return;
        }
        if self.mode == Mode::Help || self.structure.is_some() || self.inspector.is_some() {
            return;
        }
        if self.mode == Mode::Query {
            return self.paste_into_query(text);
        }
        if self.edits.captures_keys() {
            return self.paste_into_edits(text);
        }
        if let Some(databases) = self.databases.as_mut() {
            single_line(text)
                .chars()
                .for_each(|character| databases.type_key(Key::Char(character)));
            return;
        }
        if self.link_picker.is_some() {
            return self.paste_into_link_picker(text);
        }
        if self.filter_form.is_some() {
            return self.paste_into_filter_form(text);
        }
        if self.question.is_some() {
            return self.paste_into_question(text);
        }
        if let Some(command) = self.command.as_mut() {
            command.push_str(&single_line(text));
            return;
        }
        if self.filtering {
            self.filter.push_str(&single_line(text));
            self.reselect();
        }
    }
}
