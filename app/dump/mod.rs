mod mysql;
mod postgres;
pub mod secrets;
mod sqlite;
pub mod statements;

use std::fs;
use std::path::Path;
use std::sync::LazyLock;

use regex::Regex;
use serde_json::{Map, Value, json};

use crate::database;
use crate::encrypter::Encrypter;
use crate::paths;
use crate::store::Connection;
use crate::tunnel;

pub use statements::StatementReader;

pub const MANIFEST: &str = "tql.json";

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DumpOptions {
    pub tables: Vec<String>,
    pub threads: usize,
    pub data_only: bool,
    pub no_lock: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LoadOptions {
    pub drop: bool,
    pub threads: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Update {
    pub done: usize,
    pub total: Option<usize>,
    pub current: Option<String>,
}

pub type Native<'a> = Box<dyn FnOnce() -> anyhow::Result<()> + 'a>;
pub type Watch = Box<dyn FnMut(&str) -> Option<Update>>;
pub type Count<'a> = Box<dyn FnOnce() -> anyhow::Result<usize> + 'a>;

#[derive(Default)]
pub struct Job<'a> {
    pub command: Vec<String>,
    pub environment: Vec<(String, String)>,
    pub native: Option<Native<'a>>,
    pub progress: Option<Watch>,
    pub tables: Option<Count<'a>>,
}

static BARE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[A-Za-z0-9_/.=:,@%+-]+$").expect("the pattern is valid"));

impl<'a> Job<'a> {
    pub fn native(work: impl FnOnce() -> anyhow::Result<()> + 'a) -> Self {
        Job {
            native: Some(Box::new(work)),
            ..Job::default()
        }
    }

    pub fn expected_tables(&mut self) -> usize {
        self.tables
            .take()
            .and_then(|count| count().ok())
            .unwrap_or(0)
    }

    pub fn describe(&self) -> String {
        self.command
            .iter()
            .map(|part| match BARE.is_match(part) {
                true => part.clone(),
                false => format!("'{}'", part.replace('\'', r"'\''")),
            })
            .collect::<Vec<_>>()
            .join(" ")
    }
}

pub fn counting_tables(pattern: &str) -> Watch {
    let pattern = Regex::new(pattern).expect("the pattern is valid");
    let mut seen: Vec<String> = Vec::new();
    Box::new(move |line| {
        let table = pattern.captures(line)?[1].to_string();
        if !seen.contains(&table) {
            seen.push(table.clone());
        }
        Some(Update {
            done: seen.len(),
            total: None,
            current: Some(table),
        })
    })
}

pub fn reported_tables(count_pattern: &str, name_pattern: &str) -> Watch {
    let count_pattern = Regex::new(count_pattern).expect("the pattern is valid");
    let name_pattern = Regex::new(name_pattern).expect("the pattern is valid");
    let mut done = 0;
    let mut total = None;
    let mut name = None;
    Box::new(move |line| {
        let named = name_pattern.captures(line);
        let counted = count_pattern.captures(line);
        if named.is_none() && counted.is_none() {
            return None;
        }
        if let Some(named) = named {
            name = Some(named[1].to_string());
        }
        if let Some(counted) = counted {
            done = counted[1].parse().unwrap_or(0);
            total = counted[2].parse().ok();
        }
        Some(Update {
            done,
            total,
            current: name.clone(),
        })
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Engine {
    MySql,
    Postgres,
    Sqlite,
}

const ENGINES: [Engine; 3] = [Engine::MySql, Engine::Postgres, Engine::Sqlite];

impl Engine {
    pub fn for_driver(driver: &str) -> Option<Engine> {
        ENGINES.into_iter().find(|engine| engine.name() == driver)
    }

    pub fn recognise(directory: &Path) -> Option<Engine> {
        if let Some(named) = read_manifest(directory)
            .as_ref()
            .and_then(|manifest| manifest.get("engine"))
            .and_then(Value::as_str)
        {
            return Engine::for_driver(named);
        }
        ENGINES
            .into_iter()
            .find(|engine| engine.recognises(directory))
    }

    pub fn name(self) -> &'static str {
        match self {
            Engine::MySql => "mysql",
            Engine::Postgres => "pgsql",
            Engine::Sqlite => "sqlite",
        }
    }

    pub fn dumper(self) -> Option<&'static str> {
        match self {
            Engine::MySql => Some("mydumper"),
            Engine::Postgres => Some("pg_dump"),
            Engine::Sqlite => None,
        }
    }

    pub fn loader(self) -> Option<&'static str> {
        match self {
            Engine::MySql => Some("myloader"),
            Engine::Postgres => Some("pg_restore"),
            Engine::Sqlite => None,
        }
    }

    pub fn install(self) -> &'static str {
        let mac = cfg!(target_os = "macos");
        match (self, mac) {
            (Engine::MySql, true) => "brew install mydumper",
            (Engine::MySql, false) => {
                "install mydumper from your package manager, or from https://github.com/mydumper/mydumper/releases"
            }
            (Engine::Postgres, true) => "brew install libpq && brew link --force libpq",
            (Engine::Postgres, false) => {
                "install the postgresql-client package from your package manager"
            }
            (Engine::Sqlite, _) => "",
        }
    }

    pub fn compresses(self) -> bool {
        self == Engine::Postgres
    }

    pub fn recognises(self, directory: &Path) -> bool {
        let file = match self {
            Engine::MySql => "metadata",
            Engine::Postgres => "toc.dat",
            Engine::Sqlite => sqlite::FILE,
        };
        directory.join(file).is_file()
    }

    pub fn dump<'a>(
        self,
        connection: &Connection,
        directory: &Path,
        options: &DumpOptions,
        encrypter: Option<&'a Encrypter>,
    ) -> anyhow::Result<Job<'a>> {
        match self {
            Engine::MySql => mysql::dump(connection, directory, options, encrypter),
            Engine::Postgres => postgres::dump(connection, directory, options, encrypter),
            Engine::Sqlite => sqlite::dump(connection, directory, options),
        }
    }

    pub fn load<'a>(
        self,
        connection: &Connection,
        directory: &Path,
        options: &LoadOptions,
        encrypter: Option<&'a Encrypter>,
    ) -> anyhow::Result<Job<'a>> {
        match self {
            Engine::MySql => mysql::load(connection, directory, options, encrypter),
            Engine::Postgres => postgres::load(connection, directory, options, encrypter),
            Engine::Sqlite => sqlite::load(connection, directory, options),
        }
    }

    pub fn estimated_bytes(
        self,
        connection: &Connection,
        tables: &[String],
        encrypter: Option<&Encrypter>,
    ) -> anyhow::Result<Option<u64>> {
        match self {
            Engine::MySql => mysql::estimated_bytes(connection, tables, encrypter).map(Some),
            Engine::Postgres => postgres::estimated_bytes(connection, tables, encrypter).map(Some),
            Engine::Sqlite => Ok(sqlite::estimated_bytes(connection)),
        }
    }
}

