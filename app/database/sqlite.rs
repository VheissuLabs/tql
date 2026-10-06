use std::collections::HashMap;
use std::path::Path;

use anyhow::{Context, bail};
use rusqlite::types::ValueRef;

use super::{
    Database, Filters, Outcome, Page, Sort, outcome_or_affected, quote_double, select_page,
    single_column_keys,
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
