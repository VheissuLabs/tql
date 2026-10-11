use super::*;

impl Vim {
    pub(super) fn enter_insert(&mut self, editor: &mut QueryEditor) {
        editor.checkpoint();
        self.mode = VimMode::Insert;
        self.anchor = None;
    }

    pub(super) fn press_in_insert(&mut self, editor: &mut QueryEditor, key: Key) {
        if let Some(recording) = self.recording.as_mut() {
            recording.push(key);
        }
        match key {
            Key::Escape => self.leave_insert(editor),
            Key::Left | Key::Right | Key::Up | Key::Down => {
                editor.commit();
                editor.handle(key);
                editor.checkpoint();
            }
            Key::Ctrl('w') => delete_word_before(editor),
            Key::Ctrl('u') => {
                let cursor = editor.cursor();
                let start = Text::of(&editor.buffer()).line_start(cursor);
                editor.replace(start, cursor, "");
            }
            Key::Ctrl('r') => {}
            Key::ShiftEnter => editor.handle(Key::Enter),
            other => editor.handle(other),
        }
    }

    pub(super) fn leave_insert(&mut self, editor: &mut QueryEditor) {
        editor.commit();
        self.mode = VimMode::Normal;
        let cursor = editor.cursor();
        if cursor > Text::of(&editor.buffer()).line_start(cursor) {
            editor.move_to(cursor - 1);
        }
        let recording = self.recording.take();
        if !self.replaying && recording.is_some() {
            self.last_change = recording;
        }
    }

    pub(super) fn insert(&mut self, editor: &mut QueryEditor, entry: char) {
        let text = Text::of(&editor.buffer());
        let cursor = editor.cursor();
        self.enter_insert(editor);
        match entry {
            'a' => editor.move_to((cursor + 1).min(text.line_end(cursor))),
            'I' => editor.move_to(motions::first_non_blank(&text, cursor)),
            'A' => editor.move_to(text.line_end(cursor)),
            'o' => editor.open_line(false),
            'O' => editor.open_line(true),
            _ => {}
        }
    }
}

pub(super) fn delete_word_before(editor: &mut QueryEditor) {
    let text = Text::of(&editor.buffer());
    let cursor = editor.cursor();
    let line_start = text.line_start(cursor);
    if cursor == line_start {
        editor.handle(Key::Backspace);
        return;
    }

    let before = |offset: usize| offset.checked_sub(1).and_then(|previous| text.at(previous));
    let mut start = cursor;
    while start > line_start && kind(before(start), false) == 0 {
        start -= 1;
    }
    let word = kind(before(start), false);
    while start > line_start && word != 0 && kind(before(start), false) == word {
        start -= 1;
    }
    editor.replace(start, cursor, "");
}
pub(super) fn keep_on_character(editor: &mut QueryEditor) {
    let text = Text::of(&editor.buffer());
    let cursor = editor.cursor();
    let end = text.line_end(cursor);
    if cursor >= end && end > text.line_start(cursor) {
        editor.move_to(end - 1);
    }
}
