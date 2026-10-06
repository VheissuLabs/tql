use std::collections::HashMap;
use std::path::Path;

use anyhow::{Context, bail};
use rusqlite::types::ValueRef;

use super::{
    Column, Database, Page, Sort, Values, delete_statement, insert_statement, quote_double,
    refused, select_page, single_column_keys, update_statement,
};
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

    fn columns(&mut self, table: &str) -> anyhow::Result<Vec<Column>> {
        let mut statement = self.connection.prepare(
            "select name, type, not \"notnull\", dflt_value, pk from pragma_table_xinfo(?1) order by cid",
        )?;
        let described: Vec<(String, String, bool, Option<String>, i64)> = statement
            .query_map([table], |row| {
                Ok((
                    row.get(0)?,
                    row.get::<_, Option<String>>(1)?.unwrap_or_default(),
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                ))
            })?
            .collect::<Result<_, _>>()?;
        let single_key = described.iter().filter(|column| column.4 > 0).count() == 1;

        Ok(described
            .into_iter()
            .map(|(name, declared, nullable, default, primary)| {
                let declared = declared.to_lowercase();
                Column {
                    name,
                    type_name: declared.split('(').next().unwrap_or_default().to_string(),
                    nullable,
                    default,
                    auto_increment: single_key && primary > 0 && declared == "integer",
                }
            })
            .collect())
    }

    fn highest(&mut self, table: &str, column: &str) -> anyhow::Result<Option<String>> {
        let sql = format!(
            "select max({}) from {}",
            quote_double(column),
            quote_double(table)
        );
        let highest: Option<i64> = self
            .connection
            .query_row(&sql, [], |row| row.get(0))
            .unwrap_or_default();
        Ok(highest.map(|number| number.to_string()))
    }

    fn update(
        &mut self,
        table: &str,
        column: &str,
        key: &str,
        key_value: &str,
        value: Option<&str>,
    ) -> anyhow::Result<()> {
        let sql = update_statement(quote_double, numbered, table, column, key);
        self.connection
            .execute(&sql, rusqlite::params![value, key_value])
            .map_err(|error| refused(error, &sql))?;
        Ok(())
    }

    fn insert(&mut self, table: &str, values: &Values) -> anyhow::Result<()> {
        let sql = insert_statement(quote_double, numbered, table, values, "default values");
        self.connection
            .execute(
                &sql,
                rusqlite::params_from_iter(values.iter().map(|(_, value)| value)),
            )
            .map_err(|error| refused(error, &sql))?;
        Ok(())
    }

    fn delete(&mut self, table: &str, key: &str, key_values: &[String]) -> anyhow::Result<()> {
        let sql = delete_statement(quote_double, numbered, table, key, key_values.len());
        self.connection
            .execute(&sql, rusqlite::params_from_iter(key_values))
            .map_err(|error| refused(error, &sql))?;
        Ok(())
    }
}

fn numbered(position: usize) -> String {
    format!("?{position}")
}
