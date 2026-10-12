use std::collections::HashMap;
use std::sync::Arc;

use anyhow::{anyhow, bail};
use redis::{Client, Value};

use super::{Change, Contents, KeyKind, KeyStore, Reply, Scanned, raw_words, shown};
use crate::database::{decrypted, tls::Tls};
use crate::dsn::encode;
use crate::encrypter::Encrypter;
use crate::store::Connection;
use crate::tunnel::{self, Tunnel};

const MARKER: &str = "__tql_removed__";

pub struct Redis {
    client: Client,
    connection: redis::Connection,
    database: u32,
    flags: HashMap<String, bool>,
    _tunnel: Option<Arc<Tunnel>>,
}

impl Redis {
    pub fn open(connection: &Connection, encrypter: Option<&Encrypter>) -> anyhow::Result<Self> {
        let password = decrypted(&connection.password, encrypter)?;
        let tunnel = match connection.uses_ssh() {
            true => Some(tunnel::for_connection(
                connection,
                decrypted(&connection.ssh_password, encrypter)?,
            )?),
            false => None,
        };
        let secure = !matches!(
            Tls::of(connection)?.mode,
            crate::database::tls::Mode::Unset | crate::database::tls::Mode::Disable
        );
        let host = match &tunnel {
            Some(_) => "127.0.0.1".to_string(),
            None => connection
                .host
                .clone()
                .filter(|host| !host.trim().is_empty())
                .unwrap_or_else(|| "127.0.0.1".to_string()),
        };
        let port = tunnel
            .as_ref()
            .map(|tunnel| tunnel.port)
            .or(connection.port)
            .unwrap_or(6379);
        let database: u32 = connection
            .database
            .as_deref()
            .map(str::trim)
            .filter(|database| !database.is_empty())
            .map(|database| {
                database
                    .parse()
                    .map_err(|_| anyhow!("the Redis database is a number, not {database}"))
            })
            .transpose()?
            .unwrap_or(0);
        let credentials = match (connection.username.as_deref(), password.as_deref()) {
            (Some(user), Some(password)) if !user.is_empty() => {
                format!("{}:{}@", encode(user), encode(password))
            }
            (_, Some(password)) if !password.is_empty() => format!(":{}@", encode(password)),
            (Some(user), _) if !user.is_empty() => format!("{}@", encode(user)),
            _ => String::new(),
        };
        let scheme = if secure { "rediss" } else { "redis" };
        let host = if host.contains(':') {
            format!("[{host}]")
        } else {
            host
        };
        let client = Client::open(format!("{scheme}://{credentials}{host}:{port}/{database}"))?;
        let opened = client
            .get_connection()
            .map_err(|error| anyhow!("cannot reach Redis at {host}:{port}: {error}"))?;
        Ok(Redis {
            client,
            connection: opened,
            database,
            flags: HashMap::new(),
            _tunnel: tunnel,
        })
    }

    fn query(&mut self, command: &mut redis::Cmd) -> anyhow::Result<Value> {
        command
            .query(&mut self.connection)
            .map_err(|error| anyhow!(said(&error)))
    }

    fn scan_all(
        &mut self,
        command: &str,
        key: &str,
        limit: usize,
    ) -> anyhow::Result<(Vec<Value>, bool)> {
        let mut found = Vec::new();
        let mut cursor = 0u64;
        loop {
            let (next, items): (u64, Vec<Value>) = redis::cmd(command)
                .arg(key)
                .arg(cursor)
                .arg("COUNT")
                .arg(1000)
                .query(&mut self.connection)
                .map_err(|error| anyhow!(said(&error)))?;
            found.extend(items);
            cursor = next;
            if cursor == 0 || found.len() > limit {
                return Ok((found, cursor != 0));
            }
        }
    }
}

fn said(error: &redis::RedisError) -> String {
    match error.detail() {
        Some(detail) => format!("{} {detail}", error.code().unwrap_or_default())
            .trim()
            .to_string(),
        None => error.to_string(),
    }
}

