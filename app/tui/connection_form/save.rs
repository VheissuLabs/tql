use super::*;

impl ConnectionForm {
    pub fn save(&mut self, writer: &StoreWriter) -> anyhow::Result<Option<String>> {
        if self.value("name").trim().is_empty() {
            return Ok(Some("A name is required.".to_string()));
        }
        let sqlite = self.driver().is_file();
        if sqlite && self.value("database").trim().is_empty() {
            return Ok(Some(match self.driver() {
                Driver::Duckdb => "A path to the .duckdb file is required.".to_string(),
                _ => "A path to the .sqlite file is required.".to_string(),
            }));
        }
        if self.driver() == Driver::Libsql && self.value("host").trim().is_empty() {
            return Ok(Some("A libsql:// or https:// URL is required.".to_string()));
        }
        if self.driver() == Driver::Snowflake && self.value("host").trim().is_empty() {
            return Ok(Some(
                "A Snowflake account, such as myorg-myaccount, is required.".to_string(),
            ));
        }
        if self.driver() == Driver::Snowflake
            && let Some(missing) = self.missing_for_snowflake()
        {
            return Ok(Some(missing.to_string()));
        }
        if sqlite {
            let resolved =
                paths::resolve(self.value("database"), &self.places.home, &self.places.cwd);
            self.set("database", resolved);
        }
        if writer.name_taken(self.value("name"), self.id)? {
            return Ok(Some(
                "Another connection is already called that.".to_string(),
            ));
        }

        let fields = self.fields_to_save();
        match self.id {
            Some(id) => {
                writer.update(id, &fields)?;
            }
            None => self.id = Some(writer.create(&fields)?),
        }
        Ok(None)
    }

    fn missing_for_snowflake(&self) -> Option<&'static str> {
        let blank = |key: &str| self.value(key).trim().is_empty();
        match self.authenticator() {
            Authenticator::Token if blank("password") && !self.unreadable.contains(&"password") => {
                Some("A programmatic access token is required.")
            }
            Authenticator::Token | Authenticator::OAuth => None,
            _ if blank("username") => Some("The Snowflake user's login name is required."),
            Authenticator::KeyPair if blank("private_key") => {
                Some("The path to the user's private key is required.")
            }
            _ => None,
        }
    }

    fn fields_to_save(&self) -> Fields {
        let over_ssh = self.over_ssh();
        let mut fields: Fields = vec![("driver", Field::Text(self.driver().name().to_string()))];
        for key in self.keys() {
            if key == "driver" || key == "over_ssh" {
                continue;
            }
            if self.unreadable.contains(&key) {
                continue;
            }
            let value = self.value(key);
            let field = match key {
                "port" | "ssh_port" => value
                    .trim()
                    .parse()
                    .map(Field::Number)
                    .unwrap_or(Field::Null),
                "read_only" => Field::Flag(matches!(value, "yes" | "1")),
                "group_name" if value.trim().is_empty() => Field::Null,
                "group_name" => Field::Text(value.trim().to_string()),
                "authenticator" => self
                    .authenticator()
                    .stored()
                    .map_or(Field::Null, |stored| Field::Text(stored.to_string())),
                secret if SECRETS.contains(&secret) && value.is_empty() => Field::Null,
                secret if SECRETS.contains(&secret) => Field::Secret(value.to_string()),
                _ => Field::Text(value.to_string()),
            };
            fields.push((key, field));
        }
        if self.driver() == Driver::Snowflake && !self.authenticator().signs_in_with_browser() {
            fields.push(("sign_in", Field::Null));
        }
        if !over_ssh {
            fields.retain(|(key, _)| !key.starts_with("ssh_"));
            fields.extend(SSH_FIELDS.iter().map(|(key, _)| (*key, Field::Null)));
        }
        fields
    }
}
