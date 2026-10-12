use std::path::{Path, PathBuf};
use std::time::Instant;

use anyhow::{Context, anyhow, bail};
use serde_json::{Map, Value, json};

use crate::database::{self, Column, Database, Kind, Outcome, Table};
use crate::encrypter::Encrypter;
use crate::store::{Connection, Store};
use crate::store_writer::StoreWriter;
use crate::tui::history::{self, Execution};

pub const LIMIT: usize = 200;

pub const READ_ONLY: &str = "Only read-only statements are allowed here: select, show, explain, describe, pragma or with, one at a time. Change data in the tql interface.";

pub struct Agent {
    store: PathBuf,
    key: PathBuf,
}

impl Agent {
    pub fn new(store: impl Into<PathBuf>, key: impl Into<PathBuf>) -> Self {
        Agent {
            store: store.into(),
            key: key.into(),
        }
    }

    pub fn store(&self) -> &Path {
        &self.store
    }

    pub fn connections(&self) -> anyhow::Result<Value> {
        let listed: Vec<Value> = self
            .saved()?
            .into_iter()
            .map(|connection| {
                json!({
                    "name": connection.name,
                    "driver": connection.driver.name(),
                    "target": connection.describe(),
                    "read_only": connection.read_only,
                    "last_used_at": connection.last_used_at.as_deref().map(iso8601),
                })
            })
            .collect();
        Ok(Value::Array(listed))
    }

    pub fn tables(&self, name: Option<&str>, on: Option<&str>) -> anyhow::Result<Value> {
        let connection = self.connection(name, on)?;
        let tables = self
            .open(&connection)
            .and_then(|mut database| database.tables())
            .context("Could not read the schema")?;
        let tables: Vec<String> = tables.iter().map(Table::to_string).collect();
        Ok(json!({ "connection": connection.name, "tables": tables }))
    }

    pub fn describe(
        &self,
        name: Option<&str>,
        table: &str,
        on: Option<&str>,
    ) -> anyhow::Result<Value> {
        let connection = self.connection(name, on)?;
        let failed = || format!("Could not describe [{table}]");
        let mut database = self.open(&connection).with_context(failed)?;
        let listed = database.tables().with_context(failed)?;
        let default_schema = database.default_schema();
        let Some(table) = Table::resolve(table, &listed, default_schema.as_deref())
            .or_else(|| Table::parse(table))
        else {
            bail!("Table [{table}] has no columns, or does not exist.");
        };
        let columns = database.columns(&table).with_context(failed)?;
        if columns.is_empty() {
            bail!("Table [{table}] has no columns, or does not exist.");
        }
        let primary_key = database
            .indexes(&table)
            .ok()
            .and_then(|indexes| database::primary_key(&indexes));
        Ok(json!({
            "connection": connection.name,
            "table": table.to_string(),
            "primary_key": primary_key,
            "columns": columns.iter().map(described).collect::<Vec<_>>(),
        }))
    }

    pub fn query(
        &self,
        name: Option<&str>,
        statement: &str,
        source: &str,
        on: Option<&str>,
        limit: usize,
    ) -> anyhow::Result<Value> {
        let connection = self.connection(name, on)?;
        let statement = statement.trim();
        if !database::is_read_only(statement) {
            bail!(READ_ONLY);
        }

        let started = Instant::now();
        let ran = self
            .open(&connection)
            .and_then(|mut database| database.run_reading(statement));
        let duration_ms = started.elapsed().as_millis();
        history::record(
            &self.store,
            source,
            Execution {
                connection_id: connection.id,
                statement,
                error: ran
                    .as_ref()
                    .err()
                    .map(|error| format!("{error:#}"))
                    .as_deref(),
                rows: ran.as_ref().ok().map(|outcome| outcome.rows.len()),
                duration_ms,
            },
        );
        let outcome = ran?;

        let shown = limit.max(1);
        let total = outcome.rows.len();
        Ok(json!({
            "connection": connection.name,
            "rows": total,
            "duration_ms": duration_ms,
            "results": objects(&outcome, shown),
            "truncated": total > shown,
        }))
    }

    pub fn saved(&self) -> anyhow::Result<Vec<Connection>> {
        StoreWriter::prepare(&self.store)?;
        let mut connections = Store::open(&self.store).and_then(|store| store.connections())?;
        connections.sort_by(|left, right| left.name.cmp(&right.name));
        Ok(connections)
    }

    fn connection(&self, name: Option<&str>, on: Option<&str>) -> anyhow::Result<Connection> {
        let saved = self.saved()?;
        let found = name.and_then(|name| saved.iter().find(|connection| connection.name == name));
        let Some(connection) = found else {
            let known: Vec<&str> = saved
                .iter()
                .map(|connection| connection.name.as_str())
                .collect();
            return Err(anyhow!(
                "There is no tql connection named [{}]. Known connections: {}",
                name.unwrap_or_default(),
                known.join(", ")
            ));
        };
        let mut connection = connection.clone();
        if let Some(other) = on {
            connection.database = Some(other.to_string());
        }
        Ok(connection)
    }

    fn open(&self, connection: &Connection) -> anyhow::Result<Box<dyn Database>> {
        let encrypter = Encrypter::from_file(&self.key).ok();
        database::connect(connection, encrypter.as_ref())
    }
}

fn described(column: &Column) -> Value {
    json!({
        "name": column.name,
        "type_name": column.type_name,
        "type": column.full_type,
        "collation": column.collation,
        "nullable": column.nullable,
        "default": column.default,
        "auto_increment": column.auto_increment,
        "comment": column.comment,
        "generation": column.generation.as_ref().map(|generation| json!({
            "type": generation.kind,
            "expression": generation.expression,
        })),
    })
}

fn objects(outcome: &Outcome, limit: usize) -> Vec<Value> {
    outcome
        .rows
        .iter()
        .take(limit)
        .enumerate()
        .map(|(at, row)| {
            let mut object = Map::new();
            for (column, value) in row.iter().enumerate() {
                let kind = outcome
                    .kinds
                    .get(at)
                    .and_then(|kinds| kinds.get(column))
                    .copied()
                    .unwrap_or_default();
                let header = outcome.headers.get(column).cloned().unwrap_or_default();
                object.insert(header, typed(value.as_deref(), kind));
            }
            Value::Object(object)
        })
        .collect()
}

pub fn typed(value: Option<&str>, kind: Kind) -> Value {
    let Some(text) = value else {
        return Value::Null;
    };
    let typed = match kind {
        Kind::Integer => text.parse::<i64>().ok().map(Value::from),
        Kind::Float => database::json_float(text),
        Kind::Boolean => text.parse::<bool>().ok().map(Value::Bool),
        Kind::Text => None,
    };
    typed.unwrap_or_else(|| Value::String(text.to_string()))
}

fn iso8601(stored: &str) -> String {
    match jiff::civil::DateTime::strptime("%Y-%m-%d %H:%M:%S", stored) {
        Ok(at) => format!("{}+00:00", at.strftime("%Y-%m-%dT%H:%M:%S")),
        Err(_) => stored.to_string(),
    }
}

pub fn pretty(answer: &Value) -> String {
    serde_json::to_string_pretty(answer)
        .expect("a JSON value always serializes")
        .lines()
        .map(|line| {
            let indent = line.len() - line.trim_start_matches(' ').len();
            format!("{}{}", " ".repeat(indent * 2), &line[indent..])
        })
        .collect::<Vec<_>>()
        .join("\n")
}
