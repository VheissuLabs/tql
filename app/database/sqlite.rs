use super::Table;
use std::collections::HashMap;
use std::path::Path;

use anyhow::{Context, bail};
use regex::Regex;
use rusqlite::types::ValueRef;

use super::schema::keys_by_table;
use super::{
    Column, Database, Filters, ForeignKey, Generation, Index, Kind, Outcome, Page, Scalar, Scalars,
    Sort, Values, delete_statement, insert_statement, keyed, outcome_or_affected, primary_key,
    quote_double, refused, select_page, shown_delete, shown_insert, shown_update,
    single_column_keys, split_list, update_statement,
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

fn collated(created: &str, column: &str) -> Option<String> {
    let pattern = format!(
        r#"(?i)\b{}\b[^,(]+(?:\([^()]+\)[^,]*)?(?:(?:default|check|as)\s*(?:\(.*?\))?[^,]*)*collate\s+["'`]?(\w+)"#,
        regex::escape(column)
    );
    Regex::new(&pattern)
        .ok()?
        .captures(created)
        .map(|found| found[1].to_lowercase())
}

fn generated_as(created: &str, column: &str) -> Option<String> {
    let pattern = format!(
        r"(?i)\b{}\b[^,]+\s+as\s+\(((?:[^()]+|\((?:[^()]+|\([^()]*\))*\))*)\)",
        regex::escape(column)
    );
    Regex::new(&pattern)
        .ok()?
        .captures(created)
        .map(|found| found[1].to_string())
}

pub(crate) const TABLES: &str = "select name from pragma_table_list where type in ('table', 'virtual') \
     and name not like 'sqlite\\_%' escape '\\' order by schema, name";

pub(crate) const KEY_COLUMNS: &str = "select m.name, p.name from sqlite_master m join pragma_table_info(m.name) p \
     where m.type = 'table' and p.pk > 0 order by m.name, p.pk";

pub(crate) const COLUMNS: &str = "select name, type, not \"notnull\" as \"nullable\", dflt_value as \"default\", pk as \"primary\", \
     hidden as \"extra\" from pragma_table_xinfo(?1, 'main') order by cid asc";

pub(crate) const CREATED: &str = "select sql from sqlite_master where type = 'table' and name = ?1";

pub(crate) const INDEXES: &str = "select 'primary' as name, group_concat(col) as columns, 1 as \"unique\", 1 as \"primary\" \
     from (select name as col from pragma_table_xinfo(?1, 'main') where pk > 0 order by pk, cid) group by name \
     union select name, group_concat(col) as columns, \"unique\", origin = 'pk' as \"primary\" \
     from (select il.*, ii.name as col from pragma_index_list(?1, 'main') il, pragma_index_info(il.name, 'main') ii \
     order by il.seq, ii.seqno) group by name, \"unique\", \"primary\"";

pub(crate) const FOREIGN_KEYS: &str = "select owner, group_concat(\"from\"), \"table\", group_concat(\"to\") from ( \
     select m.name as owner, f.* from sqlite_master m join pragma_foreign_key_list(m.name) f \
     where m.type = 'table' order by m.name, f.id desc, f.seq) \
     group by owner, id, \"table\" order by owner, id";

pub(crate) fn columns_of(rows: Vec<Vec<Option<String>>>, created: &str) -> Vec<Column> {
    let keyed = rows
        .iter()
        .filter(|row| row[4].as_deref() != Some("0"))
        .count()
        == 1;

    rows.into_iter()
        .map(|mut row| {
            let declared = row[1].take().unwrap_or_default().to_lowercase();
            let primary = row[4].as_deref() != Some("0");
            let name = row[0].take().unwrap_or_default();
            let generation = match row[5].as_deref() {
                Some("2") => Some("virtual"),
                Some("3") => Some("stored"),
                _ => None,
            }
            .map(|kind| Generation {
                kind: Some(kind.to_string()),
                expression: generated_as(created, &name),
            });
            Column {
                type_name: declared
                    .trim_start_matches('(')
                    .split('(')
                    .next()
                    .unwrap_or_default()
                    .to_string(),
                nullable: row[2].as_deref() == Some("1"),
                default: row[3].take(),
                auto_increment: keyed && primary && declared == "integer",
                collation: collated(created, &name),
                comment: None,
                generation,
                full_type: declared,
                name,
            }
        })
        .collect()
}

pub(crate) fn indexes_of(rows: Vec<Vec<Option<String>>>) -> Vec<Index> {
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
        return indexes
            .into_iter()
            .filter(|index| index.name != "primary")
            .collect();
    }
    indexes
}

