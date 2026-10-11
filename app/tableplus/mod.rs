pub mod keychain;
pub mod plist;

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, bail};

use self::plist::Value;
use crate::paths;
use crate::store_writer::{Field, Fields, StoreWriter};

pub const DIRECTORIES: [&str; 2] = [
    "~/Library/Application Support/com.tinyapp.TablePlus/Data",
    "~/Library/Application Support/com.tinyapp.TablePlus-setapp/Data",
];

pub const AFTER_MACOS_ASKS: &str = "from Keychain, after macOS asks";

const STORED_IN_KEYCHAIN: i64 = 0;

#[derive(Debug, Clone)]
pub struct Entry {
    pub name: String,
    pub id: String,
    pub driver: String,
    pub group: Option<String>,
    pub tag: Option<String>,
    pub over_ssh: bool,
    pub skip: Option<String>,
    pub password: Option<String>,
    database_password_mode: i64,
    server_password_mode: i64,
    ssh_key: bool,
    attributes: Fields,
}

pub fn find() -> Option<PathBuf> {
    DIRECTORIES
        .iter()
        .map(|directory| PathBuf::from(paths::expand(directory)))
        .find(|directory| directory.join("Connections.plist").is_file())
}

pub fn plan(directory: &Path, writer: &StoreWriter) -> anyhow::Result<Vec<Entry>> {
    let listed = directory.join("Connections.plist");
    if !listed.is_file() {
        bail!(
            "{} has no Connections.plist; point --from at the TablePlus Data folder.",
            directory.display()
        );
    }
    let not_a_list = || {
        anyhow!(
            "{}/Connections.plist does not list any connections.",
            directory.display()
        )
    };
    let entries: Vec<Value> = match plist::read(&listed)? {
        Value::Array(entries) => entries,
        Value::Dictionary(entries) => entries.into_iter().map(|(_, entry)| entry).collect(),
        _ => return Err(not_a_list()),
    };
    let groups = groups(directory)?;

    entries
        .iter()
        .map(|entry| match entry {
            Value::Dictionary(entry) => planned(entry, &groups, writer),
            _ => Err(not_a_list()),
        })
        .collect()
}

pub fn import(
    plan: Vec<Entry>,
    with_passwords: bool,
    writer: &StoreWriter,
) -> anyhow::Result<Vec<Entry>> {
    plan.into_iter()
        .map(|mut entry| {
            if entry.skip.is_some() {
                return Ok(entry);
            }
            let mut fields = entry.attributes.clone();
            if with_passwords {
                fields.extend(passwords(&entry));
            }
            entry.password = Some(password_state(&entry, &fields, with_passwords).to_string());
            let name = writer.free_name(&entry.name)?;
            fields.push(("name", Field::Text(name.clone())));
            writer.create(&fields)?;
            entry.name = name;
            Ok(entry)
        })
        .collect()
}

fn planned(
    entry: &::plist::Dictionary,
    groups: &HashMap<String, String>,
    writer: &StoreWriter,
) -> anyhow::Result<Entry> {
    let field = |key: &str| entry.get(key);
    let table_plus_driver = plist::text(field("Driver"));
    let driver = driver_for(&table_plus_driver);

    let mut planned = Entry {
        name: non_empty(plist::text(field("ConnectionName")))
            .unwrap_or_else(|| "TablePlus connection".to_string()),
        id: plist::text(field("ID")),
        driver: driver.unwrap_or(&table_plus_driver).to_string(),
        group: groups.get(&plist::text(field("GroupID"))).cloned(),
        tag: tag_for(&plist::text(field("Enviroment")).to_lowercase()).map(String::from),
        over_ssh: plist::truthy(field("isOverSSH")),
        skip: None,
        password: None,
        database_password_mode: plist::int(field("DatabasePasswordMode")),
        server_password_mode: plist::int(field("ServerPasswordMode")),
        ssh_key: plist::truthy(field("isUsePrivateKey")),
        attributes: Vec::new(),
    };

    let Some(driver) = driver else {
        planned.skip = Some(format!("tql does not speak {table_plus_driver}"));
        return Ok(planned);
    };

    let attributes = attributes(entry, driver, &table_plus_driver, &planned);
    if let Some(existing) = writer.matching(&identity(&attributes))? {
        planned.skip = Some(format!("already in tql as {}", writer.name_of(existing)?));
        return Ok(planned);
    }
    planned.attributes = attributes;
    Ok(planned)
}

