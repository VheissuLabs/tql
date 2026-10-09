use std::fs;
use std::path::Path;

use anyhow::{anyhow, bail};
use rusqlite::{Connection as Sqlite, OpenFlags};

use crate::dump::{DumpOptions, Job, LoadOptions};
use crate::paths;
use crate::store::Connection;

pub const FILE: &str = "database.sqlite";

pub fn dump<'a>(
    connection: &Connection,
    directory: &Path,
    options: &DumpOptions,
) -> anyhow::Result<Job<'a>> {
    if !options.tables.is_empty() || options.data_only {
        bail!("A SQLite dump is the whole file; it takes no tables and no --data-only.");
    }
    let source = file_of(connection);
    let directory = directory.to_path_buf();
    Ok(Job::native(move || {
        if !directory.is_dir() {
            fs::create_dir_all(&directory)?;
        }
        copy(Path::new(&source), &directory.join(FILE))
    }))
}

pub fn load<'a>(
    connection: &Connection,
    directory: &Path,
    options: &LoadOptions,
) -> anyhow::Result<Job<'a>> {
    let target = file_of(connection);
    let occupied = fs::metadata(&target)
        .map(|meta| meta.is_file() && meta.len() > 0)
        .unwrap_or(false);
    if !options.drop && occupied {
        bail!(
            "{} already has a database at {target}; --drop replaces it.",
            connection.name
        );
    }
    let source = directory.join(FILE);
    Ok(Job::native(move || copy(&source, Path::new(&target))))
}

pub fn estimated_bytes(connection: &Connection) -> Option<u64> {
    fs::metadata(file_of(connection))
        .ok()
        .filter(|meta| meta.is_file())
        .map(|meta| meta.len())
}

fn file_of(connection: &Connection) -> String {
    paths::expand(connection.database.as_deref().unwrap_or_default())
}

fn copy(from: &Path, to: &Path) -> anyhow::Result<()> {
    let failed = |error: rusqlite::Error| anyhow!("SQLite could not copy the database: {error}");
    let source = Sqlite::open_with_flags(from, OpenFlags::SQLITE_OPEN_READ_ONLY).map_err(failed)?;
    let mut destination = Sqlite::open(to).map_err(failed)?;
    let backup = rusqlite::backup::Backup::new(&source, &mut destination).map_err(failed)?;
    match backup.step(-1).map_err(failed)? {
        rusqlite::backup::StepResult::Done => Ok(()),
        other => Err(anyhow!("SQLite could not copy the database: {other:?}")),
    }
}