fn text(value: &Value) -> String {
    match value {
        Value::Nil => String::new(),
        Value::Int(number) => number.to_string(),
        Value::BulkString(bytes) => shown(bytes),
        Value::SimpleString(text) => text.clone(),
        Value::Okay => "OK".to_string(),
        Value::Double(number) => crate::database::exporter_real_text(*number),
        Value::Boolean(flag) => flag.to_string(),
        Value::VerbatimString { text, .. } => text.clone(),
        Value::BigNumber(number) => String::from_utf8_lossy(number).into_owned(),
        Value::Array(items) | Value::Set(items) => {
            serde_json::Value::Array(items.iter().map(|item| text(item).into()).collect())
                .to_string()
        }
        Value::Map(pairs) => serde_json::Value::Object(
            pairs
                .iter()
                .map(|(key, value)| (text(key), text(value).into()))
                .collect(),
        )
        .to_string(),
        other => format!("{other:?}"),
    }
}

fn reply(value: Value) -> Reply {
    match value {
        Value::Nil => Reply::Nil,
        Value::Int(number) => Reply::Number(number),
        Value::Array(items) | Value::Set(items) => {
            Reply::List(items.into_iter().map(reply).collect())
        }
        Value::Map(pairs) => Reply::Map(
            pairs
                .into_iter()
                .map(|(key, value)| (reply(key), reply(value)))
                .collect(),
        ),
        Value::ServerError(error) => Reply::Error(
            format!("{} {}", error.code(), error.details().unwrap_or_default())
                .trim()
                .to_string(),
        ),
        other => Reply::Text(text(&other)),
    }
}

fn pairs(items: Vec<Value>) -> Vec<Vec<String>> {
    items
        .chunks(2)
        .map(|pair| pair.iter().map(text).collect())
        .collect()
}

fn moved() -> anyhow::Error {
    anyhow!("a list item moved before it was written, so nothing was written")
}

impl KeyStore for Redis {
    fn scan(&mut self, cursor: u64, pattern: &str, count: usize) -> anyhow::Result<Scanned> {
        let (cursor, keys): (u64, Vec<Vec<u8>>) = redis::cmd("SCAN")
            .arg(cursor)
            .arg("MATCH")
            .arg(if pattern.is_empty() { "*" } else { pattern })
            .arg("COUNT")
            .arg(count)
            .query(&mut self.connection)
            .map_err(|error| anyhow!(said(&error)))?;
        Ok(Scanned {
            cursor,
            keys: keys
                .into_iter()
                .map(|key| String::from_utf8_lossy(&key).into_owned())
                .collect(),
        })
    }

    fn size(&mut self) -> anyhow::Result<u64> {
        redis::cmd("DBSIZE")
            .query(&mut self.connection)
            .map_err(|error| anyhow!(said(&error)))
    }

    fn kinds(&mut self, keys: &[String]) -> anyhow::Result<Vec<KeyKind>> {
        if keys.is_empty() {
            return Ok(Vec::new());
        }
        let mut pipe = redis::pipe();
        for key in keys {
            pipe.cmd("TYPE").arg(key);
        }
        let names: Vec<String> = pipe
            .query(&mut self.connection)
            .map_err(|error| anyhow!(said(&error)))?;
        Ok(names.iter().map(|name| KeyKind::of(name)).collect())
    }

    fn ttl(&mut self, key: &str) -> anyhow::Result<Option<i64>> {
        let seconds: i64 = redis::cmd("TTL")
            .arg(key)
            .query(&mut self.connection)
            .map_err(|error| anyhow!(said(&error)))?;
        Ok((seconds >= 0).then_some(seconds))
    }