fn attributes(
    entry: &::plist::Dictionary,
    driver: &'static str,
    table_plus_driver: &str,
    planned: &Entry,
) -> Fields {
    let field = |key: &str| entry.get(key);
    let optional = |value: Option<String>| value.map_or(Field::Null, Field::Text);

    if driver == "sqlite" {
        return kept(
            vec![
                ("driver", Field::Text("sqlite".to_string())),
                ("database", Field::Text(plist::text(field("DatabasePath")))),
                ("tag", optional(planned.tag.clone())),
                ("group_name", optional(planned.group.clone())),
            ],
            false,
        );
    }

    let tls_paths: Vec<String> = match field("TlsKeyPaths") {
        Some(Value::Array(paths)) => paths.iter().map(|path| plist::text(Some(path))).collect(),
        Some(Value::Dictionary(paths)) => {
            paths.values().map(|path| plist::text(Some(path))).collect()
        }
        None => Vec::new(),
        single => vec![plist::text(single)],
    };
    let tls = |index: usize| {
        optional(
            tls_paths
                .get(index)
                .filter(|path| !path.is_empty())
                .cloned(),
        )
    };
    let port = match plist::int(field("DatabasePort")) {
        0 => default_port(driver),
        port => port,
    };
    let ssl_mode = tls_modes(table_plus_driver)
        .get(usize::try_from(plist::int(field("tLSMode"))).unwrap_or(usize::MAX))
        .map(|mode| mode.to_string());

    let mut attributes = vec![
        ("driver", Field::Text(driver.to_string())),
        (
            "host",
            Field::Text(
                non_empty(plist::text(field("DatabaseHost")))
                    .unwrap_or_else(|| "127.0.0.1".to_string()),
            ),
        ),
        ("port", Field::Number(port)),
        (
            "database",
            optional(non_empty(plist::text(field("DatabaseName")))),
        ),
        (
            "username",
            optional(non_empty(plist::text(field("DatabaseUser")))),
        ),
        ("tag", optional(planned.tag.clone())),
        ("group_name", optional(planned.group.clone())),
        ("ssl_mode", optional(ssl_mode)),
        ("ssl_key", tls(0)),
        ("ssl_cert", tls(1)),
        ("ssl_ca", tls(2)),
    ];
    if planned.over_ssh {
        attributes.extend([
            (
                "ssh_host",
                optional(non_empty(plist::text(field("ServerAddress")))),
            ),
            (
                "ssh_port",
                match plist::int(field("ServerPort")) {
                    0 => Field::Null,
                    port => Field::Number(port),
                },
            ),
            (
                "ssh_user",
                optional(non_empty(plist::text(field("ServerUser")))),
            ),
            (
                "ssh_key",
                match planned.ssh_key {
                    true => optional(key_path(&plist::text(field("ServerPrivateKeyName")))),
                    false => Field::Null,
                },
            ),
        ]);
    }
    kept(attributes, true)
}

fn kept(attributes: Fields, drop_empty: bool) -> Fields {
    attributes
        .into_iter()
        .filter(|(_, value)| match value {
            Field::Null => false,
            Field::Text(text) => !(drop_empty && text.is_empty()),
            _ => true,
        })
        .collect()
}

fn identity(attributes: &Fields) -> Fields {
    let mut identity: Fields = attributes
        .iter()
        .filter(|(key, _)| {
            ["driver", "host", "port", "database", "username", "ssh_host"].contains(key)
        })
        .cloned()
        .collect();
    if !identity.iter().any(|(key, _)| *key == "ssh_host") {
        identity.push(("ssh_host", Field::Null));
    }
    identity
}

fn key_path(name: &str) -> Option<String> {
    let name = name.trim();
    if name.is_empty() {
        return None;
    }
    if name.starts_with('/') || name.starts_with("~/") {
        return Some(name.to_string());
    }
    let path = format!("~/.ssh/{name}");
    Path::new(&paths::expand(&path)).is_file().then_some(path)
}

fn groups(directory: &Path) -> anyhow::Result<HashMap<String, String>> {
    let path = directory.join("ConnectionGroups.plist");
    if !path.is_file() {
        return Ok(HashMap::new());
    }
    let listed: Vec<Value> = match plist::read(&path)? {
        Value::Array(groups) => groups,
        Value::Dictionary(groups) => groups.into_iter().map(|(_, group)| group).collect(),
        _ => Vec::new(),
    };
    Ok(listed
        .iter()
        .filter_map(Value::as_dictionary)
        .filter_map(|group| {
            let id = group.get("ID")?;
            let name = non_empty(plist::text(group.get("Name")))?;
            Some((plist::text(Some(id)), name))
        })
        .collect())
}

fn passwords(entry: &Entry) -> Fields {
    if entry.id.is_empty() {
        return Vec::new();
    }
    let mut passwords = Vec::new();
    if entry.database_password_mode == STORED_IN_KEYCHAIN
        && entry.driver != "sqlite"
        && let Some(password) = keychain::password(&format!("{}_database", entry.id))
    {
        passwords.push(("password", Field::Secret(password)));
    }
    if entry.over_ssh
        && !entry.ssh_key
        && entry.server_password_mode == STORED_IN_KEYCHAIN
        && let Some(password) = keychain::password(&format!("{}_server", entry.id))
    {
        passwords.push(("ssh_password", Field::Secret(password)));
    }
    passwords
}

fn password_state(entry: &Entry, fields: &Fields, with_passwords: bool) -> &'static str {
    if entry.driver == "sqlite" {
        return "not needed";
    }
    if entry.database_password_mode != STORED_IN_KEYCHAIN {
        return "TablePlus asks for it each time; add it with e";
    }
    if !with_passwords {
        return "left out; add it with e";
    }
    match fields.iter().any(|(key, _)| *key == "password") {
        true => "from Keychain",
        false => "not in Keychain, or not allowed; add it with e",
    }
}

fn driver_for(table_plus_driver: &str) -> Option<&'static str> {
    match table_plus_driver {
        "MySQL" | "MariaDB" => Some("mysql"),
        "PostgreSQL" => Some("pgsql"),
        "SQLite" => Some("sqlite"),
        "SQLServer" => Some("sqlsrv"),
        _ => None,
    }
}

fn tag_for(environment: &str) -> Option<&'static str> {
    match environment {
        "production" => Some("production"),
        "staging" => Some("staging"),
        "development" | "testing" => Some("dev"),
        "local" => Some("local"),
        _ => None,
    }
}

fn default_port(driver: &str) -> i64 {
    match driver {
        "mysql" => 3306,
        "pgsql" => 5432,
        _ => 1433,
    }
}

fn tls_modes(table_plus_driver: &str) -> &'static [&'static str] {
    match table_plus_driver {
        "MySQL" => &["", "disable", "require", "verify-ca", "verify-full"],
        "MariaDB" => &["", "require", "verify-full"],
        "PostgreSQL" => &["", "disable", "require", "", "verify-ca", "verify-full"],
        _ => &[],
    }
}

fn non_empty(text: String) -> Option<String> {
    let trimmed = text.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_string())
}
