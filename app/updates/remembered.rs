use std::path::{Path, PathBuf};

use rusqlite::{Connection, OpenFlags};

pub const CHECKED: &str = "updates.checked_at";

pub const LATEST: &str = "updates.latest";

pub const INSTALLED: &str = "updates.installed";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Remembered {
    store: PathBuf,
}

impl Remembered {
    pub fn new(store: &Path) -> Self {
        Remembered {
            store: store.to_path_buf(),
        }
    }

    pub fn read(&self, key: &str) -> Option<String> {
        Connection::open_with_flags(&self.store, OpenFlags::SQLITE_OPEN_READ_ONLY)
            .ok()?
            .query_row("select value from settings where key = ?1", [key], |row| {
                row.get(0)
            })
            .ok()
            .flatten()
    }

    pub fn write(&self, key: &str, value: &str) {
        if !self.store.is_file() {
            return;
        }
        let Ok(store) = Connection::open(&self.store) else {
            return;
        };
        let _ = store.execute(
            "insert into settings (key, value, created_at, updated_at) \
             values (?1, ?2, datetime('now'), datetime('now')) \
             on conflict (key) do update set value = excluded.value, updated_at = excluded.updated_at",
            (key, value),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store_writer::StoreWriter;

    #[test]
    fn remembers_between_runs_and_never_breaks_without_a_store() {
        let directory = std::env::temp_dir().join(format!("tql-remembered-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&directory);
        let missing = Remembered::new(&directory.join("missing.sqlite"));
        missing.write(LATEST, "0.8.0");

        StoreWriter::prepare(&directory.join("tql.sqlite")).unwrap();
        let remembered = Remembered::new(&directory.join("tql.sqlite"));
        remembered.write(LATEST, "0.8.0");
        remembered.write(LATEST, "0.8.1");

        assert_eq!(missing.read(LATEST), None);
        assert!(!directory.join("missing.sqlite").exists());
        assert_eq!(remembered.read(LATEST).as_deref(), Some("0.8.1"));
        assert_eq!(remembered.read(CHECKED), None);
        std::fs::remove_dir_all(&directory).unwrap();
    }
}
