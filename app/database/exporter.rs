use std::fs::{self, DirBuilder, File};
use std::io::{BufWriter, Write};
use std::os::unix::fs::DirBuilderExt;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;
use std::time::Instant;

use anyhow::Context;
use jiff::Zoned;
use jiff::tz::TimeZone;
use regex::Regex;

use super::{Database, Table};
use crate::driver::Driver;
use crate::paths;
use crate::settings;
use crate::store::Connection;

const CHUNK: usize = 500;
const PER_STATEMENT: usize = 100;
pub const REMEMBERED: &str = "export_directory";

static NOT_A_WORD: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"[^A-Za-z0-9]+").expect("the slug pattern is valid"));

#[derive(Debug, Clone, PartialEq)]
pub enum Scalar {
    Null,
    Number(String),
    Text(String),
    Bytes(Vec<u8>),
}

impl Scalar {
    pub fn text(value: Option<String>) -> Self {
        value.map(Scalar::Text).unwrap_or(Scalar::Null)
    }

    pub fn real(number: f64) -> Self {
        if number.is_infinite() {
            let sign = if number < 0.0 { "-" } else { "" };
            return Scalar::Number(format!("{sign}9e999"));
        }
        if number.is_nan() {
            return Scalar::Null;
        }
        Scalar::Number(real_text(number))
    }

