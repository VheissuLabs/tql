use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::{anyhow, bail};
use redis::{Client, FromRedisValue, Value};

use super::{Change, Contents, KeyKind, KeyStore, PartlyWritten, Reply, Scanned, raw_words, shown};
use crate::database::decrypted;
use crate::database::tls::{Mode, Tls};
use crate::dsn::encode;
use crate::encrypter::Encrypter;
use crate::store::Connection;
use crate::tunnel::{self, Tunnel};

const MARKER: &str = "__tql_removed__";
const CONNECT: Duration = Duration::from_secs(10);
const ANSWER: Duration = Duration::from_secs(60);
const LONG: u64 = 256 * 1024;
const READS_THOUGH_ADMIN: [&str; 12] = [
    "config|get",
    "client|list",
    "client|info",
    "client|getname",
    "client|id",
    "slowlog|get",
    "slowlog|len",
    "commandlog|get",
    "commandlog|len",
    "latency|latest",
    "latency|history",
    "acl|whoami",
];

type Names = Arc<Mutex<HashMap<String, Vec<u8>>>>;

pub struct Redis {
    client: Client,
    connection: redis::Connection,
    database: u32,
    flags: HashMap<String, bool>,
    known: HashMap<String, bool>,
    names: Names,
    _tunnel: Option<Arc<Tunnel>>,
}

impl Redis {
    pub fn open(connection: &Connection, encrypter: Option<&Encrypter>) -> anyhow::Result<Self> {
        crate::database::tls::choose_ring();
        let password = decrypted(&connection.password, encrypter)?;
        let tunnel = match connection.uses_ssh() {
            true => Some(tunnel::for_connection(
                connection,
                decrypted(&connection.ssh_password, encrypter)?,
            )?),
            false => None,
        };
        let tls = Tls::of(connection)?;
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
        let bare = host.trim_start_matches('[').trim_end_matches(']');
        let host = match bare.contains(':') {
            true => format!("[{bare}]"),
            false => bare.to_string(),
        };
        let place = format!("{credentials}{host}:{port}/{database}");
        let plain = || Client::open(format!("redis://{place}"));
        let reached = match tls.mode {
            Mode::Unset | Mode::Disable => reach(plain()?),
            mode => {
                let secure = secured(&place, &tls)?;
                match (reach(secure), mode) {
                    (Err(_), Mode::Prefer) => reach(plain()?),
                    (reached, _) => reached,
                }
            }
        };
        let (client, opened) =
            reached.map_err(|error| anyhow!("cannot reach Redis at {host}:{port}: {error}"))?;
        Ok(Redis {
            client,
            connection: opened,
            database,
            flags: HashMap::new(),
            known: HashMap::new(),
            names: Arc::new(Mutex::new(HashMap::new())),
            _tunnel: tunnel,
        })
    }

    fn key(&self, name: &str) -> Vec<u8> {
        self.names
            .lock()
            .ok()
            .and_then(|names| names.get(name).cloned())
            .unwrap_or_else(|| name.as_bytes().to_vec())
    }

    fn named(&self, bytes: Vec<u8>) -> String {
        match String::from_utf8(bytes) {
            Ok(name) => name,
            Err(error) => {
                let bytes = error.into_bytes();
                let name = escaped(&bytes);
                if let Ok(mut names) = self.names.lock() {
                    names.insert(name.clone(), bytes);
                }
                name
            }
        }
    }

    fn reconnect(&mut self) -> anyhow::Result<()> {
        let mut connection = opened(&self.client)?;
        redis::cmd("SELECT")
            .arg(self.database)
            .query::<()>(&mut connection)
            .map_err(|error| anyhow!(said(&error)))?;
        self.connection = connection;
        Ok(())
    }

    fn ask<T: FromRedisValue>(&mut self, command: &redis::Cmd) -> anyhow::Result<T> {
        match command.query(&mut self.connection) {
            Err(error) if error.is_timeout() => {
                let _ = self.reconnect();
                Err(anyhow!(
                    "Redis did not answer within {}s, so tql stopped waiting",
                    ANSWER.as_secs()
                ))
            }
            Err(error) if broken(&error) => {
                self.reconnect()
                    .map_err(|_| anyhow!("{} · and tql could not reconnect", said(&error)))?;
                command
                    .query(&mut self.connection)
                    .map_err(|error| anyhow!(said(&error)))
            }
            answered => answered.map_err(|error| anyhow!(said(&error))),
        }
    }

