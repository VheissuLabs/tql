use std::collections::HashMap;
use std::path::Path;

use anyhow::{Context, bail};
use rusqlite::types::ValueRef;

use super::{Database, Page, Sort, quote_double, select_page, single_column_keys};
use crate::store::Connection;

pub struct Sqlite {
    connection: rusqlite::Connection,
}

impl Sqlite {
    pub fn open(connection: &Connection) -> anyhow::Result<Self> {
        let path = connection.database.as_deref().unwrap_or_default();
        if !Path::new(path).is_file() {
            bail!("no SQLite file at {path}");
        }
        Ok(Sqlite {
            connection: rusqlite::Connection::open(path)
                .with_context(|| format!("cannot open {path}"))?,
        })
    }
}

impl Database for Sqlite {
    fn tables(&mut self) -> anyhow::Result<Vec<String>> {
        let mut statement = self.connection.prepare(
            "select name from sqlite_master where type in ('table', 'view') and name not like 'sqlite_%' order by name",
        )?;
        let names = statement.query_map([], |row| row.get(0))?;
        Ok(names.collect::<Result<_, _>>()?)
    }

    fn rows(
        &mut self,
        table: &str,
        limit: usize,
        offset: usize,
        sort: Option<&Sort>,
    ) -> anyhow::Result<Page> {
        let sql = select_page(&quote_double(table), quote_double, limit, offset, sort);
        let mut statement = self.connection.prepare(&sql)?;
        let headers: Vec<String> = statement
            .column_names()
            .into_iter()
            .map(String::from)
            .collect();
        let width = headers.len();

        let rows = statement.query_map([], |row| {
            (0..width)
                .map(|index| {
                    Ok(match row.get_ref(index)? {
                        ValueRef::Null => None,
                        ValueRef::Integer(number) => Some(number.to_string()),
                        ValueRef::Real(number) => Some(number.to_string()),
                        ValueRef::Text(bytes) => Some(String::from_utf8_lossy(bytes).into_owned()),
                        ValueRef::Blob(bytes) => Some(format!("<{} bytes>", bytes.len())),
                    })
                })
                .collect()
        })?;

        Ok(Page {
            statement: sql.clone(),
            headers,
            rows: rows.collect::<Result<_, _>>()?,
        })
    }

    fn primary_keys(&mut self) -> anyhow::Result<HashMap<String, String>> {
        let mut statement = self.connection.prepare(
            "select m.name, p.name from sqlite_master m join pragma_table_info(m.name) p \
             where m.type = 'table' and p.pk > 0 order by m.name, p.pk",
        )?;
        let pairs = statement
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
            .collect::<Result<_, _>>()?;
        Ok(single_column_keys(pairs))
    }
}
