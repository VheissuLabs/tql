use std::collections::HashMap;
use std::env;
use std::path::PathBuf;

use crate::database::snowflake::Authenticator;
use crate::driver::Driver;
use crate::terminal::prelude::*;
use ratatui::buffer::Buffer;

use super::choice::Choice;
use super::chrome::{Island, theme};
use super::filters::LineEditor;
use super::key_files;
use super::text::{pad, single_line, truncate, width};
use crate::encrypter::Encrypter;
use crate::paths;
use crate::store::Connection;
use crate::store_writer::{Field, Fields, StoreWriter};
use crate::tag;

mod input;
mod render;
mod save;

pub const DRIVERS: [&str; 9] = [
    Driver::Sqlite.name(),
    Driver::Mysql.name(),
    Driver::Pgsql.name(),
    Driver::Sqlsrv.name(),
    Driver::Duckdb.name(),
    Driver::Libsql.name(),
    Driver::Clickhouse.name(),
    Driver::Snowflake.name(),
    Driver::Redis.name(),
];
pub const SSL_MODES: [&str; 6] = [
    "",
    "disable",
    "prefer",
    "require",
    "verify-ca",
    "verify-full",
];
pub const TYPE_IT: &str = "type a path…";
pub const NO_GROUP: &str = "none";