    fn read(
        &mut self,
        key: &str,
        kind: &KeyKind,
        offset: usize,
        limit: usize,
    ) -> anyhow::Result<Contents> {
        let exists: i64 = redis::cmd("EXISTS")
            .arg(key)
            .query(&mut self.connection)
            .map_err(|error| anyhow!(said(&error)))?;
        if exists == 0 {
            return Ok(Contents::Missing);
        }
        let rows = |headers: &[&str], rows: Vec<Vec<String>>, more: bool| Contents::Rows {
            headers: headers.iter().map(|header| header.to_string()).collect(),
            rows,
            more,
        };
        Ok(match kind {
            KeyKind::String => {
                let bytes: Vec<u8> = redis::cmd("GET")
                    .arg(key)
                    .query(&mut self.connection)
                    .map_err(|error| anyhow!(said(&error)))?;
                Contents::Text(bytes)
            }
            KeyKind::Hash => {
                let (items, more) = self.scan_all("HSCAN", key, (offset + limit) * 2)?;
                let all = pairs(items);
                let more = more || all.len() > offset + limit;
                rows(
                    &["field", "value"],
                    all.into_iter().skip(offset).take(limit).collect(),
                    more,
                )
            }
            KeyKind::Set => {
                let (items, more) = self.scan_all("SSCAN", key, offset + limit)?;
                let more = more || items.len() > offset + limit;
                rows(
                    &["member"],
                    items
                        .iter()
                        .skip(offset)
                        .take(limit)
                        .map(|item| vec![text(item)])
                        .collect(),
                    more,
                )
            }
            KeyKind::List => {
                let items: Vec<Value> = redis::cmd("LRANGE")
                    .arg(key)
                    .arg(offset)
                    .arg(offset + limit)
                    .query(&mut self.connection)
                    .map_err(|error| anyhow!(said(&error)))?;
                let more = items.len() > limit;
                rows(
                    &["index", "value"],
                    items
                        .iter()
                        .take(limit)
                        .enumerate()
                        .map(|(at, item)| vec![(offset + at).to_string(), text(item)])
                        .collect(),
                    more,
                )
            }
            KeyKind::Zset => {
                let items: Vec<Value> = redis::cmd("ZRANGE")
                    .arg(key)
                    .arg(offset)
                    .arg(offset + limit)
                    .arg("WITHSCORES")
                    .query(&mut self.connection)
                    .map_err(|error| anyhow!(said(&error)))?;
                let all = pairs(items);
                let more = all.len() > limit;
                rows(
                    &["member", "score"],
                    all.into_iter().take(limit).collect(),
                    more,
                )
            }
            KeyKind::Stream => {
                let entries: Vec<Value> = redis::cmd("XRANGE")
                    .arg(key)
                    .arg("-")
                    .arg("+")
                    .arg("COUNT")
                    .arg(offset + limit + 1)
                    .query(&mut self.connection)
                    .map_err(|error| anyhow!(said(&error)))?;
                let more = entries.len() > offset + limit;
                rows(
                    &["id", "fields"],
                    entries
                        .into_iter()
                        .skip(offset)
                        .take(limit)
                        .map(|entry| match entry {
                            Value::Array(parts) if parts.len() == 2 => {
                                let fields = match &parts[1] {
                                    Value::Array(items) => serde_json::Value::Object(
                                        pairs(items.clone())
                                            .into_iter()
                                            .map(|pair| {
                                                (
                                                    pair[0].clone(),
                                                    pair.get(1).cloned().unwrap_or_default().into(),
                                                )
                                            })
                                            .collect(),
                                    )
                                    .to_string(),
                                    other => text(other),
                                };
                                vec![text(&parts[0]), fields]
                            }
                            other => vec![text(&other), String::new()],
                        })
                        .collect(),
                    more,
                )
            }
            KeyKind::Other(name) => Contents::Unsupported(name.clone()),
        })
    }

