use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Key {
    Char(char),
    Ctrl(char),
    Alt(char),
    Up,
    Down,
    Left,
    Right,
    Enter,
    ShiftEnter,
    Escape,
    Tab,
    BackTab,
    Backspace,
    Delete,
    Home,
    End,
    PageUp,
    PageDown,
}

impl Key {
    pub fn from_event(event: KeyEvent) -> Option<Key> {
        let control = event.modifiers.contains(KeyModifiers::CONTROL);
        let alt = event.modifiers.contains(KeyModifiers::ALT);
        let shift = event.modifiers.contains(KeyModifiers::SHIFT);

        Some(match event.code {
            KeyCode::Char(character) if control => Key::Ctrl(character.to_ascii_lowercase()),
            KeyCode::Char(character) if alt => Key::Alt(character),
            KeyCode::Char(character) => Key::Char(character),
            KeyCode::Enter if shift => Key::ShiftEnter,
            KeyCode::Enter => Key::Enter,
            KeyCode::Esc => Key::Escape,
            KeyCode::Tab => Key::Tab,
            KeyCode::BackTab => Key::BackTab,
            KeyCode::Backspace => Key::Backspace,
            KeyCode::Delete => Key::Delete,
            KeyCode::Up => Key::Up,
            KeyCode::Down => Key::Down,
            KeyCode::Left => Key::Left,
            KeyCode::Right => Key::Right,
            KeyCode::Home => Key::Home,
            KeyCode::End => Key::End,
            KeyCode::PageUp => Key::PageUp,
            KeyCode::PageDown => Key::PageDown,
            _ => return None,
        })
    }

    pub fn parse(bytes: &str) -> Vec<Key> {
        const SEQUENCES: [(&str, Key); 17] = [
            ("\x1b[A", Key::Up),
            ("\x1bOA", Key::Up),
            ("\x1b[B", Key::Down),
            ("\x1bOB", Key::Down),
            ("\x1b[C", Key::Right),
            ("\x1bOC", Key::Right),
            ("\x1b[D", Key::Left),
            ("\x1bOD", Key::Left),
            ("\x1b[Z", Key::BackTab),
            ("\x1b[H", Key::Home),
            ("\x1b[F", Key::End),
            ("\x1b[3~", Key::Delete),
            ("\x1b[5~", Key::PageUp),
            ("\x1b[6~", Key::PageDown),
            ("\x1b[13;2u", Key::ShiftEnter),
            ("\x1b\r", Key::ShiftEnter),
            ("\x1b\n", Key::ShiftEnter),
        ];

        let mut keys = Vec::new();
        let mut rest = bytes;

        while let Some(first) = rest.chars().next() {
            if let Some((sequence, key)) = SEQUENCES
                .iter()
                .find(|(sequence, _)| rest.starts_with(sequence))
            {
                keys.push(*key);
                rest = &rest[sequence.len()..];
                continue;
            }

            rest = &rest[first.len_utf8()..];
            let key = match first {
                '\x1b' => match rest.chars().next() {
                    Some(next) if next != '\x1b' => {
                        rest = &rest[next.len_utf8()..];
                        Key::Alt(next)
                    }
                    _ => Key::Escape,
                },
                '\n' | '\r' => Key::Enter,
                '\t' => Key::Tab,
                '\x7f' | '\x08' => Key::Backspace,
                control @ '\x01'..='\x1a' => Key::Ctrl((b'a' + control as u8 - 1) as char),
                character => Key::Char(character),
            };
            keys.push(key);
        }

        keys
    }

    pub fn glyph(&self) -> String {
        match self {
            Key::Char(' ') => "space".to_string(),
            Key::Char(character) => character.to_string(),
            Key::Ctrl(character) => format!("ctrl+{character}"),
            Key::Alt(character) => format!("alt+{character}"),
            Key::Up => "↑".to_string(),
            Key::Down => "↓".to_string(),
            Key::Left => "←".to_string(),
            Key::Right => "→".to_string(),
            Key::Enter => "↵".to_string(),
            Key::ShiftEnter => "⇧↵".to_string(),
            Key::Escape => "esc".to_string(),
            Key::Tab => "tab".to_string(),
            Key::BackTab => "⇧tab".to_string(),
            Key::Backspace => "⌫".to_string(),
            Key::Delete => "del".to_string(),
            Key::Home => "home".to_string(),
            Key::End => "end".to_string(),
            Key::PageUp => "pgup".to_string(),
            Key::PageDown => "pgdn".to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Key;

    #[test]
    fn parses_raw_terminal_bytes_like_tqls_tests_send_them() {
        assert_eq!(
            Key::parse("jk\x1b[A\x1bOB\t\x1b[Z\n\x1b\x1b1\x0b?"),
            [
                Key::Char('j'),
                Key::Char('k'),
                Key::Up,
                Key::Down,
                Key::Tab,
                Key::BackTab,
                Key::Enter,
                Key::Escape,
                Key::Alt('1'),
                Key::Ctrl('k'),
                Key::Char('?'),
            ]
        );
    }

    #[test]
    fn escape_before_a_new_line_is_shift_enter() {
        assert_eq!(
            Key::parse("a\x1b\nb\x1b\r"),
            [
                Key::Char('a'),
                Key::ShiftEnter,
                Key::Char('b'),
                Key::ShiftEnter
            ]
        );
    }

    #[test]
    fn a_lone_escape_at_the_end_is_escape() {
        assert_eq!(
            Key::parse(":q\x1b"),
            [Key::Char(':'), Key::Char('q'), Key::Escape]
        );
    }
}
