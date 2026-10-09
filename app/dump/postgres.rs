use std::path::Path;
use std::process::Command;

use crate::dump::{self, DumpOptions, Job, LoadOptions, active_database, counting_tables, secrets};
use crate::encrypter::Encrypter;
use crate::store::Connection;

pub fn dump<'a>(
    connection: &Connection,
    directory: &Path,
    options: &DumpOptions,
    encrypter: Option<&'a Encrypter>,
) -> anyhow::Result<Job<'a>> {
    let mut command = vec!["pg_dump".to_string()];
    command.extend(target(connection, encrypter)?);
    command.extend([
        "--format=directory".to_string(),
        format!("--jobs={}", options.threads),
        format!("--file={}", directory.display()),
        "--verbose".to_string(),
    ]);
    command.extend(
        options
            .tables
            .iter()
            .map(|table| format!("--table={table}")),
    );
    if options.data_only {
        command.push("--data-only".to_string());
    }

    let counted = connection.clone();
    let named = options.tables.len();
    Ok(Job {
        command,
        environment: environment(connection, encrypter)?,
        progress: Some(counting_tables(r#"dumping contents of table "([^"]+)""#)),
        tables: Some(Box::new(move || {
            if named > 0 {
                return Ok(named);
            }
            Ok(dump::count_of(
                &counted,
                encrypter,
                "select count(*) as tables from information_schema.tables where table_schema not in ('pg_catalog', 'information_schema') and table_type = 'BASE TABLE'",
                &[],
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
    let mut command = vec!["pg_restore".to_string()];
    command.extend(target(connection, encrypter)?);
    command.extend([
        "--format=directory".to_string(),
        format!("--jobs={}", options.threads),
        "--no-owner".to_string(),
        "--no-privileges".to_string(),
        "--exit-on-error".to_string(),
        "--verbose".to_string(),
    ]);
    if options.drop {
        command.extend(["--clean".to_string(), "--if-exists".to_string()]);
    }
    command.push(directory.display().to_string());

    let folder = directory.to_path_buf();
    Ok(Job {
        command,
        environment: environment(connection, encrypter)?,
        progress: Some(counting_tables(r#"processing data for table "([^"]+)""#)),
        tables: Some(Box::new(move || {
            let listing = Command::new("pg_restore")
                .arg("--list")
                .arg(folder)
                .output()?;
            Ok(String::from_utf8_lossy(&listing.stdout)
                .matches(" TABLE DATA ")
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
    if tables.is_empty() {
        return dump::count_of(
            connection,
            encrypter,
            "select pg_database_size(current_database()) as bytes",
            &[],
        );
    }
    let sizes = vec!["pg_table_size(?::regclass)"; tables.len()].join(" + ");
    dump::count_of(
        connection,
        encrypter,
        &format!("select {sizes} as bytes"),
        tables,
    )
}

fn target(connection: &Connection, encrypter: Option<&Encrypter>) -> anyhow::Result<Vec<String>> {
    let (host, port) = dump::endpoint(connection, encrypter)?;
    Ok(vec![
        format!("--host={host}"),
        format!("--port={port}"),
        format!(
            "--username={}",
            connection.username.as_deref().unwrap_or_default()
        ),
        format!("--dbname={}", active_database(connection)),
        "--no-password".to_string(),
    ])
}

fn environment(
    connection: &Connection,
    encrypter: Option<&Encrypter>,
) -> anyhow::Result<Vec<(String, String)>> {
    let password = dump::password(connection, encrypter)?
        .replace('\\', "\\\\")
        .replace(':', "\\:");
    let file = secrets::write("pgpass", &format!("*:*:*:*:{password}\n"))?;
    Ok([
        ("PGPASSFILE", file.display().to_string()),
        (
            "PGSSLMODE",
            connection
                .ssl_mode
                .as_deref()
                .unwrap_or_default()
                .trim()
                .to_string(),
        ),
        ("PGSSLROOTCERT", dump::expanded(&connection.ssl_ca)),
        ("PGSSLCERT", dump::expanded(&connection.ssl_cert)),
        ("PGSSLKEY", dump::expanded(&connection.ssl_key)),
    ]
    .into_iter()
    .filter(|(_, value)| !value.is_empty())
    .map(|(key, value)| (key.to_string(), value))
    .collect())
}
