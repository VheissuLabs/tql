use std::collections::HashMap;
use std::path::Path;

use anyhow::{Context, bail};
use rusqlite::types::ValueRef;

use super::{
    Column, Database, Filters, Outcome, Page, Sort, Values, delete_statement, insert_statement,
    outcome_or_affected, quote_double, refused, select_page, single_column_keys, update_statement,
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

    fn filtered_rows(
        &mut self,
        table: &str,
        limit: usize,
        offset: usize,
        sort: Option<&Sort>,
        filters: Option<&Filters>,
    ) -> anyhow::Result<Page> {
        let condition = filters.and_then(|filters| filters.condition(&quote_double));
        let page = |shown: Option<String>| {
            select_page(
                &quote_double(table),
                quote_double,
                limit,
                offset,
                sort,
                shown.as_deref(),
            )
        };
        let sql = page(condition.as_ref().map(|condition| condition.placeheld()));
        let bindings = condition
            .as_ref()
            .map(|condition| condition.bindings())
            .unwrap_or_default();
        let (headers, rows) = fetch(
            &mut self.connection.prepare(&sql)?,
            rusqlite::params_from_iter(bindings),
        )?;

        Ok(Page {
            statement: page(condition.as_ref().map(|condition| condition.readable())),
            headers,
            rows,
        })
    }

    fn run(&mut self, statement: &str) -> anyhow::Result<Outcome> {
        let mut prepared = self.connection.prepare(statement).map_err(plain)?;
        if prepared.column_count() == 0 {
            let affected = prepared.execute([])?;
            return Ok(outcome_or_affected(Vec::new(), Vec::new(), affected));
        }
        let (headers, rows) = fetch(&mut prepared, [])?;
        Ok(outcome_or_affected(headers, rows, 0))
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

fn plain(error: rusqlite::Error) -> anyhow::Error {
    match error {
        rusqlite::Error::SqlInputError { msg, .. } => anyhow::anyhow!(msg),
        other => other.into(),
    }
}

type Rows = Vec<Vec<Option<String>>>;

fn fetch(
    statement: &mut rusqlite::Statement,
    bindings: impl rusqlite::Params,
) -> anyhow::Result<(Vec<String>, Rows)> {
    let headers: Vec<String> = statement
        .column_names()
        .into_iter()
        .map(String::from)
        .collect();
    let width = headers.len();

    let rows = statement.query_map(bindings, |row| {
        (0..width)
            .map(|index| Ok(text(row.get_ref(index)?)))
            .collect()
    })?;
    Ok((headers, rows.collect::<Result<_, _>>()?))
}

fn text(value: ValueRef) -> Option<String> {
    Some(match value {
        ValueRef::Null => return None,
        ValueRef::Integer(number) => number.to_string(),
        ValueRef::Real(number) => number.to_string(),
        ValueRef::Text(bytes) => String::from_utf8_lossy(bytes).into_owned(),
        ValueRef::Blob(bytes) => format!("<{} bytes>", bytes.len()),
    })
}