    pub fn literal(&self, driver: Driver) -> String {
        match self {
            Scalar::Null => "null".to_string(),
            Scalar::Number(number) => number.clone(),
            Scalar::Text(text) if driver == Driver::Mysql => mysql_quoted(text),
            Scalar::Text(text) if driver == Driver::Sqlsrv => {
                format!("N'{}'", text.replace('\'', "''"))
            }
            Scalar::Text(text) => format!("'{}'", text.replace('\'', "''")),
            Scalar::Bytes(bytes) if driver == Driver::Pgsql => format!("'\\x{}'", hex(bytes)),
            Scalar::Bytes(bytes) if driver == Driver::Sqlsrv => format!("0x{}", hex(bytes)),
            Scalar::Bytes(bytes) => format!("X'{}'", hex(bytes)),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Scalars {
    pub headers: Vec<String>,
    pub rows: Vec<Vec<Scalar>>,
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn mysql_quoted(text: &str) -> String {
    let mut quoted = String::with_capacity(text.len() + 2);
    quoted.push('\'');
    for character in text.chars() {
        match character {
            '\0' => quoted.push_str("\\0"),
            '\n' => quoted.push_str("\\n"),
            '\r' => quoted.push_str("\\r"),
            '\\' => quoted.push_str("\\\\"),
            '\'' => quoted.push_str("\\'"),
            '"' => quoted.push_str("\\\""),
            '\u{1a}' => quoted.push_str("\\Z"),
            other => quoted.push(other),
        }
    }
    quoted.push('\'');
    quoted
}

#[derive(Debug, Clone, PartialEq)]
pub struct Exported {
    pub path: PathBuf,
    pub rows: usize,
    pub bytes: u64,
    pub duration_ms: u128,
}

impl Exported {
    fn of(path: PathBuf, rows: usize, started: Instant) -> Self {
        Exported {
            bytes: fs::metadata(&path)
                .map(|meta| meta.len())
                .unwrap_or_default(),
            path,
            rows,
            duration_ms: started.elapsed().as_millis(),
        }
    }

    pub fn size(&self) -> String {
        let units = ["B", "KB", "MB", "GB"];
        let mut size = self.bytes as f64;
        let mut unit = 0;
        while size >= 1024.0 && unit < units.len() - 1 {
            size /= 1024.0;
            unit += 1;
        }
        if unit == 0 {
            return format!("{}B", self.bytes);
        }
        format!("{}{}", (size * 10.0).round() / 10.0, units[unit])
    }
}

struct Statements {
    driver: Driver,
    table: String,
    columns: String,
    buffer: Vec<String>,
}

impl Statements {
    fn push(&mut self, row: &[Scalar], file: &mut impl Write) -> anyhow::Result<()> {
        let values: Vec<String> = row.iter().map(|value| value.literal(self.driver)).collect();
        self.buffer.push(format!("({})", values.join(", ")));
        if self.buffer.len() >= PER_STATEMENT {
            self.flush(file)?;
        }
        Ok(())
    }

    fn flush(&mut self, file: &mut impl Write) -> anyhow::Result<()> {
        if self.buffer.is_empty() {
            return Ok(());
        }
        write!(
            file,
            "insert into {} ({}) values\n{};\n\n",
            self.table,
            self.columns,
            self.buffer.join(",\n")
        )?;
        self.buffer.clear();
        Ok(())
    }
}

pub struct Exporter<'a> {
    connection: Connection,
    store: Option<&'a Path>,
}

impl<'a> Exporter<'a> {
    pub fn new(connection: &Connection, store: Option<&'a Path>) -> Self {
        Exporter {
            connection: connection.clone(),
            store,
        }
    }

    pub fn directory(&self) -> anyhow::Result<PathBuf> {
        let configured = settings::with(|settings| settings.export_path.trim().to_string());
        let path = if !configured.is_empty() {
            PathBuf::from(paths::expand(&configured))
        } else if let Some(remembered) = self
            .remembered()
            .filter(|remembered| Path::new(remembered).is_dir())
        {
            PathBuf::from(remembered)
        } else {
            paths::config_directory().join("exports")
        };

        if !path.is_dir() {
            DirBuilder::new()
                .recursive(true)
                .mode(0o700)
                .create(&path)
                .with_context(|| format!("Could not make the folder [{}].", path.display()))?;
        }
        Ok(path)
    }

    pub fn filename(&self, table: Option<&Table>) -> anyhow::Result<PathBuf> {
        let database = self.connection.database.clone().unwrap_or_default();
        let subject = if self.connection.driver.is_file() {
            Path::new(&database)
                .file_stem()
                .map(|stem| stem.to_string_lossy().into_owned())
                .unwrap_or_default()
        } else {
            database
        };
        let subject = if subject.is_empty() {
            self.connection.name.clone()
        } else {
            subject
        };
        let part = table
            .map(|table| format!("{}-", slug(&table.to_string())))
            .unwrap_or_default();
        let stamp = now().strftime("%Y%m%d-%H%M%S");
        Ok(self
            .directory()?
            .join(format!("{}-{part}{stamp}.sql", slug(&subject))))
    }

    pub fn table(
        &self,
        database: &mut dyn Database,
        table: &Table,
        path: Option<PathBuf>,
        limit: Option<usize>,
    ) -> anyhow::Result<Exported> {
        let started = Instant::now();
        let path = match path {
            Some(path) => path,
            None => self.filename(Some(table))?,
        };
        let mut file = self.create(&path)?;
        file.write_all(self.header(&table.to_string()).as_bytes())?;
        let rows = self.write_table(database, table, limit, &mut file)?;
        file.flush()?;
        drop(file);

        self.remember(&path);
        Ok(Exported::of(path, rows, started))
    }

    pub fn tables(
        &self,
        database: &mut dyn Database,
        tables: &[Table],
        path: Option<PathBuf>,
        limit: Option<usize>,
    ) -> anyhow::Result<Exported> {
        let started = Instant::now();
        let path = match path {
            Some(path) => path,
            None => {
                let only = (tables.len() == 1).then(|| &tables[0]);
                self.filename(only)?
            }
        };
        let mut file = self.create(&path)?;
        let names: Vec<String> = tables.iter().map(Table::to_string).collect();
        file.write_all(self.header(&names.join(", ")).as_bytes())?;
        let mut rows = 0;
        for table in tables {
            writeln!(file, "-- {table}")?;
            rows += self.write_table(database, table, limit, &mut file)?;
        }
        file.flush()?;
        drop(file);

        self.remember(&path);
        Ok(Exported::of(path, rows, started))
    }

    pub fn rows(
        &self,
        database: &dyn Database,
        label: &str,
        headers: &[String],
        rows: &[Vec<Option<String>>],
        path: Option<PathBuf>,
    ) -> anyhow::Result<Exported> {
        let started = Instant::now();
        let path = match path {
            Some(path) => path,
            None => self.filename(Some(&Table::bare(label)))?,
        };
        let mut file = self.create(&path)?;
        file.write_all(self.header(label).as_bytes())?;

        let mut statements = Statements {
            driver: self.connection.driver,
            table: database.quote(label),
            columns: quoted_columns(database, headers),
            buffer: Vec::new(),
        };
        for row in rows {
            let values: Vec<Scalar> = row.iter().cloned().map(Scalar::text).collect();
            statements.push(&values, &mut file)?;
        }
        statements.flush(&mut file)?;
        file.flush()?;
        drop(file);

        self.remember(&path);
        Ok(Exported::of(path, rows.len(), started))
    }

    fn write_table(
        &self,
        database: &mut dyn Database,
        table: &Table,
        limit: Option<usize>,
        file: &mut impl Write,
    ) -> anyhow::Result<usize> {
        let key = database.key_columns()?.remove(table).unwrap_or_default();
        let order = match key.is_empty() {
            true => String::new(),
            false => format!(
                " order by {}",
                key.iter()
                    .map(|column| database.quote(column))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        };
        let mut statements = Statements {
            driver: self.connection.driver,
            table: table.quoted(|name| database.quote(name)),
            columns: String::new(),
            buffer: Vec::new(),
        };

        let mut written = 0;
        loop {
            let take = limit.map_or(CHUNK, |limit| CHUNK.min(limit.saturating_sub(written)));
            if take == 0 {
                break;
            }
            let page = database.scalars(&database.page(
                &format!("select * from {}{order}", statements.table),
                take,
                written,
            ))?;
            if page.rows.is_empty() {
                break;
            }
            if statements.columns.is_empty() {
                statements.columns = quoted_columns(database, &page.headers);
            }
            for row in &page.rows {
                statements.push(row, file)?;
                written += 1;
            }
        }
        statements.flush(file)?;
        Ok(written)
    }

    fn create(&self, path: &Path) -> anyhow::Result<BufWriter<File>> {
        let file = File::create(path)
            .with_context(|| format!("Could not write to [{}].", path.display()))?;
        Ok(BufWriter::new(file))
    }

    fn header(&self, table: &str) -> String {
        format!(
            "-- tql export\n-- connection: {} ({})\n-- table: {table}\n-- exported: {}\n\n",
            self.connection.name,
            self.connection.driver,
            now().strftime("%Y-%m-%dT%H:%M:%S%:z")
        )
    }

    fn remembered(&self) -> Option<String> {
        let store = rusqlite::Connection::open_with_flags(
            self.store?,
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
        )
        .ok()?;
        store
            .query_row(
                "select value from settings where key = ?1",
                [REMEMBERED],
                |row| row.get(0),
            )
            .ok()
            .flatten()
    }

    fn remember(&self, path: &Path) {
        let Some(store) = self.store else {
            return;
        };
        let Some(directory) = std::path::absolute(path)
            .ok()
            .and_then(|path| path.parent().map(Path::to_path_buf))
        else {
            return;
        };
        let directory = directory.display().to_string();
        if self.remembered().as_deref() == Some(directory.as_str()) {
            return;
        }
        let Ok(store) = rusqlite::Connection::open(store) else {
            return;
        };
        let _ = store.execute(
            "insert into settings (key, value, created_at, updated_at) \
             values (?1, ?2, datetime('now'), datetime('now')) \
             on conflict (key) do update set value = excluded.value, updated_at = excluded.updated_at",
            (REMEMBERED, directory),
        );
    }
}

fn quoted_columns(database: &dyn Database, headers: &[String]) -> String {
    headers
        .iter()
        .map(|header| database.quote(header))
        .collect::<Vec<_>>()
        .join(", ")
}

fn slug(text: &str) -> String {
    let slugged = NOT_A_WORD.replace_all(text, "-").to_lowercase();
    if slugged.is_empty() {
        return "export".to_string();
    }
    slugged
}

fn now() -> Zoned {
    Zoned::now().with_time_zone(TimeZone::UTC)
}

pub fn real_text(number: f64) -> String {
    let text = number.to_string();
    match number.is_finite() && !text.contains(['.', 'e', 'E']) {
        true => format!("{text}.0"),
        false => text,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quotes_text_the_way_each_driver_reads_it_back() {
        let text = Scalar::Text("it's a \\ \"line\"\nbreak".to_string());
        assert_eq!(text.literal(Driver::Sqlite), "'it''s a \\ \"line\"\nbreak'");
        assert_eq!(text.literal(Driver::Pgsql), "'it''s a \\ \"line\"\nbreak'");
        assert_eq!(
            text.literal(Driver::Mysql),
            "'it\\'s a \\\\ \\\"line\\\"\\nbreak'"
        );
    }

    #[test]
    fn writes_numbers_nulls_and_bytes_as_literals() {
        assert_eq!(Scalar::Null.literal(Driver::Sqlite), "null");
        assert_eq!(Scalar::real(1000.0).literal(Driver::Sqlite), "1000.0");
        assert_eq!(Scalar::real(0.15).literal(Driver::Sqlite), "0.15");
        assert_eq!(Scalar::real(f64::INFINITY).literal(Driver::Sqlite), "9e999");
        assert_eq!(Scalar::real(2.0).literal(Driver::Sqlite), "2.0");
        assert_eq!(Scalar::real(-0.5).literal(Driver::Sqlite), "-0.5");
        assert_eq!(
            Scalar::real(1e300)
                .literal(Driver::Sqlite)
                .parse::<f64>()
                .unwrap(),
            1e300
        );
        assert_eq!(
            Scalar::Bytes(vec![0, 255]).literal(Driver::Sqlite),
            "X'00ff'"
        );
        assert_eq!(
            Scalar::Bytes(vec![0, 255]).literal(Driver::Pgsql),
            "'\\x00ff'"
        );
    }

    #[test]
    fn sizes_read_like_tqls() {
        let sized = |bytes| Exported {
            path: PathBuf::new(),
            rows: 0,
            bytes,
            duration_ms: 0,
        };
        assert_eq!(sized(512).size(), "512B");
        assert_eq!(sized(1024).size(), "1KB");
        assert_eq!(sized(1536).size(), "1.5KB");
        assert_eq!(sized(3 * 1024 * 1024).size(), "3MB");
    }

    #[test]
    fn slugs_keep_letters_and_digits() {
        assert_eq!(slug("karlm_v2"), "karlm-v2");
        assert_eq!(slug("MySql Dev!"), "mysql-dev-");
        assert_eq!(slug("!!!"), "-");
        assert_eq!(slug(""), "export");
    }
}
