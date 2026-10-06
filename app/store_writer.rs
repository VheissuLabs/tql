use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, bail};
use rusqlite::types::{Value, ValueRef};

use crate::encrypter::Encrypter;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Field {
    Text(String),
    Number(i64),
    Flag(bool),
    Secret(String),
    Null,
}

pub type Fields = Vec<(&'static str, Field)>;

pub struct StoreWriter {
    connection: rusqlite::Connection,
    key: PathBuf,
}

const SCHEMA: &str = include_str!("store_schema.sql");

impl StoreWriter {
    pub fn prepare(store: &Path) -> anyhow::Result<()> {
        if store.exists() {
            return Ok(());
        }
        if let Some(directory) = store.parent().filter(|directory| !directory.exists()) {
            fs::create_dir_all(directory)?;
            restrict(directory, 0o700)?;
        }
        rusqlite::Connection::open(store)
            .with_context(|| format!("cannot create {}", store.display()))?
            .execute_batch(SCHEMA)?;
        restrict(store, 0o600)
    }

    pub fn open(store: &Path, key: &Path) -> anyhow::Result<Self> {
        if !store.exists() {
            bail!(
                "no tql store at {}; add a connection with tql first",
                store.display()
            );
        }
        let connection = rusqlite::Connection::open(store)
            .with_context(|| format!("cannot open {}", store.display()))?;
        connection.pragma_update(None, "foreign_keys", true)?;
        Ok(StoreWriter {
            connection,
            key: key.to_path_buf(),
        })
    }

    pub fn create(&self, fields: &[(&'static str, Field)]) -> anyhow::Result<i64> {
        let now = now();
        let mut columns: Vec<&str> = Vec::new();
        let mut values: Vec<Value> = Vec::new();
        for (column, field) in fields {
            columns.push(column);
            values.push(self.stored(field)?);
        }
        columns.extend(["updated_at", "created_at"]);
        values.extend([Value::Text(now.clone()), Value::Text(now)]);

        let placeholders: Vec<String> = (1..=columns.len())
            .map(|position| format!("?{position}"))
            .collect();
        self.connection.execute(
            &format!(
                "insert into connections ({}) values ({})",
                quoted(&columns).join(", "),
                placeholders.join(", ")
            ),
            rusqlite::params_from_iter(values),
        )?;
        Ok(self.connection.last_insert_rowid())
    }

    pub fn update(&self, id: i64, fields: &[(&'static str, Field)]) -> anyhow::Result<bool> {
        let original = self.raw(id)?;
        let mut columns: Vec<&str> = Vec::new();
        let mut values: Vec<Value> = Vec::new();
        for (column, field) in fields {
            if !self.changed(original.get(*column).cloned().flatten(), field) {
                continue;
            }
            columns.push(column);
            values.push(self.stored(field)?);
        }
        if columns.is_empty() {
            return Ok(false);
        }
        columns.push("updated_at");
        values.push(Value::Text(now()));

        let assignments: Vec<String> = quoted(&columns)
            .iter()
            .enumerate()
            .map(|(position, column)| format!("{column} = ?{}", position + 1))
            .collect();
        values.push(Value::Integer(id));
        self.connection.execute(
            &format!(
                "update connections set {} where id = ?{}",
                assignments.join(", "),
                values.len()
            ),
            rusqlite::params_from_iter(values),
        )?;
        Ok(true)
    }

    pub fn delete(&self, ids: &[i64]) -> anyhow::Result<usize> {
        if ids.is_empty() {
            return Ok(0);
        }
        let placeholders: Vec<String> = (1..=ids.len())
            .map(|position| format!("?{position}"))
            .collect();
        Ok(self.connection.execute(
            &format!(
                "delete from connections where id in ({})",
                placeholders.join(", ")
            ),
            rusqlite::params_from_iter(ids),
        )?)
    }

    pub fn touch(&self, id: i64) -> anyhow::Result<()> {
        self.update(id, &[("last_used_at", Field::Text(now()))])?;
        Ok(())
    }

    pub fn name_taken(&self, name: &str, except: Option<i64>) -> anyhow::Result<bool> {
        Ok(self.connection.query_row(
            "select exists(select 1 from connections where name = ?1 and id is not ?2)",
            (name, except),
            |row| row.get(0),
        )?)
    }

    pub fn free_name(&self, name: &str) -> anyhow::Result<String> {
        if !self.name_taken(name, None)? {
            return Ok(name.to_string());
        }
        for suffix in 2..1000 {
            let numbered = format!("{name} ({suffix})");
            if !self.name_taken(&numbered, None)? {
                return Ok(numbered);
            }
        }
        Ok(format!("{name} ({})", now().replace([' ', ':', '-'], "")))
    }

    pub fn matching(&self, identity: &[(&'static str, Field)]) -> anyhow::Result<Option<i64>> {
        let present: Vec<&(&str, Field)> = identity
            .iter()
            .filter(|(_, field)| *field != Field::Null)
            .collect();
        let mut values: Vec<Value> = Vec::new();
        let mut conditions: Vec<String> = Vec::new();
        for (column, field) in present {
            values.push(self.stored(field)?);
            conditions.push(format!("\"{column}\" = ?{}", values.len()));
        }
        let filter = match conditions.is_empty() {
            true => String::new(),
            false => format!(" where {}", conditions.join(" and ")),
        };
        let mut statement = self
            .connection
            .prepare(&format!("select id from connections{filter} limit 1"))?;
        let mut rows = statement.query(rusqlite::params_from_iter(values))?;
        Ok(match rows.next()? {
            Some(row) => Some(row.get(0)?),
            None => None,
        })
    }

    fn raw(&self, id: i64) -> anyhow::Result<HashMap<String, Option<String>>> {
        let mut statement = self
            .connection
            .prepare("select * from connections where id = ?1")?;
        let columns: Vec<String> = statement
            .column_names()
            .into_iter()
            .map(String::from)
            .collect();
        let mut rows = statement.query([id])?;
        let Some(row) = rows.next()? else {
            bail!("connection {id} is no longer in the store");
        };
        let mut values = HashMap::new();
        for (index, column) in columns.into_iter().enumerate() {
            values.insert(column, text_of(row.get_ref(index)?));
        }
        Ok(values)
    }

    fn changed(&self, original: Option<String>, field: &Field) -> bool {
        match field {
            Field::Null => original.is_some(),
            Field::Text(text) => original.as_deref() != Some(text),
            Field::Number(number) => original != Some(number.to_string()),
            Field::Flag(flag) => original.as_deref() != Some(if *flag { "1" } else { "0" }),
            Field::Secret(plain) => {
                let Some(payload) = original else {
                    return true;
                };
                Encrypter::from_file(&self.key)
                    .and_then(|encrypter| encrypter.decrypt_string(&payload))
                    .map_or(true, |decrypted| decrypted != *plain)
            }
        }
    }

    fn stored(&self, field: &Field) -> anyhow::Result<Value> {
        Ok(match field {
            Field::Text(text) => Value::Text(text.clone()),
            Field::Number(number) => Value::Integer(*number),
            Field::Flag(flag) => Value::Integer(i64::from(*flag)),
            Field::Secret(plain) => {
                Value::Text(Encrypter::ensure(&self.key)?.encrypt_string(plain)?)
            }
            Field::Null => Value::Null,
        })
    }
}

fn quoted(columns: &[&str]) -> Vec<String> {
    columns
        .iter()
        .map(|column| format!("\"{column}\""))
        .collect()
}

fn text_of(value: ValueRef) -> Option<String> {
    match value {
        ValueRef::Null => None,
        ValueRef::Integer(number) => Some(number.to_string()),
        ValueRef::Real(number) => Some(number.to_string()),
        ValueRef::Text(bytes) | ValueRef::Blob(bytes) => {
            Some(String::from_utf8_lossy(bytes).into_owned())
        }
    }
}

pub fn now() -> String {
    jiff::Timestamp::now()
        .strftime("%Y-%m-%d %H:%M:%S")
        .to_string()
}

#[cfg(unix)]
fn restrict(path: &Path, mode: u32) -> anyhow::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    Ok(fs::set_permissions(path, fs::Permissions::from_mode(mode))?)
}

#[cfg(not(unix))]
fn restrict(_path: &Path, _mode: u32) -> anyhow::Result<()> {
    Ok(())
}
