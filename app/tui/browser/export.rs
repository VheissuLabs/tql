use super::*;
use crate::database::{Exported, Exporter};

impl Browser {
    pub(super) fn export(&mut self) {
        match self.exported() {
            Ok(exported) => self.say(format!(
                "exported {} rows ({}) to {}",
                exported.rows,
                exported.size(),
                exported.path.display()
            )),
            Err(error) => self.say(format!("export failed: {error:#}")),
        }
    }

    pub(super) fn export_database(&mut self) {
        match self.exported_database() {
            Ok((exported, tables)) => self.say(format!(
                "exported {} rows from {tables} tables ({}) to {}",
                exported.rows,
                exported.size(),
                exported.path.display()
            )),
            Err(error) => self.say(format!("export failed: {error:#}")),
        }
    }

    fn exported_database(&mut self) -> anyhow::Result<(Exported, usize)> {
        let connection = self.exporting();
        let tables = self.database.tables()?;
        if tables.is_empty() {
            anyhow::bail!("this database has no tables");
        }
        let exporter = Exporter::new(&connection, self.store.as_deref());
        let exported = exporter.tables(self.database.as_mut(), &tables, None, None)?;
        Ok((exported, tables.len()))
    }

    fn exporting(&self) -> Connection {
        Connection {
            database: if !self.connection.driver.has_databases() {
                self.connection.database.clone()
            } else {
                self.active_database.clone()
            },
            ..self.connection.clone()
        }
    }

    fn exported(&mut self) -> anyhow::Result<Exported> {
        let connection = self.exporting();
        let exporter = Exporter::new(&connection, self.store.as_deref());
        if self.query.results {
            return exporter.rows(
                self.database.as_ref(),
                "results",
                &self.headers,
                &self.raw,
                None,
            );
        }
        let Some(table) = self.current_table() else {
            anyhow::bail!("there is no table open to export");
        };
        exporter.table(self.database.as_mut(), &table, None, None)
    }
}
