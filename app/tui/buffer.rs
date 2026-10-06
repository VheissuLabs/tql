use zerorust_tui::Key;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TextBuffer {
    characters: Vec<char>,
    cursor: usize,
}

impl TextBuffer {
    pub fn holding(text: &str) -> Self {
        let mut buffer = TextBuffer::default();
        buffer.set(text);
        buffer
    }

    pub fn text(&self) -> String {
        self.characters.iter().collect()
    }

    pub fn cursor(&self) -> usize {
        self.cursor
    }

    pub fn set(&mut self, text: &str) {
        self.characters = text.chars().collect();
        self.cursor = self.characters.len();
    }

    pub fn jump_to_start(&mut self) {
        self.cursor = 0;
    }

    pub fn jump_to_end(&mut self) {
        self.cursor = self.characters.len();
    }

    pub fn lines(&self) -> Vec<String> {
        self.text().split('\n').map(String::from).collect()
    }

    pub fn cursor_line(&self) -> usize {
        self.characters[..self.cursor]
            .iter()
            .filter(|character| **character == '\n')
            .count()
    }

    pub fn cursor_column(&self) -> usize {
        self.characters[..self.cursor]
            .iter()
            .rev()
            .take_while(|character| **character != '\n')
            .count()
    }

    pub fn jump_to_line(&mut self, line: usize) {
        let lines = self.lines();
        let line = line.min(lines.len().saturating_sub(1));
        self.cursor = lines[..line]
            .iter()
            .map(|text| text.chars().count() + 1)
            .sum();
    }

    pub fn handle(&mut self, key: Key) {
        match key {
            Key::Enter | Key::ShiftEnter => self.insert("\n"),
            Key::Backspace | Key::Ctrl('h') => self.backspace(),
            Key::Delete => {
                if self.cursor < self.characters.len() {
                    self.characters.remove(self.cursor);
                }
            }
            Key::Left | Key::Ctrl('b') => self.cursor = self.cursor.saturating_sub(1),
            Key::Right | Key::Ctrl('f') => {
                self.cursor = (self.cursor + 1).min(self.characters.len())
            }
            Key::Up => self.move_line(false),
            Key::Down => self.move_line(true),
            Key::Home | Key::Ctrl('a') => self.cursor -= self.cursor_column(),
            Key::End | Key::Ctrl('e') => self.jump_to_line_end(),
            Key::Tab => self.insert("  "),
            Key::BackTab => self.outdent(),
            Key::Char(character) => self.insert(&character.to_string()),
            _ => {}
        }
    }

    fn insert(&mut self, text: &str) {
        for character in text.chars() {
            self.characters.insert(self.cursor, character);
            self.cursor += 1;
        }
    }

    fn backspace(&mut self) {
        if self.cursor == 0 {
            return;
        }
        self.cursor -= 1;
        self.characters.remove(self.cursor);
    }

    fn move_line(&mut self, down: bool) {
        let lines = self.lines();
        let current = self.cursor_line();
        let target = if down {
            current + 1
        } else {
            let Some(above) = current.checked_sub(1) else {
                return;
            };
            above
        };
        if target >= lines.len() {
            return;
        }
        let column = self.cursor_column().min(lines[target].chars().count());
        self.jump_to_line(target);
        self.cursor += column;
    }

    fn jump_to_line_end(&mut self) {
        let length = self.lines()[self.cursor_line()].chars().count();
        self.cursor += length - self.cursor_column();
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
}

#[cfg(test)]
mod tests {
    use super::*;

    fn typed(buffer: &mut TextBuffer, keys: &[Key]) {
        for key in keys {
            buffer.handle(*key);
        }
    }

    #[test]
    fn opens_with_the_cursor_after_the_text() {
        let buffer = TextBuffer::holding("beta");

        assert_eq!(buffer.cursor(), 4);
    }

    #[test]
    fn types_and_deletes_at_the_cursor() {
        let mut buffer = TextBuffer::holding("bta");
        typed(
            &mut buffer,
            &[
                Key::Left,
                Key::Left,
                Key::Char('e'),
                Key::End,
                Key::Backspace,
            ],
        );

        assert_eq!(buffer.text(), "bet");
    }

    #[test]
    fn moves_between_lines_keeping_the_column_where_it_can() {
        let mut buffer = TextBuffer::holding("first line\nab\nthird");
        buffer.jump_to_start();
        typed(&mut buffer, &[Key::End, Key::Down]);

        assert_eq!((buffer.cursor_line(), buffer.cursor_column()), (1, 2));

        typed(&mut buffer, &[Key::Down, Key::Up, Key::Up]);

        assert_eq!((buffer.cursor_line(), buffer.cursor_column()), (0, 2));
    }

    #[test]
    fn jumps_to_a_line_and_clamps_past_the_end() {
        let mut buffer = TextBuffer::holding("a\nb\nc");
        buffer.jump_to_line(1);

        assert_eq!(buffer.cursor_line(), 1);

        buffer.jump_to_line(40);

        assert_eq!(buffer.cursor_line(), 2);
    }

    #[test]
    fn enter_breaks_the_line_and_tab_indents() {
        let mut buffer = TextBuffer::holding("ab");
        typed(&mut buffer, &[Key::Left, Key::Enter, Key::Tab]);

        assert_eq!(buffer.text(), "a\n  b");

        buffer.handle(Key::BackTab);

        assert_eq!(buffer.text(), "a\nb");
    }
}
