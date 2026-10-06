use std::collections::HashMap;
use std::path::Path;

use anyhow::{Context, bail};
use rusqlite::types::ValueRef;

use super::schema::keys_by_table;
use super::{
    Column, Database, Filters, ForeignKey, Index, Outcome, Page, Sort, outcome_or_affected,
    primary_key, quote_double, select_page, single_column_keys, split_list,
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

    fn select(&mut self, statement: &str, values: &[String]) -> anyhow::Result<Page> {
        let (headers, rows) = fetch(
            &mut self.connection.prepare(statement)?,
            rusqlite::params_from_iter(values),
        )?;
        Ok(Page {
            statement: statement.to_string(),
            headers,
            rows,
        })
    }

    fn columns(&mut self, table: &str) -> anyhow::Result<Vec<Column>> {
        let rows = self
            .select(
                "select name, type, not \"notnull\" as \"nullable\", dflt_value as \"default\", pk as \"primary\" \
                 from pragma_table_xinfo(?, 'main') order by cid asc",
                &[table.to_string()],
            )?
            .rows;
        let keyed = rows
            .iter()
            .filter(|row| row[4].as_deref() != Some("0"))
            .count()
            == 1;

        Ok(rows
            .into_iter()
            .map(|mut row| {
                let declared = row[1].take().unwrap_or_default().to_lowercase();
                let primary = row[4].as_deref() != Some("0");
                Column {
                    name: row[0].take().unwrap_or_default(),
                    type_name: declared
                        .trim_start_matches('(')
                        .split('(')
                        .next()
                        .unwrap_or_default()
                        .to_string(),
                    nullable: row[2].as_deref() == Some("1"),
                    default: row[3].take(),
                    auto_increment: keyed && primary && declared == "integer",
                }
            })
            .collect())
    }

    fn indexes(&mut self, table: &str) -> anyhow::Result<Vec<Index>> {
        let rows = self
            .select(
                "select 'primary' as name, group_concat(col) as columns, 1 as \"unique\", 1 as \"primary\" \
                 from (select name as col from pragma_table_xinfo(?1, 'main') where pk > 0 order by pk, cid) group by name \
                 union select name, group_concat(col) as columns, \"unique\", origin = 'pk' as \"primary\" \
                 from (select il.*, ii.name as col from pragma_index_list(?1, 'main') il, pragma_index_info(il.name, 'main') ii \
                 order by il.seq, ii.seqno) group by name, \"unique\", \"primary\"",
                &[table.to_string()],
            )?
            .rows;
        let indexes: Vec<Index> = rows
            .into_iter()
            .map(|mut row| Index {
                name: row[0].take().unwrap_or_default().to_lowercase(),
                columns: split_list(row[1].take()),
                unique: row[2].as_deref() == Some("1"),
                primary: row[3].as_deref() == Some("1"),
            })
            .collect();

        if indexes.iter().filter(|index| index.primary).count() > 1 {
            return Ok(indexes
                .into_iter()
                .filter(|index| index.name != "primary")
                .collect());
        }
        Ok(indexes)
    }

    fn foreign_keys(&mut self) -> anyhow::Result<HashMap<String, Vec<ForeignKey>>> {
        let rows = self
            .select(
                "select owner, group_concat(\"from\"), \"table\", group_concat(\"to\") from ( \
                 select m.name as owner, f.* from sqlite_master m join pragma_foreign_key_list(m.name) f \
                 where m.type = 'table' order by m.name, f.id desc, f.seq) \
                 group by owner, id, \"table\" order by owner, id",
                &[],
            )?
            .rows;

        let mut keys = keys_by_table(rows);
        for key in keys.values_mut().flatten() {
            if key.references.is_empty() {
                key.references = primary_key(&self.indexes(&key.table)?)
                    .into_iter()
                    .collect();
            }
        }
        Ok(keys)
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
