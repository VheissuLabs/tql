use std::path::Path;
use std::sync::LazyLock;

use regex::Regex;

use crate::dump::{
    self, DumpOptions, Job, LoadOptions, active_database, counting_tables, reported_tables, secrets,
};
use crate::encrypter::Encrypter;
use crate::store::Connection;

const SSL_MODES: [(&str, &str); 5] = [
    ("disable", "DISABLED"),
    ("prefer", "PREFERRED"),
    ("require", "REQUIRED"),
    ("verify-ca", "VERIFY_CA"),
    ("verify-full", "VERIFY_IDENTITY"),
];

pub fn dump<'a>(
    connection: &Connection,
    directory: &Path,
    options: &DumpOptions,
    encrypter: Option<&'a Encrypter>,
) -> anyhow::Result<Job<'a>> {
    let database = active_database(connection);
    let mut command = vec![
        "mydumper".to_string(),
        format!(
            "--defaults-file={}",
            client_file(connection, encrypter)?.display()
        ),
        format!("--database={database}"),
        format!("--outputdir={}", directory.display()),
        format!("--threads={}", options.threads),
        "--triggers".to_string(),
        "--routines".to_string(),
        "--skip-definer".to_string(),
        "--verbose=3".to_string(),
    ];
    if !options.tables.is_empty() {
        let listed: Vec<String> = options
            .tables
            .iter()
            .map(|table| format!("{database}.{table}"))
            .collect();
        command.push(format!("--tables-list={}", listed.join(",")));
    }
    if options.data_only {
        command.push("--no-schemas".to_string());
    }
    if options.no_lock {
        command.push("--sync-thread-lock-mode=NO_LOCK".to_string());
    }

    let counted = connection.clone();
    let named = options.tables.len();
    Ok(Job {
        command,
        progress: Some(counting_tables(r"`[^`]+`\.`([^`]+)` \[\s*\d+%\s*\]")),
        tables: Some(Box::new(move || {
            if named > 0 {
                return Ok(named);
            }
            let database = active_database(&counted);
            Ok(dump::count_of(
                &counted,
                encrypter,
                "select count(*) as tables from information_schema.tables where table_schema = ? and table_type = 'BASE TABLE'",
                &[database],
            )? as usize)
        })),
        ..Job::default()
    })
}

pub fn load<'a>(
    connection: &Connection,
    directory: &Path,
    options: &LoadOptions,
    encrypter: Option<&'a Encrypter>,
) -> anyhow::Result<Job<'a>> {
    let mut command = vec![
        "myloader".to_string(),
        format!(
            "--defaults-file={}",
            client_file(connection, encrypter)?.display()
        ),
        format!("--directory={}", directory.display()),
        format!("--database={}", active_database(connection)),
        format!("--threads={}", options.threads),
        "--verbose=3".to_string(),
    ];
    if options.drop {
        command.push("--drop-table=DROP".to_string());
    }

    static SCHEMA: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"^[^.]+\.[^.]+-schema\.sql").expect("the pattern is valid"));
    let folder = directory.to_path_buf();
    Ok(Job {
        command,
        progress: Some(reported_tables(
            r"Tables (\d+) of (\d+) completed",
            r"restoring (?:table |indexes )?[^.\s]+\.(\S+)",
        )),
        tables: Some(Box::new(move || {
            Ok(std::fs::read_dir(folder)?
                .flatten()
                .filter(|entry| SCHEMA.is_match(&entry.file_name().to_string_lossy()))
                .count())
        })),
        ..Job::default()
    })
}

pub fn estimated_bytes(
    connection: &Connection,
    tables: &[String],
    encrypter: Option<&Encrypter>,
) -> anyhow::Result<u64> {
    let mut statement = "select coalesce(sum(data_length), 0) as bytes from information_schema.tables where table_schema = ?".to_string();
    let mut values = vec![active_database(connection)];
    if !tables.is_empty() {
        statement.push_str(&format!(
            " and table_name in ({})",
            vec!["?"; tables.len()].join(", ")
        ));
        values.extend(tables.iter().cloned());
    }
    dump::count_of(connection, encrypter, &statement, &values)
}

pub fn client_file(
    connection: &Connection,
    encrypter: Option<&Encrypter>,
) -> anyhow::Result<std::path::PathBuf> {
    let (host, port) = dump::endpoint(connection, encrypter)?;
    let ssl_mode = connection.ssl_mode.as_deref().unwrap_or_default().trim();
    let settings = [
        ("host", host),
        ("port", port.to_string()),
        ("user", connection.username.clone().unwrap_or_default()),
        ("password", dump::password(connection, encrypter)?),
        (
            "ssl-mode",
            SSL_MODES
                .iter()
                .find(|(mode, _)| *mode == ssl_mode)
                .map(|(_, named)| named.to_string())
                .unwrap_or_default(),
        ),
        ("ssl-ca", dump::expanded(&connection.ssl_ca)),
        ("ssl-cert", dump::expanded(&connection.ssl_cert)),
        ("ssl-key", dump::expanded(&connection.ssl_key)),
    ];
    let lines: Vec<String> = settings
        .iter()
        .filter(|(_, value)| !value.is_empty())
        .map(|(key, value)| {
            format!(
                "{key}=\"{}\"",
                value.replace('\\', "\\\\").replace('"', "\\\"")
            )
        })
        .collect();
    secrets::write("client.cnf", &format!("[client]\n{}\n", lines.join("\n")))
}
