use super::*;

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
                match command.as_str() {
                    "q" | "q!" | "quit" => return self.quit(),
                    "w" | "write" => self.write_pending(),
                    "new" => self.create(),
                    other => self.say(format!("unknown command: :{other}")),
                }
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
}
