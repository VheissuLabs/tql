use super::*;

impl Picker {
    pub(super) fn yank_database(&mut self) {
        let Some(connection) = self.current() else {
            return;
        };
        let database = connection.database.clone().unwrap_or_default();
        if database.is_empty() {
            let status = format!(
                "{} has no database set, so it asks on connect",
                connection.name
            );
            self.say(status);
            return;
        }
        self.yanked(&database, &database);
    }

    pub(super) fn yank_connection_string(&mut self) {
        let Some(connection) = self.current() else {
            return;
        };
        let password = connection.password.as_ref().and_then(|payload| {
            let (_, key) = self.store.as_ref()?;
            Encrypter::from_file(key).ok()?.decrypt_string(payload).ok()
        });
        let text = connection.connection_string(password.as_deref());
        self.yanked(&text, "connection string");
    }

    pub(super) fn yanked(&mut self, text: &str, what: &str) {
        let reached = self.clipboard.copy(text);
        let place = match reached {
            Reached::Terminal => " to the terminal clipboard",
            Reached::System => "",
        };
        self.say(format!("yanked {what}{place}"));
    }
}