    fn query(&mut self, command: &mut redis::Cmd) -> anyhow::Result<Value> {
        self.ask(command)
    }

    fn scan_all(
        &mut self,
        command: &str,
        key: &str,
        limit: usize,
    ) -> anyhow::Result<(Vec<Value>, bool)> {
        let mut found = Vec::new();
        let mut cursor = 0u64;
        let key = self.key(key);
        loop {
            let (next, items): (u64, Vec<Value>) = self.ask(
                redis::cmd(command)
                    .arg(&key)
                    .arg(cursor)
                    .arg("COUNT")
                    .arg(1000),
            )?;
            found.extend(items);
            cursor = next;
            if cursor == 0 || found.len() > limit {
                return Ok((found, cursor != 0));
            }
        }
    }

    fn checked(&mut self, changes: &[Change], keys: &[String]) -> anyhow::Result<()> {
        for key in keys {
            let kind: String = self.ask(redis::cmd("TYPE").arg(self.key(key)))?;
            let mut deleted = false;
            for change in changes.iter().filter(|change| change.key() == key) {
                let fits: &[&str] = match change {
                    Change::DeleteKey { .. } => {
                        deleted = true;
                        continue;
                    }
                    Change::NewKey { .. } if deleted => continue,
                    Change::NewKey { .. } => &["none"],
                    Change::SetString { .. } => &["string", "none"],
                    Change::SetField { .. } | Change::DeleteField { .. } => &["hash", "none"],
                    Change::SetItem { .. } | Change::RemoveItem { .. } => &["list"],
                    Change::PushItem { .. } => &["list", "none"],
                    Change::ReplaceMember { .. }
                    | Change::AddMember { .. }
                    | Change::RemoveMember { .. } => &["set", "none"],
                    Change::SetScore { .. }
                    | Change::AddScored { .. }
                    | Change::RemoveScored { .. } => &["zset", "none"],
                };
                if fits.contains(&kind.as_str()) {
                    continue;
                }
                match change {
                    Change::NewKey { .. } => bail!("{key} already exists, so nothing was written"),
                    _ => bail!("{key} is a {kind} now, so nothing was written"),
                }
            }
        }
        for change in changes {
            let (Change::SetItem {
                key, index, was, ..
            }
            | Change::RemoveItem { key, index, was }) = change
            else {
                continue;
            };
            let here: Option<Vec<u8>> =
                self.ask(redis::cmd("LINDEX").arg(self.key(key)).arg(*index))?;
            if here.as_deref() != Some(was.as_bytes()) {
                return Err(moved());
            }
        }
        Ok(())
    }
}

fn opened(client: &Client) -> redis::RedisResult<redis::Connection> {
    let connection = client.get_connection_with_timeout(CONNECT)?;
    connection.set_read_timeout(Some(ANSWER))?;
    connection.set_write_timeout(Some(ANSWER))?;
    Ok(connection)
}

fn reach(client: Client) -> redis::RedisResult<(Client, redis::Connection)> {
    let connection = opened(&client)?;
    Ok((client, connection))
}

fn secured(place: &str, tls: &Tls) -> anyhow::Result<Client> {
    let read = |path: &std::path::Path| {
        std::fs::read(path).map_err(|error| anyhow!("cannot read {}: {error}", path.display()))
    };
    let unchecked = match tls.checks_chain() {
        true => "",
        false => "#insecure",
    };
    Ok(Client::build_with_tls(
        format!("rediss://{place}{unchecked}"),
        redis::TlsCertificates {
            client_tls: match &tls.identity {
                Some((certificate, key)) => Some(redis::ClientTlsConfig {
                    client_cert: read(certificate)?,
                    client_key: read(key)?,
                }),
                None => None,
            },
            root_cert: match (&tls.ca, tls.checks_chain()) {
                (Some(ca), true) => Some(read(ca)?),
                _ => None,
            },
        },
    )?)
}

fn broken(error: &redis::RedisError) -> bool {
    error.is_io_error() || error.is_connection_dropped() || error.is_unrecoverable_error()
}