pub fn label(driver: &str) -> String {
    match driver {
        "mysql" => "MySQL",
        "pgsql" => "Postgres",
        "sqlite" => "SQLite",
        "sqlsrv" => "SQL Server",
        other => other,
    }
    .to_string()
}

pub fn active_database(connection: &Connection) -> String {
    connection.database.clone().unwrap_or_default()
}

pub fn endpoint(
    connection: &Connection,
    encrypter: Option<&Encrypter>,
) -> anyhow::Result<(String, u16)> {
    if connection.uses_ssh() {
        let password = database::decrypted(&connection.ssh_password, encrypter)?;
        let tunnel = tunnel::for_connection(connection, password)?;
        return Ok(("127.0.0.1".to_string(), tunnel.port));
    }
    Ok((
        connection.host.clone().unwrap_or_default(),
        connection.port.unwrap_or_default(),
    ))
}

pub fn password(connection: &Connection, encrypter: Option<&Encrypter>) -> anyhow::Result<String> {
    Ok(database::decrypted(&connection.password, encrypter)?.unwrap_or_default())
}

pub fn expanded(path: &Option<String>) -> String {
    paths::expand(path.as_deref().unwrap_or_default())
}

pub fn count_of(
    connection: &Connection,
    encrypter: Option<&Encrypter>,
    statement: &str,
    values: &[String],
) -> anyhow::Result<u64> {
    let mut database = database::connect(connection, encrypter)?;
    let page = database.select(statement, values)?;
    Ok(page
        .rows
        .first()
        .and_then(|row| row.first().cloned().flatten())
        .and_then(|value| value.trim().parse::<f64>().ok())
        .map(|value| value as u64)
        .unwrap_or(0))
}

pub fn write_manifest(
    directory: &Path,
    engine: Engine,
    connection: &Connection,
    options: &DumpOptions,
    version: &str,
) -> anyhow::Result<()> {
    let database = match connection.driver.as_str() {
        "sqlite" => Value::String(
            Path::new(connection.database.as_deref().unwrap_or_default())
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_default(),
        ),
        _ => connection
            .database
            .clone()
            .map(Value::String)
            .unwrap_or(Value::Null),
    };
    let manifest = json!({
        "tql": match version.starts_with('v') {
            true => version.to_string(),
            false => format!("v{version}"),
        },
        "engine": engine.name(),
        "tool": engine.dumper().unwrap_or("sqlite backup"),
        "connection": connection.name,
        "database": database,
        "tables": match options.tables.is_empty() {
            true => Value::Null,
            false => json!(options.tables),
        },
        "data_only": options.data_only,
        "dumped_at": jiff::Zoned::now().strftime("%Y-%m-%dT%H:%M:%S%:z").to_string(),
    });
    fs::write(
        directory.join(MANIFEST),
        format!("{}\n", crate::agent::pretty(&manifest)),
    )?;
    Ok(())
}

