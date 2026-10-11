use super::*;
use crate::registry::{self, Name, Screen};

impl Picker {
    pub(super) fn type_into_command(&mut self, key: Key) -> Flow<Option<Connection>> {
        let Some(command) = self.command.as_mut() else {
            return Flow::Continue;
        };
        match key {
            Key::Escape => self.command = None,
            Key::Enter => {
                let command = command.trim().to_string();
                self.command = None;
                return match registry::parse(&command) {
                    Some((entry, _)) if entry.works_on(Screen::Connections) => {
                        self.run_named(entry.name)
                    }
                    _ => {
                        self.say(registry::unavailable(Screen::Connections, &command));
                        Flow::Continue
                    }
                };
            }
            Key::Backspace if command.is_empty() => self.command = None,
            Key::Backspace => {
                command.pop();
            }
            Key::Char(character) => command.push(character),
            _ => {}
        }
        Flow::Continue
    }

    pub fn run_named(&mut self, name: Name) -> Flow<Option<Connection>> {
        match name {
            Name::Quit => return self.quit(),
            Name::Write => self.write_pending(),
            Name::New => self.create(),
            other => {
                let word = registry::entry_for(other).word();
                self.say(registry::unavailable(Screen::Connections, word));
            }
        }
        Flow::Continue
    }
}