fn escaped(bytes: &[u8]) -> String {
    let mut name = String::new();
    for chunk in bytes.utf8_chunks() {
        name.push_str(chunk.valid());
        for byte in chunk.invalid() {
            name.push_str(&format!("\\x{byte:02x}"));
        }
    }
    name
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

fn raw(value: &Value) -> bool {
    matches!(value, Value::BulkString(bytes) if shown(bytes) != String::from_utf8_lossy(bytes))
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
        let (cursor, keys): (u64, Vec<Vec<u8>>) = self.ask(
            redis::cmd("SCAN")
                .arg(cursor)
                .arg("MATCH")
                .arg(if pattern.is_empty() { "*" } else { pattern })
                .arg("COUNT")
                .arg(count),
        )?;
        Ok(Scanned {
            cursor,
            keys: keys.into_iter().map(|key| self.named(key)).collect(),
        })
    }

    fn size(&mut self) -> anyhow::Result<u64> {
        self.ask(&redis::cmd("DBSIZE"))
    }

    fn kinds(&mut self, keys: &[String]) -> anyhow::Result<Vec<KeyKind>> {
        if keys.is_empty() {
            return Ok(Vec::new());
        }
        let mut pipe = redis::pipe();
        pipe.ignore_errors();
        for key in keys {
            pipe.cmd("TYPE").arg(self.key(key));
        }
        let answers: Vec<Value> = match pipe.query(&mut self.connection) {
            Err(error) if broken(&error) || error.is_timeout() => {
                let _ = self.reconnect();
                return Err(anyhow!(said(&error)));
            }
            answered => answered.map_err(|error| anyhow!(said(&error)))?,
        };
        Ok(answers
            .iter()
            .map(|answer| match answer {
                Value::ServerError(_) => KeyKind::Other("unreadable".to_string()),
                name => KeyKind::of(&text(name)),
            })
            .collect())
    }

    fn ttl(&mut self, key: &str) -> anyhow::Result<Option<i64>> {
        let seconds: i64 = self.ask(redis::cmd("TTL").arg(self.key(key)))?;
        Ok((seconds >= 0).then_some(seconds))
    }

    fn read(
        &mut self,
        key: &str,
        kind: &KeyKind,
        offset: usize,
        limit: usize,
    ) -> anyhow::Result<Contents> {
        let name = self.key(key);
        let exists: i64 = self.ask(redis::cmd("EXISTS").arg(&name))?;
        if exists == 0 {
            return Ok(Contents::Missing);
        }
        let rows = |headers: &[&str], rows: Vec<Vec<String>>, binary: Vec<bool>, more: bool| {
            Contents::Rows {
                headers: headers.iter().map(|header| header.to_string()).collect(),
                binary: binary
                    .into_iter()
                    .chain(std::iter::repeat(false))
                    .take(rows.len())
                    .collect(),
                rows,
                more,
            }
        };
        Ok(match kind {
            KeyKind::String => {
                let size: u64 = self.ask(redis::cmd("STRLEN").arg(&name))?;
                match size > LONG {
                    true => Contents::Long {
                        head: self.ask(redis::cmd("GETRANGE").arg(&name).arg(0).arg(LONG - 1))?,
                        size,
                    },
                    false => Contents::Text(self.ask(redis::cmd("GET").arg(&name))?),
                }
            }
            KeyKind::Hash => {
                let (items, more) = self.scan_all("HSCAN", key, (offset + limit) * 2)?;
                let flags: Vec<bool> = items
                    .chunks(2)
                    .skip(offset)
                    .take(limit)
                    .map(|pair| pair.iter().any(raw))
                    .collect();
                let all = pairs(items.clone());
                let more = more || all.len() > offset + limit;
                let mut page: Vec<Vec<String>> = all.into_iter().skip(offset).take(limit).collect();
                if page.is_empty() || !self.supports("httl") {
                    return Ok(rows(&["field", "value"], page, flags, more));
                }
                let fields: Vec<Vec<u8>> = items
                    .iter()
                    .step_by(2)
                    .skip(offset)
                    .take(limit)
                    .map(|field| match field {
                        Value::BulkString(bytes) => bytes.clone(),
                        other => text(other).into_bytes(),
                    })
                    .collect();
                let ttls: Vec<i64> = self.ask(
                    redis::cmd("HTTL")
                        .arg(&name)
                        .arg("FIELDS")
                        .arg(page.len())
                        .arg(fields),
                )?;
                for (row, ttl) in page.iter_mut().zip(ttls) {
                    row.push(match ttl {
                        seconds if seconds >= 0 => format!("{seconds}s"),
                        _ => "none".to_string(),
                    });
                }
                rows(&["field", "value", "ttl"], page, flags, more)
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
                    items.iter().skip(offset).take(limit).map(raw).collect(),
                    more,
                )
            }
            KeyKind::List => {
                let items: Vec<Value> = self.ask(
                    redis::cmd("LRANGE")
                        .arg(&name)
                        .arg(offset)
                        .arg(offset + limit),
                )?;
                let more = items.len() > limit;
                rows(
                    &["index", "value"],
                    items
                        .iter()
                        .take(limit)
                        .enumerate()
                        .map(|(at, item)| vec![(offset + at).to_string(), text(item)])
                        .collect(),
                    items.iter().take(limit).map(raw).collect(),
                    more,
                )
            }
            KeyKind::Zset => {
                let items: Vec<Value> = self.ask(
                    redis::cmd("ZRANGE")
                        .arg(&name)
                        .arg(offset)
                        .arg(offset + limit)
                        .arg("WITHSCORES"),
                )?;
                let flags: Vec<bool> = items
                    .chunks(2)
                    .take(limit)
                    .map(|pair| pair.iter().any(raw))
                    .collect();
                let all = pairs(items);
                let more = all.len() > limit;
                rows(
                    &["member", "score"],
                    all.into_iter().take(limit).collect(),
                    flags,
                    more,
                )
            }
            KeyKind::Stream => {
                let entries: Vec<Value> = self.ask(
                    redis::cmd("XRANGE")
                        .arg(&name)
                        .arg("-")
                        .arg("+")
                        .arg("COUNT")
                        .arg(offset + limit + 1),
                )?;
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
                                    Value::Array(items) => fields(pairs(items.clone())),
                                    other => text(other),
                                };
                                vec![text(&parts[0]), fields]
                            }
                            other => vec![text(&other), String::new()],
                        })
                        .collect(),
                    Vec::new(),
                    more,
                )
            }
            KeyKind::Other(name) => Contents::Unsupported(name.clone()),
        })
    }

    fn write(&mut self, changes: &[Change]) -> anyhow::Result<()> {
        let mut keys: Vec<String> = Vec::new();
        for change in changes {
            if !keys.iter().any(|key| key == change.key()) {
                keys.push(change.key().to_string());
            }
        }
        let mut watch = redis::cmd("WATCH");
        for key in &keys {
            watch.arg(self.key(key));
        }
        self.ask::<()>(&watch)?;
        if let Err(error) = self.checked(changes, &keys) {
            let _ = self.ask::<()>(&redis::cmd("UNWATCH"));
            return Err(error);
        }

        let mut pipe = redis::pipe();
        pipe.atomic();
        for change in changes {
            if let Change::SetItem {
                key, index, value, ..
            } = change
            {
                pipe.cmd("LSET")
                    .arg(self.key(key))
                    .arg(*index)
                    .arg(value)
                    .ignore();
            }
        }
        let mut emptied: Vec<&str> = Vec::new();
        for change in changes {
            if let Change::RemoveItem { key, index, .. } = change {
                pipe.cmd("LSET")
                    .arg(self.key(key))
                    .arg(*index)
                    .arg(MARKER)
                    .ignore();
                if !emptied.contains(&key.as_str()) {
                    emptied.push(key);
                }
            }
        }
        for key in emptied {
            pipe.cmd("LREM")
                .arg(self.key(key))
                .arg(0)
                .arg(MARKER)
                .ignore();
        }
        for change in changes {
            match change {
                Change::SetItem { .. } | Change::RemoveItem { .. } => {}
                Change::SetString { key, value } => {
                    pipe.cmd("SET")
                        .arg(self.key(key))
                        .arg(value)
                        .arg("KEEPTTL")
                        .ignore();
                }
                Change::SetField { key, field, value } => {
                    pipe.cmd("HSET")
                        .arg(self.key(key))
                        .arg(field)
                        .arg(value)
                        .ignore();
                }
                Change::DeleteField { key, field } => {
                    pipe.cmd("HDEL").arg(self.key(key)).arg(field).ignore();
                }
                Change::PushItem { key, value } => {
                    pipe.cmd("RPUSH").arg(self.key(key)).arg(value).ignore();
                }
                Change::ReplaceMember { key, was, value } => {
                    pipe.cmd("SREM").arg(self.key(key)).arg(was).ignore();
                    pipe.cmd("SADD").arg(self.key(key)).arg(value).ignore();
                }
                Change::AddMember { key, value } => {
                    pipe.cmd("SADD").arg(self.key(key)).arg(value).ignore();
                }
                Change::RemoveMember { key, value } => {
                    pipe.cmd("SREM").arg(self.key(key)).arg(value).ignore();
                }
                Change::SetScore { key, member, score }
                | Change::AddScored { key, member, score } => {
                    pipe.cmd("ZADD")
                        .arg(self.key(key))
                        .arg(score)
                        .arg(member)
                        .ignore();
                }
                Change::RemoveScored { key, member } => {
                    pipe.cmd("ZREM").arg(self.key(key)).arg(member).ignore();
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
                    pipe.cmd(command).arg(self.key(key)).arg(values).ignore();
                }
                Change::DeleteKey { key } => {
                    pipe.cmd("DEL").arg(self.key(key)).ignore();
                }
            }
        }
        match pipe.query::<Value>(&mut self.connection) {
            Ok(Value::Nil) => Err(anyhow!(
                "a key changed before it was written, so nothing was written"
            )),
            Ok(_) => Ok(()),
            Err(error) => {
                if broken(&error) || error.is_timeout() {
                    let _ = self.reconnect();
                }
                Err(PartlyWritten(said(&error)).into())
            }
        }
    }

    fn expire(&mut self, key: &str, seconds: Option<i64>) -> anyhow::Result<()> {
        let name = self.key(key);
        let done: i64 = match seconds {
            Some(seconds) => self.ask(redis::cmd("EXPIRE").arg(&name).arg(seconds))?,
            None => self.ask(redis::cmd("PERSIST").arg(&name))?,
        };
        if done == 0 {
            let exists: i64 = self.ask(redis::cmd("EXISTS").arg(&name))?;
            if exists == 0 {
                bail!("{key} is not in Redis, so it has no TTL to set; :w writes a new key first");
            }
        }
        Ok(())
    }

    fn rename(&mut self, from: &str, to: &str) -> anyhow::Result<()> {
        let renamed: i64 = self.ask(redis::cmd("RENAMENX").arg(self.key(from)).arg(to))?;
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
        let info: String = self.ask(redis::cmd("INFO").arg("keyspace"))?;
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
            Err(error) if broken(&error) || error.is_timeout() => {
                let reconnected = self.reconnect();
                Err(anyhow!(
                    "{}{}",
                    said(&error),
                    match reconnected {
                        Ok(()) => " · tql reconnected, so the next command runs",
                        Err(_) => " · and tql could not reconnect",
                    }
                ))
            }
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
            Err(error) => Ok(Reply::Error(said(&error))),
        }
    }

    fn reads_only(&mut self, command: &str, subcommand: Option<&str>) -> anyhow::Result<bool> {
        let command = command.to_lowercase();
        let name = match subcommand {
            Some(subcommand) => {
                let both = format!("{command}|{}", subcommand.to_lowercase());
                match self.supports(&both) {
                    true => both,
                    false => command,
                }
            }
            None => command,
        };
        if let Some(known) = self.flags.get(&name) {
            return Ok(*known);
        }
        let info: Value = self.ask(redis::cmd("COMMAND").arg("INFO").arg(&name))?;
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
        let known = !flags.is_empty() || self.supports(&name);
        let scripted = matches!(name.as_str(), "eval" | "evalsha" | "fcall");
        let reads = known
            && !scripted
            && (READS_THOUGH_ADMIN.contains(&name.as_str())
                || !flags.iter().any(|flag| {
                    matches!(
                        flag.as_str(),
                        "write" | "admin" | "pubsub" | "may_replicate" | "blocking"
                    )
                }));
        self.flags.insert(name, reads);
        Ok(reads)
    }

    fn server(&mut self) -> String {
        let info: String = self
            .ask(redis::cmd("INFO").arg("server"))
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

    fn supports(&mut self, command: &str) -> bool {
        let name = command.to_lowercase();
        if let Some(known) = self.known.get(&name) {
            return *known;
        }
        let found = matches!(
            redis::cmd("COMMAND").arg("INFO").arg(&name).query::<Value>(&mut self.connection),
            Ok(Value::Array(entries)) if entries.first().is_some_and(|entry| *entry != Value::Nil)
        );
        self.known.insert(name, found);
        found
    }

    fn expire_field(&mut self, key: &str, field: &str, seconds: Option<i64>) -> anyhow::Result<()> {
        if !self.supports("hexpire") {
            bail!("this server has no expiry on hash fields; Redis 7.4 and Valkey 9 do");
        }
        let mut command = match seconds {
            Some(seconds) => {
                let mut command = redis::cmd("HEXPIRE");
                command.arg(self.key(key)).arg(seconds);
                command
            }
            None => {
                let mut command = redis::cmd("HPERSIST");
                command.arg(self.key(key));
                command
            }
        };
        command.arg("FIELDS").arg(1).arg(field);
        let answered: Vec<i64> = self.ask(&command)?;
        match answered.first() {
            Some(-2) => bail!("{field} is no longer in {key}"),
            _ => Ok(()),
        }
    }

    fn log(&mut self) -> anyhow::Result<Contents> {
        let entries: Vec<Value> = match self.supports("commandlog") {
            true => self.ask(redis::cmd("COMMANDLOG").arg("GET").arg(50).arg("slow"))?,
            false => self.ask(redis::cmd("SLOWLOG").arg("GET").arg(50))?,
        };
        let rows: Vec<Vec<String>> = entries
            .into_iter()
            .filter_map(|entry| match entry {
                Value::Array(fields) => Some(fields),
                _ => None,
            })
            .map(|fields| {
                let number = |at: usize| match fields.get(at) {
                    Some(Value::Int(number)) => *number,
                    _ => 0,
                };
                let when = jiff::Timestamp::from_second(number(1))
                    .map(|at| at.strftime("%Y-%m-%d %H:%M:%S").to_string())
                    .unwrap_or_default();
                let command = match fields.get(3) {
                    Some(Value::Array(words)) => {
                        words.iter().map(text).collect::<Vec<_>>().join(" ")
                    }
                    Some(other) => text(other),
                    None => String::new(),
                };
                let client = fields.get(4).map(text).unwrap_or_default();
                vec![when, format!("{}µs", number(2)), command, client]
            })
            .collect();
        Ok(Contents::Rows {
            headers: ["when", "took", "command", "client"]
                .iter()
                .map(|header| header.to_string())
                .collect(),
            binary: vec![false; rows.len()],
            rows,
            more: false,
        })
    }

    fn fork(&mut self) -> anyhow::Result<Box<dyn KeyStore>> {
        let mut connection = opened(&self.client).map_err(|error| anyhow!(said(&error)))?;
        redis::cmd("SELECT")
            .arg(self.database)
            .query::<()>(&mut connection)
            .map_err(|error| anyhow!(said(&error)))?;
        Ok(Box::new(Redis {
            client: self.client.clone(),
            connection,
            database: self.database,
            flags: HashMap::new(),
            known: HashMap::new(),
            names: self.names.clone(),
            _tunnel: self._tunnel.clone(),
        }))
    }
}

fn fields(pairs: Vec<Vec<String>>) -> String {
    let mut names: Vec<&str> = pairs.iter().map(|pair| pair[0].as_str()).collect();
    names.sort_unstable();
    let repeated = names.windows(2).any(|two| two[0] == two[1]);
    let value =
        |pair: &Vec<String>| serde_json::Value::from(pair.get(1).cloned().unwrap_or_default());
    match repeated {
        true => serde_json::Value::Array(
            pairs
                .iter()
                .map(|pair| serde_json::Value::Array(vec![pair[0].clone().into(), value(pair)]))
                .collect(),
        ),
        false => serde_json::Value::Object(
            pairs
                .iter()
                .map(|pair| (pair[0].clone(), value(pair)))
                .collect(),
        ),
    }
    .to_string()
}
