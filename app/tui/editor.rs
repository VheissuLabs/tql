use crate::terminal::Key;

#[derive(Debug, Clone, Default)]
pub struct QueryEditor {
    characters: Vec<char>,
    cursor: usize,
    autoindent: bool,
    undone: Vec<(Vec<char>, usize)>,
    redone: Vec<(Vec<char>, usize)>,
    pending: Option<(Vec<char>, usize)>,
}

impl QueryEditor {
    pub fn new(autoindent: bool) -> Self {
        QueryEditor {
            autoindent,
            ..QueryEditor::default()
        }
    }

    pub fn holding(buffer: &str) -> Self {
        let mut editor = QueryEditor::new(false);
        editor.set(buffer);
        editor
    }

    pub fn buffer(&self) -> String {
        self.characters.iter().collect()
    }

    pub fn cursor(&self) -> usize {
        self.cursor
    }

    pub fn is_empty(&self) -> bool {
        self.characters
            .iter()
            .all(|character| character.is_whitespace())
    }

    pub fn set(&mut self, buffer: &str) {
        self.characters = buffer.chars().collect();
        self.cursor = self.characters.len();
    }

    pub fn move_to(&mut self, offset: usize) {
        self.cursor = offset.min(self.characters.len());
    }

    pub fn replace(&mut self, start: usize, end: usize, text: &str) {
        let inserted: Vec<char> = text.chars().collect();
        self.cursor = start + inserted.len();
        self.characters.splice(start..end, inserted);
    }

    pub fn checkpoint(&mut self) {
        if self.pending.is_none() {
            self.pending = Some((self.characters.clone(), self.cursor));
        }
    }

    pub fn commit(&mut self) {
        if let Some(pending) = self.pending.take()
            && pending.0 != self.characters
        {
            self.undone.push(pending);
            self.redone.clear();
        }
    }

    pub fn undo(&mut self) -> bool {
        self.commit();
        let (mut from, mut to) = (
            std::mem::take(&mut self.undone),
            std::mem::take(&mut self.redone),
        );
        let stepped = self.step(&mut from, &mut to);
        (self.undone, self.redone) = (from, to);
        stepped
    }

    pub fn redo(&mut self) -> bool {
        self.commit();
        let (mut from, mut to) = (
            std::mem::take(&mut self.redone),
            std::mem::take(&mut self.undone),
        );
        let stepped = self.step(&mut from, &mut to);
        (self.redone, self.undone) = (from, to);
        stepped
    }

    fn step(
        &mut self,
        from: &mut Vec<(Vec<char>, usize)>,
        to: &mut Vec<(Vec<char>, usize)>,
    ) -> bool {
        while from
            .last()
            .is_some_and(|(characters, _)| *characters == self.characters)
        {
            from.pop();
        }
        let Some((characters, cursor)) = from.pop() else {
            return false;
        };
        to.push((
            std::mem::replace(&mut self.characters, characters),
            std::mem::replace(&mut self.cursor, cursor),
        ));
        true
    }

    pub fn move_to_start(&mut self) {
        self.cursor = 0;
    }

    pub fn move_to_end(&mut self) {
        self.cursor = self.characters.len();
    }

    pub fn move_to_line_column(&mut self, line: usize, column: usize) {
        let lines = self.lines();
        let line = line.min(lines.len() - 1);
        let start: usize = lines[..line]
            .iter()
            .map(|text| text.chars().count() + 1)
            .sum();
        self.cursor = start + column.min(lines[line].chars().count());
    }

    pub fn lines(&self) -> Vec<String> {
        self.buffer().split('\n').map(String::from).collect()
    }

    pub fn cursor_line(&self) -> usize {
        self.characters[..self.cursor]
            .iter()
            .filter(|character| **character == '\n')
            .count()
    }

    pub fn cursor_column(&self) -> usize {
        let before = &self.characters[..self.cursor];
        match before.iter().rposition(|character| *character == '\n') {
            Some(newline) => before.len() - newline - 1,
            None => before.len(),
        }
    }

    pub fn handle(&mut self, key: Key) {
        match key {
            Key::Enter => self.newline(),
            Key::Backspace | Key::Ctrl('h') => self.backspace(),
            Key::Delete => self.delete(),
            Key::Left | Key::Ctrl('b') => self.cursor = self.cursor.saturating_sub(1),
            Key::Right | Key::Ctrl('f') => {
                self.cursor = (self.cursor + 1).min(self.characters.len())
            }
            Key::Up => self.move_line(false),
            Key::Down => self.move_line(true),
            Key::Home | Key::Ctrl('a') => self.move_to_line_start(),
            Key::End | Key::Ctrl('e') => self.move_to_line_end(),
            Key::Tab => self.insert("  "),
            Key::BackTab => self.outdent(),
            Key::Char(character) if !character.is_control() => self.insert(&character.to_string()),
            _ => {}
        }
    }

    fn insert(&mut self, text: &str) {
        let at = self.cursor;
        self.replace(at, at, text);
    }

    fn backspace(&mut self) {
        if self.cursor == 0 {
            return;
        }
        self.cursor -= 1;
        self.characters.remove(self.cursor);
    }

    fn delete(&mut self) {
        if self.cursor < self.characters.len() {
            self.characters.remove(self.cursor);
        }
    }

    fn move_line(&mut self, down: bool) {
        let line = self.cursor_line();
        let target = if down {
            line + 1
        } else {
            let Some(above) = line.checked_sub(1) else {
                return;
            };
            above
        };
        if target >= self.lines().len() {
            return;
        }
        self.move_to_line_column(target, self.cursor_column());
    }