impl Database for Sqlite {
    fn tables(&mut self) -> anyhow::Result<Vec<Table>> {
        let mut statement = self.connection.prepare(TABLES)?;
        let names = statement.query_map([], |row| row.get::<_, String>(0).map(Table::bare))?;
        Ok(names.collect::<Result<_, _>>()?)
    }

    fn filtered_rows(
        &mut self,
        table: &Table,
        limit: usize,
        offset: usize,
        sort: Option<&Sort>,
        filters: Option<&Filters>,
    ) -> anyhow::Result<Page> {
        let condition = filters.and_then(|filters| filters.condition(&quote_double));
        let page = |shown: Option<String>| {
            select_page(
                &table.quoted(quote_double),
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
        let (headers, rows, kinds) = fetch(
            &mut self.connection.prepare(&sql)?,
            rusqlite::params_from_iter(bindings),
        )?;

        Ok(Page {
            statement: page(condition.as_ref().map(|condition| condition.readable())),
            headers,
            rows,
            kinds,
        })
    }

    fn run(&mut self, statement: &str) -> anyhow::Result<Outcome> {
        let mut prepared = self.connection.prepare(statement).map_err(plain)?;
        if prepared.column_count() == 0 {
            let affected = prepared.execute([]).map_err(plain)?;
            return Ok(outcome_or_affected(Vec::new(), Vec::new(), affected));
        }
        let (headers, rows, kinds) = fetch(&mut prepared, [])?;
        Ok(Outcome {
            kinds,
            ..outcome_or_affected(headers, rows, 0)
        })
    }

    fn run_reading(&mut self, statement: &str) -> anyhow::Result<Outcome> {
        self.connection.pragma_update(None, "query_only", true)?;
        let ran = self.run(statement);
        let _ = self.connection.pragma_update(None, "query_only", false);
        ran
    }

    fn scalars(&mut self, statement: &str) -> anyhow::Result<Scalars> {
        let mut prepared = self.connection.prepare(statement).map_err(plain)?;
        let headers: Vec<String> = prepared
            .column_names()
            .into_iter()
            .map(String::from)
            .collect();
        let width = headers.len();
        let rows = prepared
            .query_map([], |row| {
                (0..width)
                    .map(|index| Ok(scalar(row.get_ref(index)?)))
                    .collect()
            })?
            .collect::<Result<_, _>>()?;
        Ok(Scalars { headers, rows })
    }

    fn primary_keys(&mut self) -> anyhow::Result<HashMap<Table, String>> {
        Ok(single_column_keys(self.key_columns()?))
    }

    fn key_columns(&mut self) -> anyhow::Result<HashMap<Table, Vec<String>>> {
        let mut statement = self.connection.prepare(KEY_COLUMNS)?;
        let pairs = statement
            .query_map([], |row| {
                Ok((Table::bare(row.get::<_, String>(0)?), row.get(1)?))
            })?
            .collect::<Result<_, _>>()?;
        Ok(keyed(pairs))
    }

    fn select(&mut self, statement: &str, values: &[String]) -> anyhow::Result<Page> {
        let (headers, rows, kinds) = fetch(
            &mut self.connection.prepare(statement)?,
            rusqlite::params_from_iter(values),
        )?;
        Ok(Page {
            statement: statement.to_string(),
            headers,
            rows,
            kinds,
        })
    }

    fn columns(&mut self, table: &Table) -> anyhow::Result<Vec<Column>> {
        let rows = self
            .select(COLUMNS, std::slice::from_ref(&table.name))?
            .rows;
        let created = self
            .select(CREATED, std::slice::from_ref(&table.name))?
            .rows
            .into_iter()
            .next()
            .and_then(|mut row| row[0].take())
            .unwrap_or_default();
        Ok(columns_of(rows, &created))
    }

    fn indexes(&mut self, table: &Table) -> anyhow::Result<Vec<Index>> {
        let rows = self
            .select(INDEXES, std::slice::from_ref(&table.name))?
            .rows;
        Ok(indexes_of(rows))
    }

    fn foreign_keys(&mut self) -> anyhow::Result<HashMap<Table, Vec<ForeignKey>>> {
        let rows = self.select(FOREIGN_KEYS, &[])?.rows;

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
    fn highest(&mut self, table: &Table, column: &str) -> anyhow::Result<Option<String>> {
        let sql = format!(
            "select max({}) from {}",
            quote_double(column),
            table.quoted(quote_double)
        );
        Ok(self
            .connection
            .query_row(&sql, [], |row| Ok(text(row.get_ref(0)?)))?)
    }

    fn update(
        &mut self,
        table: &Table,
        column: &str,
        key: &str,
        key_value: &str,
        value: Option<&str>,
    ) -> anyhow::Result<u64> {
        let sql = update_statement(quote_double, numbered, table, column, key);
        let reached = self
            .connection
            .execute(&sql, rusqlite::params![value, key_value])
            .map_err(|error| {
                refused(
                    error,
                    &shown_update(quote_double, table, column, key, value, key_value),
                )
            })?;
        Ok(reached as u64)
    }

    fn insert(&mut self, table: &Table, values: &Values) -> anyhow::Result<()> {
        let sql = insert_statement(quote_double, numbered, table, values, "default values");
        self.connection
            .execute(
                &sql,
                rusqlite::params_from_iter(values.iter().map(|(_, value)| value)),
            )
            .map_err(|error| {
                refused(
                    error,
                    &shown_insert(quote_double, table, values, "default values"),
                )
            })?;
        Ok(())
    }

    fn delete(&mut self, table: &Table, key: &str, key_values: &[String]) -> anyhow::Result<u64> {
        let sql = delete_statement(quote_double, numbered, table, key, key_values.len());
        let reached = self
            .connection
            .execute(&sql, rusqlite::params_from_iter(key_values))
            .map_err(|error| refused(error, &shown_delete(quote_double, table, key, key_values)))?;
        Ok(reached as u64)
    }
}

fn numbered(position: usize) -> String {
    format!("?{position}")
}

fn plain(error: rusqlite::Error) -> anyhow::Error {
    match error {
        rusqlite::Error::SqlInputError { msg, .. } => anyhow::anyhow!(msg),
        rusqlite::Error::SqliteFailure(_, Some(msg)) => anyhow::anyhow!(msg),
        other => other.into(),
    }
}

type Rows = Vec<Vec<Option<String>>>;
type Kinds = Vec<Vec<Kind>>;

fn fetch(
    statement: &mut rusqlite::Statement,
    bindings: impl rusqlite::Params,
) -> anyhow::Result<(Vec<String>, Rows, Kinds)> {
    let headers: Vec<String> = statement
        .column_names()
        .into_iter()
        .map(String::from)
        .collect();
    let width = headers.len();

    let typed = statement.query_map(bindings, |row| {
        (0..width)
            .map(|index| {
                let value = row.get_ref(index)?;
                Ok((text(value), kind(value)))
            })
            .collect::<rusqlite::Result<Vec<_>>>()
    })?;
    let mut rows = Vec::new();
    let mut kinds = Vec::new();
    for row in typed {
        let (values, row_kinds) = row?.into_iter().unzip();
        rows.push(values);
        kinds.push(row_kinds);
    }
    Ok((headers, rows, kinds))
}

fn kind(value: ValueRef) -> Kind {
    match value {
        ValueRef::Integer(_) => Kind::Integer,
        ValueRef::Real(_) => Kind::Float,
        _ => Kind::Text,
    }
}

fn scalar(value: ValueRef) -> Scalar {
    match value {
        ValueRef::Null => Scalar::Null,
        ValueRef::Integer(number) => Scalar::Number(number.to_string()),
        ValueRef::Real(number) => Scalar::real(number),
        ValueRef::Text(bytes) => match std::str::from_utf8(bytes) {
            Ok(text) => Scalar::Text(text.to_string()),
            Err(_) => Scalar::Bytes(bytes.to_vec()),
        },
        ValueRef::Blob(bytes) => Scalar::Bytes(bytes.to_vec()),
    }
}

fn text(value: ValueRef) -> Option<String> {
    Some(match value {
        ValueRef::Null => return None,
        ValueRef::Integer(number) => number.to_string(),
        ValueRef::Real(number) => super::exporter::real_text(number),
        ValueRef::Text(bytes) => String::from_utf8_lossy(bytes).into_owned(),
        ValueRef::Blob(bytes) => format!("<{} bytes>", bytes.len()),
    })
}