pub fn read_manifest(directory: &Path) -> Option<Map<String, Value>> {
    let read = fs::read_to_string(directory.join(MANIFEST)).ok()?;
    match serde_json::from_str(&read).ok()? {
        Value::Object(manifest) => Some(manifest),
        _ => None,
    }
}

pub fn human_bytes(bytes: u64) -> String {
    let units = ["B", "KB", "MB", "GB", "TB"];
    let mut power = 0;
    while power < units.len() - 1 && bytes >= 1024u64.pow(power as u32 + 1) {
        power += 1;
    }
    let scaled = bytes as f64 / 1024f64.powi(power as i32);
    let rounded = (scaled * 10.0).round() / 10.0;
    match rounded.fract() == 0.0 {
        true => format!("{} {}", rounded as u64, units[power]),
        false => format!("{rounded} {}", units[power]),
    }
}

pub fn elapsed(seconds: f64) -> String {
    let whole = seconds.round() as u64;
    match seconds {
        seconds if seconds < 10.0 => format!("{seconds:.1}s"),
        seconds if seconds < 60.0 => format!("{whole}s"),
        _ => format!("{}m {}s", whole / 60, whole % 60),
    }
}

pub fn directory_size(directory: &Path) -> u64 {
    let Ok(entries) = fs::read_dir(directory) else {
        return 0;
    };
    entries
        .flatten()
        .map(|entry| match entry.file_type() {
            Ok(kind) if kind.is_dir() => directory_size(&entry.path()),
            _ => entry.metadata().map(|meta| meta.len()).unwrap_or(0),
        })
        .sum()
}

pub fn free_space(directory: &Path) -> Option<u64> {
    let mut nearest = directory.to_path_buf();
    while !nearest.is_dir() {
        match nearest.parent() {
            Some(parent) if parent != nearest => nearest = parent.to_path_buf(),
            _ => break,
        }
    }
    let path = std::ffi::CString::new(nearest.as_os_str().as_encoded_bytes()).ok()?;
    let mut stats: libc::statvfs = unsafe { std::mem::zeroed() };
    if unsafe { libc::statvfs(path.as_ptr(), &mut stats) } != 0 {
        return None;
    }
    Some(stats.f_bavail as u64 * stats.f_frsize as u64)
}

pub fn slug(subject: &str, fallback: &str) -> String {
    static RUNS: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"[^A-Za-z0-9]+").expect("the pattern is valid"));
    let slug = RUNS.replace_all(subject, "-").to_lowercase();
    match slug.is_empty() {
        true => fallback.to_string(),
        false => slug,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sizes_read_the_way_tql_has_always_written_them() {
        assert_eq!(human_bytes(0), "0 B");
        assert_eq!(human_bytes(8192), "8 KB");
        assert_eq!(human_bytes(8400), "8.2 KB");
        assert_eq!(human_bytes(14 * 1024 * 1024), "14 MB");
        assert_eq!(human_bytes(1023), "1023 B");
    }

    #[test]
    fn times_read_in_tenths_then_seconds_then_minutes() {
        assert_eq!(elapsed(1.234), "1.2s");
        assert_eq!(elapsed(12.6), "13s");
        assert_eq!(elapsed(125.0), "2m 5s");
    }

    #[test]
    fn describes_a_command_with_only_odd_arguments_quoted() {
        let job = Job {
            command: vec![
                "mydumper".into(),
                "--database=shop".into(),
                "--outputdir=/tmp/my dump".into(),
                "it's".into(),
            ],
            ..Job::default()
        };

        assert_eq!(
            job.describe(),
            r"mydumper --database=shop '--outputdir=/tmp/my dump' 'it'\''s'"
        );
    }

    #[test]
    fn counts_each_table_once_as_the_dumper_reports_it() {
        let mut watch = counting_tables(r"dumping contents of table ([^ ]+)");

        assert_eq!(watch("noise"), None);
        assert_eq!(watch("dumping contents of table a").unwrap().done, 1);
        assert_eq!(watch("dumping contents of table a").unwrap().done, 1);
        let update = watch("dumping contents of table b").unwrap();
        assert_eq!((update.done, update.current.as_deref()), (2, Some("b")));
    }

    #[test]
    fn takes_the_count_and_the_name_from_the_loaders_own_report() {
        let mut watch = reported_tables(
            r"Tables (\d+) of (\d+) completed",
            r"restoring (?:table |indexes )?[^.\s]+\.(\S+)",
        );

        assert_eq!(
            watch("restoring table shop.orders"),
            Some(Update {
                done: 0,
                total: None,
                current: Some("orders".into())
            })
        );
        assert_eq!(
            watch("Tables 3 of 7 completed"),
            Some(Update {
                done: 3,
                total: Some(7),
                current: Some("orders".into())
            })
        );
    }
}