    fn move_to_line_start(&mut self) {
        self.cursor -= self.cursor_column();
    }

    fn move_to_line_end(&mut self) {
        let length = self.lines()[self.cursor_line()].chars().count();
        self.cursor += length - self.cursor_column();
    }

    pub fn autoindents(&self) -> bool {
        self.autoindent
    }

    pub fn open_line(&mut self, above: bool) {
        let carried = if self.autoindent {
            self.indentation()
        } else {
            String::new()
        };
        if above {
            self.move_to_line_start();
            self.insert(&format!("{carried}\n"));
            self.cursor -= 1;
            return;
        }
        self.move_to_line_end();
        self.insert(&format!("\n{carried}"));
    }

    pub fn indentation(&self) -> String {
        self.lines()[self.cursor_line()]
            .chars()
            .take_while(|character| *character == ' ' || *character == '\t')
            .collect()
    }

    fn outdent(&mut self) {
        let line = &self.lines()[self.cursor_line()];
        let removed = line
            .chars()
            .take(2)
            .take_while(|character| *character == ' ')
            .count();
        if removed == 0 {
            return;
        }
        let column = self.cursor_column();
        let start = self.cursor - column;
        self.characters.drain(start..start + removed);
        self.cursor = start + column.saturating_sub(removed);
    }

    fn newline(&mut self) {
        let carried = if self.autoindent {
            self.indentation()
        } else {
            String::new()
        };
        let kept: String = carried.chars().take(self.cursor_column()).collect();
        self.insert(&format!("\n{kept}"));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn typed(text: &str) -> QueryEditor {
        let mut editor = QueryEditor::new(true);
        for character in text.chars() {
            editor.handle(Key::Char(character));
        }
        editor
    }

    #[test]
    fn inserts_at_the_cursor_after_moving_left() {
        let mut editor = typed("sect");
        editor.handle(Key::Left);
        editor.handle(Key::Left);
        editor.handle(Key::Char('l'));
        editor.handle(Key::Char('e'));

        assert_eq!(editor.buffer(), "select");
    }

    #[test]
    fn moves_between_lines_keeping_the_column_where_it_can() {
        let mut editor = QueryEditor::new(false);
        editor.set("select *\nfrom tracks");
        editor.handle(Key::Up);

        assert_eq!((editor.cursor_line(), editor.cursor_column()), (0, 8));

        editor.handle(Key::Up);
        editor.handle(Key::Home);
        editor.handle(Key::Down);
        editor.handle(Key::End);

        assert_eq!((editor.cursor_line(), editor.cursor_column()), (1, 11));
    }

    #[test]
    fn keeps_the_indentation_on_a_new_line_but_never_past_the_cursor() {
        let mut editor = typed("  where a = 1");
        editor.handle(Key::Enter);
        for character in "and b = 2".chars() {
            editor.handle(Key::Char(character));
        }
        assert_eq!(editor.buffer(), "  where a = 1\n  and b = 2");

        let mut shallow = QueryEditor::new(true);
        shallow.set("    where");
        shallow.move_to(2);
        shallow.handle(Key::Enter);
        assert_eq!(shallow.buffer(), "  \n    where");
    }

    #[test]
    fn indents_with_tab_and_outdents_only_the_spaces_a_line_has() {
        let mut editor = QueryEditor::new(false);
        editor.set("select *\n    from fruit");
        editor.handle(Key::BackTab);
        assert_eq!(editor.buffer(), "select *\n  from fruit");
        assert_eq!(editor.cursor_column(), 12);

        let mut shallow = QueryEditor::new(false);
        shallow.set(" from fruit");
        shallow.move_to_start();
        shallow.handle(Key::BackTab);
        shallow.handle(Key::BackTab);
        shallow.handle(Key::Tab);
        assert_eq!(shallow.buffer(), "  from fruit");
    }

    #[test]
    fn ignores_control_characters_and_backspace_at_the_start() {
        let mut editor = typed("select");
        editor.handle(Key::Char('\0'));
        editor.move_to_start();
        editor.handle(Key::Backspace);

        assert_eq!(editor.buffer(), "select");
        assert!(QueryEditor::new(false).is_empty() && typed("   ").is_empty());
    }

    #[test]
    fn opens_holding_a_value_with_the_cursor_after_it_and_jumps_between_lines() {
        let mut editor = QueryEditor::holding("a\nbb\nc");
        assert_eq!(editor.cursor(), 6);

        editor.move_to_line_column(1, 0);
        assert_eq!((editor.cursor_line(), editor.cursor_column()), (1, 0));

        editor.move_to_line_column(40, 0);
        assert_eq!(editor.cursor_line(), 2);

        editor.move_to_start();
        editor.move_to_end();
        assert_eq!(editor.cursor(), 6);
    }

    #[test]
    fn undoes_and_redoes_to_each_checkpoint() {
        let mut editor = QueryEditor::new(false);
        editor.checkpoint();
        editor.set("select");
        editor.commit();
        editor.checkpoint();
        editor.set("select name");

        assert!(editor.undo());
        assert_eq!(editor.buffer(), "select");
        assert!(editor.undo());
        assert_eq!(editor.buffer(), "");
        assert!(!editor.undo());
        assert!(editor.redo() && editor.redo());
        assert_eq!(editor.buffer(), "select name");
        assert!(!editor.redo());
    }
}
