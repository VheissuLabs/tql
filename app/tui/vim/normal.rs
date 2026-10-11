use super::*;

impl Vim {
    pub(super) fn press_in_normal(&mut self, editor: &mut QueryEditor, key: Key) -> Outcome {
        if self.keys.is_empty() {
            match key {
                Key::Escape => return Outcome::Leave,
                Key::Tab => return Outcome::NextPane,
                Key::BackTab => return Outcome::PreviousPane,
                Key::Char(':') => return Outcome::Command,
                _ => {}
            }
        }
        if key == Key::Escape {
            self.keys.clear();
            return Outcome::Nothing;
        }

        self.keys.push(key);
        let keys = self.keys.clone();
        match self.parse(&keys, true, false) {
            Parsed::Waiting => {}
            Parsed::Invalid => self.keys.clear(),
            Parsed::Ready(command) => {
                self.keys.clear();
                self.run(editor, command);
            }
        }
        Outcome::Nothing
    }

    pub(super) fn run(&mut self, editor: &mut QueryEditor, command: Command) {
        match command.order {
            Order::Operate(operator, operand) => {
                self.operate(editor, operator, operand, command.count, command.counted);
                self.remember(&command);
            }
            Order::Go(Target::Motion(motion)) => {
                self.move_cursor(editor, motion, command.count, command.counted)
            }
            Order::Go(Target::Object(_)) => {}
            Order::Act(key, character) => {
                self.act(editor, key, character, command.count, command.counted);
                if is_change(key) {
                    self.remember(&command);
                }
            }
        }
    }

    pub(super) fn remember(&mut self, command: &Command) {
        if self.replaying || matches!(command.order, Order::Operate('y', _)) {
            return;
        }
        let mut keys = if command.counted {
            digits_of(command.count)
        } else {
            Vec::new()
        };
        keys.extend(command.body.iter().copied());
        if self.mode == VimMode::Insert {
            self.recording = Some(keys);
            return;
        }
        self.last_change = Some(keys);
    }

    pub(super) fn act(
        &mut self,
        editor: &mut QueryEditor,
        key: Key,
        character: Option<Key>,
        count: usize,
        counted: bool,
    ) {
        match key {
            Key::Char('x') => self.operate_with(editor, 'd', Motion::Right, count),
            Key::Char('X') => self.operate_with(editor, 'd', Motion::Left, count),
            Key::Char('D') => self.operate_with(editor, 'd', Motion::LineEnd, count),
            Key::Char('C') => self.operate_with(editor, 'c', Motion::LineEnd, count),
            Key::Char('s') => self.substitute(editor, count),
            Key::Char('S') => self.operate(editor, 'c', Operand::Lines, count, false),
            Key::Char('Y') => self.operate(editor, 'y', Operand::Lines, count, false),
            Key::Char('r') => replace_characters(editor, character, count),
            Key::Char('J') => self.join(editor, count),
            Key::Char('~') => self.swap_case_under(editor, count),
            Key::Char(side @ ('p' | 'P')) => self.put(editor, side == 'p', count),
            Key::Char('u') => self.undo(editor, count),
            Key::Ctrl('r') => self.redo(editor, count),
            Key::Char('.') => self.repeat(editor, counted.then_some(count)),
            Key::Char(entry @ ('i' | 'a' | 'I' | 'A' | 'o' | 'O')) => self.insert(editor, entry),
            Key::Char('v') => self.start_visual(editor, VimMode::Visual),
            Key::Char('V') => self.start_visual(editor, VimMode::VisualLine),
            _ => {}
        }
    }

    pub(super) fn operate_with(
        &mut self,
        editor: &mut QueryEditor,
        operator: char,
        motion: Motion,
        count: usize,
    ) {
        if Text::of(&editor.buffer()).is_blank_line(editor.cursor()) && motion != Motion::LineEnd {
            return;
        }
        self.operate(
            editor,
            operator,
            Operand::On(Target::Motion(motion)),
            count,
            true,
        );
    }

    pub(super) fn substitute(&mut self, editor: &mut QueryEditor, count: usize) {
        if Text::of(&editor.buffer()).is_blank_line(editor.cursor()) {
            self.enter_insert(editor);
            return;
        }
        self.operate_with(editor, 'c', Motion::Right, count);
    }

    pub(super) fn join(&mut self, editor: &mut QueryEditor, count: usize) {
        let line = Text::of(&editor.buffer()).line_of(editor.cursor());
        editor.checkpoint();
        operators::join_lines(editor, line, line + count.saturating_sub(1).max(1));
        editor.commit();
    }

    pub(super) fn swap_case_under(&mut self, editor: &mut QueryEditor, count: usize) {
        let cursor = editor.cursor();
        let end = Text::of(&editor.buffer())
            .line_end(cursor)
            .min(cursor + count);
        if end <= cursor {
            return;
        }
        editor.checkpoint();
        self.apply(editor, '~', Range::new(cursor, end));
        editor.move_to(end);
        editor.commit();
    }

    pub(super) fn put(&mut self, editor: &mut QueryEditor, after: bool, count: usize) {
        if self.register.is_empty() {
            return;
        }
        let text = Text::of(&editor.buffer());
        let cursor = editor.cursor();
        editor.checkpoint();

        if self.register_is_lines {
            let lines = self.register.repeat(count);
            if after {
                let at = text.line_end(cursor);
                editor.replace(at, at, &format!("\n{}", lines.trim_end_matches('\n')));
                editor.move_to(motions::first_non_blank(
                    &Text::of(&editor.buffer()),
                    at + 1,
                ));
            } else {
                let at = text.line_start(cursor);
                editor.replace(at, at, &lines);
                editor.move_to(motions::first_non_blank(&Text::of(&editor.buffer()), at));
            }
            editor.commit();
            return;
        }

        let at = if after && !text.is_blank_line(cursor) {
            (cursor + 1).min(text.line_end(cursor))
        } else {
            cursor
        };
        let pasted = self.register.repeat(count);
        editor.replace(at, at, &pasted);
        editor.move_to((at + pasted.chars().count()).saturating_sub(1));
        editor.commit();
    }

    pub(super) fn undo(&mut self, editor: &mut QueryEditor, count: usize) {
        if (0..count).any(|_| !editor.undo()) {
            self.message = Some("already at oldest change".to_string());
        }
    }

    pub(super) fn redo(&mut self, editor: &mut QueryEditor, count: usize) {
        if (0..count).any(|_| !editor.redo()) {
            self.message = Some("already at newest change".to_string());
        }
    }

    pub(super) fn repeat(&mut self, editor: &mut QueryEditor, count: Option<usize>) {
        let Some(mut keys) = self.last_change.clone() else {
            return;
        };
        if let Some(count) = count {
            let uncounted: Vec<Key> = keys.into_iter().skip_while(is_digit).collect();
            keys = digits_of(count);
            keys.extend(uncounted);
        }
        self.replaying = true;
        for key in keys {
            self.press(editor, key);
        }
        self.replaying = false;
    }
}

pub(super) fn replace_characters(editor: &mut QueryEditor, character: Option<Key>, count: usize) {
    let Some(Key::Char(replacement)) =
        character.filter(|key| matches!(key, Key::Char(character) if !character.is_control()))
    else {
        return;
    };
    let cursor = editor.cursor();
    if cursor + count > Text::of(&editor.buffer()).line_end(cursor) {
        return;
    }
    editor.checkpoint();
    editor.replace(
        cursor,
        cursor + count,
        &replacement.to_string().repeat(count),
    );
    editor.move_to(cursor + count - 1);
    editor.commit();
}