const YES_NO: [&str; 2] = ["no", "yes"];
const VALUE_WIDTH: usize = 34;
const PICKER_WIDTH: u16 = 52;
const SSH_FIELDS: [(&str, &str); 5] = [
    ("ssh_host", "Server"),
    ("ssh_port", "Port"),
    ("ssh_user", "User"),
    ("ssh_key", "Key"),
    ("ssh_password", "Password"),
];
const FILES: [&str; 5] = ["ssh_key", "ssl_ca", "ssl_cert", "ssl_key", "private_key"];
const SECRETS: [&str; 2] = ["password", "ssh_password"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Places {
    pub home: PathBuf,
    pub cwd: PathBuf,
}

impl Places {
    pub fn here() -> Self {
        Places {
            home: paths::home(),
            cwd: env::current_dir().unwrap_or_default(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    Stay,
    Cancel,
    Save,
}

pub struct ConnectionForm {
    pub id: Option<i64>,
    pub creating: bool,
    pub index: usize,
    pub editor: Option<LineEditor>,
    pub values: HashMap<&'static str, String>,
    pub error: Option<String>,
    pub picker: Option<Choice>,
    unreadable: Vec<&'static str>,
    places: Places,
    groups: Vec<String>,
}

impl ConnectionForm {
    pub fn creating(places: Places) -> Self {
        let mut form = ConnectionForm::blank(places);
        form.creating = true;
        form.values
            .insert("driver", Driver::Sqlite.name().to_string());
        form
    }

    pub fn editing(connection: &Connection, encrypter: Option<&Encrypter>, places: Places) -> Self {
        let mut form = ConnectionForm::blank(places);
        form.id = Some(connection.id);
        let text = |value: &Option<String>| value.clone().unwrap_or_default();
        let number = |value: Option<u16>| value.map(|port| port.to_string()).unwrap_or_default();
        for (key, value) in [
            ("name", connection.name.clone()),
            ("database", text(&connection.database)),
            ("host", text(&connection.host)),
            ("port", number(connection.port)),
            ("username", text(&connection.username)),
            ("ssl_mode", text(&connection.ssl_mode)),
            ("ssl_ca", text(&connection.ssl_ca)),
            ("ssl_cert", text(&connection.ssl_cert)),
            ("ssl_key", text(&connection.ssl_key)),
            ("group_name", text(&connection.group_name)),
            ("tag", text(&connection.tag)),
            ("ssh_host", text(&connection.ssh_host)),
            ("ssh_port", number(connection.ssh_port)),
            ("ssh_user", text(&connection.ssh_user)),
            ("ssh_key", text(&connection.ssh_key)),
            ("warehouse", text(&connection.warehouse)),
            ("role", text(&connection.role)),
            ("private_key", text(&connection.private_key)),
            (
                "authenticator",
                Authenticator::parse(connection.authenticator.as_deref())
                    .label()
                    .to_string(),
            ),
        ] {
            form.values.insert(key, value);
        }
        for (key, payload) in [
            ("password", &connection.password),
            ("ssh_password", &connection.ssh_password),
        ] {
            let Some(payload) = payload else {
                continue;
            };
            match encrypter.map(|encrypter| encrypter.decrypt_string(payload)) {
                Some(Ok(plain)) => {
                    form.values.insert(key, plain);
                }
                _ => form.unreadable.push(key),
            }
        }
        form.values
            .insert("driver", connection.driver.name().to_string());
        form.values
            .insert("read_only", yes_or_no(connection.read_only));
        form.values
            .insert("over_ssh", yes_or_no(connection.uses_ssh()));
        form
    }

    fn blank(places: Places) -> Self {
        ConnectionForm {
            id: None,
            creating: false,
            index: 0,
            editor: None,
            values: HashMap::from([
                ("read_only", "no".to_string()),
                ("over_ssh", "no".to_string()),
                ("authenticator", Authenticator::Token.label().to_string()),
            ]),
            error: None,
            picker: None,
            unreadable: Vec::new(),
            places,
            groups: Vec::new(),
        }
    }

    pub fn with_groups(mut self, groups: Vec<String>) -> Self {
        self.groups = groups;
        self
    }

    pub fn choice_options(&self) -> Vec<String> {
        self.picker
            .as_ref()
            .map(|picker| picker.options.clone())
            .unwrap_or_default()
    }

    pub fn value(&self, key: &str) -> &str {
        self.values.get(key).map(String::as_str).unwrap_or_default()
    }

    fn set(&mut self, key: &'static str, value: String) {
        self.values.insert(key, value);
    }

    pub fn driver(&self) -> Driver {
        Driver::parse(self.value("driver")).unwrap_or_default()
    }

    pub fn authenticator(&self) -> Authenticator {
        Authenticator::ALL
            .into_iter()
            .find(|authenticator| authenticator.label() == self.value("authenticator"))
            .unwrap_or_default()
    }

    pub fn over_ssh(&self) -> bool {
        self.value("over_ssh") == "yes"
    }

    pub fn editing_field(&self) -> bool {
        self.editor.is_some()
    }

    pub fn fields(&self) -> Vec<(&'static str, &'static str)> {
        let mut fields = Vec::new();
        if self.creating {
            fields.push(("driver", "Driver"));
        }
        fields.push(("name", "Name"));
        if self.driver().is_file() {
            fields.push(("database", "Path"));
        } else if self.driver() == Driver::Libsql {
            fields.extend([("host", "URL"), ("password", "Auth token")]);
        } else if self.driver() == Driver::Snowflake {
            fields.extend([
                ("host", "Account"),
                ("username", "User"),
                ("database", "Database"),
                ("authenticator", "Sign in with"),
            ]);
            match self.authenticator() {
                Authenticator::Token => fields.push(("password", "Access token")),
                Authenticator::KeyPair => {
                    fields.extend([("private_key", "Private key"), ("password", "Passphrase")])
                }
                Authenticator::Browser => {}
            }
            fields.extend([("warehouse", "Warehouse"), ("role", "Role")]);
        } else {
            fields.extend([
                ("host", "Host"),
                ("port", "Port"),
                ("database", "Database"),
                ("username", "Username"),
                ("password", "Password"),
                ("ssl_mode", "SSL mode"),
            ]);
            if !self.value("ssl_mode").trim().is_empty() {
                fields.extend([
                    ("ssl_ca", "SSL CA cert"),
                    ("ssl_cert", "SSL cert"),
                    ("ssl_key", "SSL key"),
                ]);
            }
            fields.push(("over_ssh", "Over SSH"));
            if self.over_ssh() {
                fields.extend(SSH_FIELDS);
            }
        }
        fields.extend([
            ("group_name", "Group"),
            ("tag", "Tag"),
            ("read_only", "Read only"),
        ]);
        fields
    }

    pub fn keys(&self) -> Vec<&'static str> {
        self.fields().into_iter().map(|(key, _)| key).collect()
    }

    pub fn current_key(&self) -> &'static str {
        self.keys().get(self.index).copied().unwrap_or("name")
    }

    pub fn choices(&self, key: &str) -> Option<Vec<&'static str>> {
        match key {
            "driver" => Some(DRIVERS.to_vec()),
            "ssl_mode" => Some(SSL_MODES.to_vec()),
            "read_only" | "over_ssh" => Some(YES_NO.to_vec()),
            "authenticator" => Some(
                Authenticator::ALL
                    .iter()
                    .map(|authenticator| authenticator.label())
                    .collect(),
            ),
            _ => None,
        }
    }

    fn picked(key: &str) -> bool {
        FILES.contains(&key) || key == "tag" || key == "group_name"
    }

    pub fn move_by(&mut self, steps: isize) {
        let last = self.keys().len().saturating_sub(1);
        self.index = self.index.saturating_add_signed(steps).min(last);
    }

    pub fn cycle(&mut self, forward: bool) {
        let key = self.current_key();
        let Some(choices) = self.choices(key) else {
            return;
        };
        if key == "driver" {
            self.cycle_driver(forward);
            return;
        }
        let next = stepped(&choices, self.value(key), forward);
        self.set(key, next.to_string());
        if key == "authenticator" {
            self.set("password", String::new());
            self.unreadable.retain(|secret| *secret != "password");
        }
        if key == "ssl_mode" {
            self.follow_tls_port();
        }
    }

    fn follow_tls_port(&mut self) {
        if self.driver() != Driver::Clickhouse {
            return;
        }
        let tls = !matches!(self.value("ssl_mode").trim(), "" | "disable");
        match (tls, self.value("port")) {
            (true, "8123") => self.set("port", "8443".to_string()),
            (false, "8443") => self.set("port", "8123".to_string()),
            _ => {}
        }
    }

    pub fn cycle_driver(&mut self, forward: bool) {
        let was = default_port(self.driver()).to_string();
        let next = stepped(&DRIVERS, self.driver().name(), forward);
        self.set("driver", next.to_string());
        if self.value("port") == was {
            self.set("port", String::new());
        }
        self.apply_driver_defaults();
        self.index = self.index.min(self.keys().len().saturating_sub(1));
    }

    fn apply_driver_defaults(&mut self) {
        if self.driver().is_file() {
            return;
        }
        if matches!(self.driver(), Driver::Libsql | Driver::Snowflake) {
            if self.value("host") == "127.0.0.1" {
                self.set("host", String::new());
            }
            return;
        }
        if self.value("host").is_empty() {
            self.set("host", "127.0.0.1".to_string());
        }
        if self.value("port").is_empty() {
            self.set("port", default_port(self.driver()).to_string());
        }
    }

    pub fn group(key: &str) -> &'static str {
        match key {
            "driver" | "name" => "",
            "over_ssh" => "ssh",
            "group_name" | "tag" | "read_only" => "labels",
            ssl if ssl.starts_with("ssl_") => "tls",
            ssh if ssh.starts_with("ssh_") => "ssh",
            _ => "where",
        }
    }

    fn starts_group(&self, key: &str) -> bool {
        let keys = self.keys();
        let Some(position) = keys.iter().position(|field| *field == key) else {
            return false;
        };
        position == 0 || Self::group(keys[position - 1]) != Self::group(key)
    }

    pub fn display(&self, key: &str) -> String {
        let value = self.value(key);
        if key == self.current_key()
            && let Some(editor) = &self.editor
        {
            return editor.text();
        }
        match key {
            "ssh_port" if value.is_empty() => "22".to_string(),
            "ssh_key" if value.is_empty() => "agent or ~/.ssh/config".to_string(),
            "ssl_mode" if value.is_empty() => "driver default".to_string(),
            "tag" if value.is_empty() => tag::NONE.to_string(),
            "group_name" if value.is_empty() => NO_GROUP.to_string(),
            "host" if value.is_empty() && self.driver() == Driver::Snowflake => {
                "myorg-myaccount".to_string()
            }
            "warehouse" | "role" if value.is_empty() => "the user's default".to_string(),
            "username" if value.is_empty() && self.driver() == Driver::Snowflake => {
                match self.authenticator() {
                    Authenticator::Token => "the token's user".to_string(),
                    _ => "your Snowflake login name".to_string(),
                }
            }
            "private_key" if value.is_empty() => "~/.snowflake/rsa_key.p8".to_string(),
            "password"
                if value.is_empty()
                    && self.driver() == Driver::Snowflake
                    && self.authenticator() == Authenticator::KeyPair =>
            {
                "none, the key is not encrypted".to_string()
            }
            "database" if value.is_empty() && self.driver().is_server() => {
                "ask on connect".to_string()
            }
            "over_ssh" => yes_or_no(self.over_ssh()),
            "read_only" => yes_or_no(matches!(value, "1" | "yes")),
            secret if SECRETS.contains(&secret) && self.unreadable.contains(&secret) => {
                "•".repeat(8)
            }
            secret if SECRETS.contains(&secret) => "•".repeat(value.chars().count().min(8)),
            _ => value.to_string(),
        }
    }

    pub fn is_placeholder(&self, key: &str) -> bool {
        self.value(key).is_empty()
            && !["over_ssh", "read_only", "tag", "group_name"].contains(&key)
            && (!SECRETS.contains(&key) || !self.unreadable.contains(&key))
            && !self.display(key).is_empty()
    }

    pub fn name(&self) -> &str {
        self.value("name")
    }
}

fn stepped<'a>(choices: &[&'a str], current: &str, forward: bool) -> &'a str {
    let at = choices
        .iter()
        .position(|choice| *choice == current)
        .unwrap_or(0);
    let count = choices.len();
    let next = if forward {
        (at + 1) % count
    } else {
        (at + count - 1) % count
    };
    choices[next]
}

fn default_port(driver: Driver) -> u16 {
    driver.default_port().unwrap_or_default()
}

fn yes_or_no(flag: bool) -> String {
    YES_NO[usize::from(flag)].to_string()
}
