use super::*;
use crate::registry::{self, Name, Screen};
use crate::tui::updating::{self, Updating};

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
                    Some((entry, argument)) if entry.works_on(Screen::Connections) => {
                        self.run_named(entry.name, &argument)
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

    pub fn run_named(&mut self, name: Name, argument: &str) -> Flow<Option<Connection>> {
        match name {
            Name::Open => return self.open_typed(argument),
            Name::Update => self.start_updating(),
            Name::Quit => return self.quit(),
            Name::Write => self.write_pending(),
            Name::New => self.create(),
            Name::SignOut => self.sign_out(),
            Name::Version => self.say(registry::version()),
            Name::Settings => {
                self.settings = Some(crate::tui::settings_modal::SettingsModal::open(
                    &crate::paths::config_file(),
                ))
            }
            other => {
                let word = registry::entry_for(other).word();
                self.say(registry::unavailable(Screen::Connections, word));
            }
        }
        Flow::Continue
    }

    fn start_updating(&mut self) {
        if self.updating.is_none() {
            self.updating = Some(Updating::start());
        }
        self.say(updating::CHECKING);
    }

    pub(super) fn hear_from_update(&mut self) -> bool {
        let Some((said, done)) = self.updating.as_mut().and_then(Updating::heard) else {
            return false;
        };
        if done {
            self.updating = None;
        }
        self.say(said);
        true
    }

    fn open_typed(&mut self, typed: &str) -> Flow<Option<Connection>> {
        if typed.trim().is_empty() {
            self.command = Some("open ".to_string());
            return Flow::Continue;
        }
        let Some((store, key)) = self.store.clone() else {
            self.say("there is no connection list to open it into");
            return Flow::Continue;
        };
        match crate::opener::saved(typed, &store, &key) {
            Ok(connection) => Flow::Exit(Some(connection)),
            Err(error) => {
                self.say(format!("{error:#}"));
                Flow::Continue
            }
        }
    }
}