    fn write(&mut self, changes: &[Change]) -> anyhow::Result<()> {
        let watched: Vec<&Change> = changes
            .iter()
            .filter(|change| matches!(change, Change::SetItem { .. } | Change::RemoveItem { .. }))
            .collect();
        for change in &watched {
            self.query(redis::cmd("WATCH").arg(change.key()))?;
        }
        for change in &watched {
            let (Change::SetItem {
                key, index, was, ..
            }
            | Change::RemoveItem { key, index, was }) = change
            else {
                continue;
            };
            let here: Option<Vec<u8>> = redis::cmd("LINDEX")
                .arg(key)
                .arg(*index)
                .query(&mut self.connection)
                .map_err(|error| anyhow!(said(&error)))?;
            if here.as_deref() != Some(was.as_bytes()) {
                let _ = self.query(&mut redis::cmd("UNWATCH"));
                return Err(moved());
            }
        }

        let mut pipe = redis::pipe();
        pipe.atomic();
        for change in changes {
            match change {
                Change::SetString { key, value } => {
                    pipe.cmd("SET").arg(key).arg(value).ignore();
                }
                Change::SetField { key, field, value } => {
                    pipe.cmd("HSET").arg(key).arg(field).arg(value).ignore();
                }
                Change::DeleteField { key, field } => {
                    pipe.cmd("HDEL").arg(key).arg(field).ignore();
                }
                Change::SetItem {
                    key, index, value, ..
                } => {
                    pipe.cmd("LSET").arg(key).arg(*index).arg(value).ignore();
                }
                Change::PushItem { key, value } => {
                    pipe.cmd("RPUSH").arg(key).arg(value).ignore();
                }
                Change::RemoveItem { key, index, .. } => {
                    pipe.cmd("LSET").arg(key).arg(*index).arg(MARKER).ignore();
                    pipe.cmd("LREM").arg(key).arg(1).arg(MARKER).ignore();
                }
                Change::ReplaceMember { key, was, value } => {
                    pipe.cmd("SREM").arg(key).arg(was).ignore();
                    pipe.cmd("SADD").arg(key).arg(value).ignore();
                }
                Change::AddMember { key, value } => {
                    pipe.cmd("SADD").arg(key).arg(value).ignore();
                }
                Change::RemoveMember { key, value } => {
                    pipe.cmd("SREM").arg(key).arg(value).ignore();
                }
                Change::SetScore { key, member, score }
                | Change::AddScored { key, member, score } => {
                    pipe.cmd("ZADD").arg(key).arg(score).arg(member).ignore();
                }
                Change::RemoveScored { key, member } => {
                    pipe.cmd("ZREM").arg(key).arg(member).ignore();
                }
                Change::NewKey { key, kind, first } => {
                    let command = match kind {
                        KeyKind::String => "SET",
                        KeyKind::Hash => "HSET",
                        KeyKind::List => "RPUSH",
                        KeyKind::Set => "SADD",
                        KeyKind::Zset => "ZADD",
                        other => bail!("tql cannot make a {} key", other.name()),
                    };
                    let mut values = first.clone();
                    if *kind == KeyKind::Zset && values.len() == 2 {
                        values.reverse();
                    }
                    pipe.cmd(command).arg(key).arg(values).ignore();
                }
                Change::DeleteKey { key } => {
                    pipe.cmd("DEL").arg(key).ignore();
                }
            }
        }
        let done: Value = pipe
            .query(&mut self.connection)
            .map_err(|error| anyhow!(said(&error)))?;
        if done == Value::Nil {
            return Err(moved());
        }
        Ok(())
    }

    fn expire(&mut self, key: &str, seconds: Option<i64>) -> anyhow::Result<()> {
        match seconds {
            Some(seconds) => self.query(redis::cmd("EXPIRE").arg(key).arg(seconds))?,
            None => self.query(redis::cmd("PERSIST").arg(key))?,
        };
        Ok(())
    }

    fn rename(&mut self, from: &str, to: &str) -> anyhow::Result<()> {
        let renamed: i64 = redis::cmd("RENAMENX")
            .arg(from)
            .arg(to)
            .query(&mut self.connection)
            .map_err(|error| anyhow!(said(&error)))?;
        if renamed == 0 {
            bail!("{to} already exists, so {from} keeps its name");
        }
        Ok(())
    }

