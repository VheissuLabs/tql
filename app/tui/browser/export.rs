use super::*;
use crate::database::{Exported, Exporter};
use crate::driver::Driver;

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

    fn exported(&mut self) -> anyhow::Result<Exported> {
        let connection = Connection {
            database: if self.connection.driver == Driver::Sqlite {
                self.connection.database.clone()
            } else {
                self.active_database.clone()
            },
            ..self.connection.clone()
        };
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
