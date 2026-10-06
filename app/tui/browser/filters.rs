use super::*;
use crate::tui::filters::{Cell, FilterForm};

const FORM_KEYS: [char; 11] = ['h', 'j', 'k', 'l', 'n', 'd', 'o', 'i', ' ', '+', '-'];

impl Browser {
    pub(super) fn open_filters(&mut self) {
        if self.headers.is_empty() {
            self.say("open a table first");
            return;
        }
        self.filter_form = Some(FilterForm::new(
            self.headers.clone(),
            self.filters.clone(),
            self.headers.get(self.column_index).cloned(),
        ));
        self.hush();
    }

    pub(super) fn type_into_filter_form(&mut self, key: Key) {
        let Some(form) = self.filter_form.as_mut() else {
            return;
        };

        if let Some(picker) = form.picker.as_mut() {
            match key {
                Key::Escape => form.picker = None,
                Key::Enter => form.choose(),
                Key::Up | Key::Ctrl('p') => picker.move_by(false),
                Key::Down | Key::Ctrl('n') => picker.move_by(true),
                other => picker.type_key(other),
            }
            return;
        }

        if let Some(editor) = form.editor.as_mut() {
            match key {
                Key::Escape => form.abandon(),
                Key::Enter => form.commit(),
                Key::Ctrl('s') => self.apply_filters(),
                other => editor.handle(other),
            }
            return;
        }

        if form.cell == Cell::Value {
            match key {
                Key::Enter => return self.apply_filters(),
                Key::Char(character) if !FORM_KEYS.contains(&character) => {
                    form.start_editing();
                    if let Some(editor) = form.editor.as_mut() {
                        editor.handle(key);
                    }
                    return;
                }
                _ => {}
            }
        }

        match key {
            Key::Escape => self.clear_filters(),
            Key::Ctrl('s') => self.apply_filters(),
            Key::Up | Key::Char('k') => form.move_row(false),
            Key::Down | Key::Char('j') => form.move_row(true),
            Key::Left | Key::Char('h') => move_or_cycle(form, false),
            Key::Right | Key::Char('l') => move_or_cycle(form, true),
            Key::Tab => form.move_cell(true),
            Key::BackTab => form.move_cell(false),
            Key::Char('+' | 'n') => form.add(),
            Key::Char('-' | 'd') => form.remove(),
            Key::Char('o') => form.toggle_joiner(),
            Key::Enter | Key::Char('i') if form.cell == Cell::Value => form.start_editing(),
            Key::Enter | Key::Char('i') => form.open_picker(),
            _ => {}
        }
    }

    pub(super) fn apply_filters(&mut self) {
        let Some(mut form) = self.filter_form.take() else {
            return;
        };
        form.commit();
        self.filters = Some(form.to_filters()).filter(|filters| filters.usable());
        self.offset = 0;
        self.edits.deletes.clear();
        self.edits.updates.clear();
        self.load(false);
    }

    pub(super) fn clear_filters(&mut self) {
        self.filter_form = None;
        if self.filters.take().is_none() {
            self.hush();
            return;
        }
        self.offset = 0;
        self.load(false);
        self.say("filter cleared");
    }
}

fn move_or_cycle(form: &mut FilterForm, forward: bool) {
    if form.cell == Cell::Value {
        form.move_cell(forward);
        return;
    }
    form.cycle(forward);
}