    fn databases(&mut self) -> anyhow::Result<Vec<(u32, u64)>> {
        let count: u32 = redis::cmd("CONFIG")
            .arg("GET")
            .arg("databases")
            .query::<Vec<String>>(&mut self.connection)
            .ok()
            .and_then(|pair| pair.get(1)?.parse().ok())
            .unwrap_or(16);
        let info: String = redis::cmd("INFO")
            .arg("keyspace")
            .query(&mut self.connection)
            .map_err(|error| anyhow!(said(&error)))?;
        let mut keys: HashMap<u32, u64> = HashMap::new();
        for line in info.lines() {
            let Some((name, fields)) = line.trim().split_once(':') else {
                continue;
            };
            let Some(number) = name
                .strip_prefix("db")
                .and_then(|number| number.parse().ok())
            else {
                continue;
            };
            let count = fields
                .split(',')
                .find_map(|field| field.strip_prefix("keys="))
                .and_then(|count| count.parse().ok())
                .unwrap_or_default();
            keys.insert(number, count);
        }
        Ok((0..count)
            .map(|number| (number, keys.get(&number).copied().unwrap_or_default()))
            .collect())
    }

    fn select(&mut self, database: u32) -> anyhow::Result<()> {
        self.query(redis::cmd("SELECT").arg(database))?;
        self.database = database;
        Ok(())
    }

    fn run(&mut self, line: &str) -> anyhow::Result<Reply> {
        let words = raw_words(line)?;
        let Some((first, rest)) = words.split_first() else {
            bail!("type a command, such as GET key");
        };
        let mut command = redis::cmd(&String::from_utf8_lossy(first));
        for word in rest {
            command.arg(word.as_slice());
        }
        match command.query::<Value>(&mut self.connection) {
            Ok(value) => {
                if String::from_utf8_lossy(first).eq_ignore_ascii_case("select")
                    && let Some(number) = rest
                        .first()
                        .and_then(|word| String::from_utf8_lossy(word).parse().ok())
                {
                    self.database = number;
                }
                Ok(reply(value))
            }
            Err(error) if error.is_io_error() || error.is_connection_dropped() => {
                Err(anyhow!(said(&error)))
            }
            Err(error) => Ok(Reply::Error(said(&error))),
        }
    }

    fn reads_only(&mut self, command: &str) -> anyhow::Result<bool> {
        let name = command.to_lowercase();
        if let Some(known) = self.flags.get(&name) {
            return Ok(*known);
        }
        let info: Value = redis::cmd("COMMAND")
            .arg("INFO")
            .arg(&name)
            .query(&mut self.connection)
            .map_err(|error| anyhow!(said(&error)))?;
        let flags: Vec<String> = match info {
            Value::Array(entries) => match entries.into_iter().next() {
                Some(Value::Array(fields)) => match fields.get(2) {
                    Some(Value::Array(flags) | Value::Set(flags)) => {
                        flags.iter().map(text).collect()
                    }
                    _ => Vec::new(),
                },
                _ => Vec::new(),
            },
            _ => Vec::new(),
        };
        let reads = flags.iter().any(|flag| flag == "readonly")
            && !flags.iter().any(|flag| flag == "write");
        self.flags.insert(name, reads);
        Ok(reads)
    }

    fn server(&mut self) -> String {
        let info: String = redis::cmd("INFO")
            .arg("server")
            .query(&mut self.connection)
            .unwrap_or_default();
        let field = |name: &str| {
            info.lines()
                .find_map(|line| line.trim().strip_prefix(&format!("{name}:")))
                .map(String::from)
        };
        match field("valkey_version") {
            Some(version) => format!("Valkey {version}"),
            None => format!("Redis {}", field("redis_version").unwrap_or_default()),
        }
    }

    fn fork(&mut self) -> anyhow::Result<Box<dyn KeyStore>> {
        let mut connection = self
            .client
            .get_connection()
            .map_err(|error| anyhow!(said(&error)))?;
        redis::cmd("SELECT")
            .arg(self.database)
            .query::<()>(&mut connection)
            .map_err(|error| anyhow!(said(&error)))?;
        Ok(Box::new(Redis {
            client: self.client.clone(),
            connection,
            database: self.database,
            flags: HashMap::new(),
            _tunnel: self._tunnel.clone(),
        }))
    }
}
