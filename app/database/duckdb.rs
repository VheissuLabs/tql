use std::collections::HashMap;
use std::path::Path;

use anyhow::bail;
use duckdb::types::{TimeUnit, Value};
use jiff::Timestamp;
use serde_json::{Map, Number, json};

use super::lexed::writing_word;
use super::{
    Column, Database, Filters, ForeignKey, Index, Kind, Outcome, Page, Scalar, Scalars, Sort,
    Table, Values, delete_statement, insert_statement, is_read_only, keyed, outcome_or_affected,
    quote_double, refused, select_page, shown_delete, shown_insert, shown_update,
    single_column_keys, update_statement,
};
use crate::store::Connection;

const SCHEMA_OR_DEFAULT: &str =
    "case when table_schema = current_schema() then null else table_schema end";

pub struct Duck {
    connection: duckdb::Connection,
    default: Option<String>,
}

type Rows = Vec<Vec<Option<String>>>;
type Kinds = Vec<Vec<Kind>>;

impl Duck {
    pub fn open(connection: &Connection) -> anyhow::Result<Self> {
        let path = connection.database.as_deref().unwrap_or_default();
        if !Path::new(path).is_file() {
            bail!("no DuckDB file at {path}");
        }
        let opened = duckdb::Connection::open(path).map_err(|error| {
            let said = error.to_string();
            match said.to_lowercase().contains("lock") {
                true => anyhow::anyhow!("{path} is in use by another program: {said}"),
                false => anyhow::anyhow!("cannot open {path}: {said}"),
            }
        })?;
        Ok(Duck {
            connection: opened,
            default: None,
        })
    }

    fn fetch(&self, sql: &str, values: &[String]) -> anyhow::Result<(Vec<String>, Rows, Kinds)> {
        let mut statement = self.connection.prepare(sql).map_err(plain)?;
        let mut found = statement
            .query(duckdb::params_from_iter(values))
            .map_err(plain)?;
        let headers: Vec<String> = found
            .as_ref()
            .map(|statement| statement.column_names())
            .unwrap_or_default();
        let width = headers.len();
        let mut rows = Vec::new();
        let mut kinds = Vec::new();
        while let Some(row) = found.next().map_err(plain)? {
            let mut shown = Vec::with_capacity(width);
            let mut kind = Vec::with_capacity(width);
            for index in 0..width {
                let value: Value = row.get(index).map_err(plain)?;
                kind.push(kind_of(&value));
                shown.push(text(&value));
            }
            rows.push(shown);
            kinds.push(kind);
        }
        Ok((headers, rows, kinds))
    }

    fn page(&self, sql: &str, values: &[String]) -> anyhow::Result<Page> {
        let (headers, rows, kinds) = self.fetch(sql, values)?;
        Ok(Page {
            statement: sql.to_string(),
            headers,
            rows,
            kinds,
        })
    }

    fn schema_of(&mut self, table: &Table) -> String {
        table
            .schema
            .clone()
            .or_else(|| self.default_schema())
            .unwrap_or_else(|| "main".to_string())
    }

    fn write(&self, sql: &str, values: &[Option<&str>], shown: String) -> anyhow::Result<u64> {
        self.connection
            .execute(sql, duckdb::params_from_iter(values))
            .map(|reached| reached as u64)
            .map_err(|error| refused(error, &shown))
    }
}

fn plain(error: duckdb::Error) -> anyhow::Error {
    anyhow::anyhow!(error.to_string())
}

fn dollar(position: usize) -> String {
    format!("${position}")
}

fn kind_of(value: &Value) -> Kind {
    match value {
        Value::TinyInt(_)
        | Value::SmallInt(_)
        | Value::Int(_)
        | Value::BigInt(_)
        | Value::UTinyInt(_)
        | Value::USmallInt(_)
        | Value::UInt(_)
        | Value::UBigInt(_) => Kind::Integer,
        Value::Float(_) | Value::Double(_) => Kind::Float,
        Value::Boolean(_) => Kind::Boolean,
        _ => Kind::Text,
    }
}

fn text(value: &Value) -> Option<String> {
    Some(match value {
        Value::Null => return None,
        Value::Text(text) | Value::Enum(text) => text.clone(),
        Value::Float(number) => super::exporter::real_text(f64::from(*number)),
        Value::Double(number) => super::exporter::real_text(*number),
        Value::Blob(bytes) | Value::Geometry(bytes) => format!("<{} bytes>", bytes.len()),
        Value::Date32(_) | Value::Timestamp(..) | Value::Time64(..) => moment(value),
        Value::List(_) | Value::Array(_) | Value::Struct(_) | Value::Map(_) => {
            jsoned(value).to_string()
        }
        Value::Union(inner) => return text(inner),
        other => scalar_text(other),
    })
}

fn scalar_text(value: &Value) -> String {
    match value {
        Value::Boolean(flag) => flag.to_string(),
        Value::TinyInt(number) => number.to_string(),
        Value::SmallInt(number) => number.to_string(),
        Value::Int(number) => number.to_string(),
        Value::BigInt(number) => number.to_string(),
        Value::HugeInt(number) => number.to_string(),
        Value::UHugeInt(number) => number.to_string(),
        Value::UTinyInt(number) => number.to_string(),
        Value::USmallInt(number) => number.to_string(),
        Value::UInt(number) => number.to_string(),
        Value::UBigInt(number) => number.to_string(),
        Value::Decimal(number) => number.to_string(),
        Value::Interval {
            months,
            days,
            nanos,
        } => format!("{months} months {days} days {}s", *nanos as f64 / 1e9),
        _ => String::new(),
    }
}

fn nanos(unit: &TimeUnit, amount: i64) -> i128 {
    let amount = i128::from(amount);
    match unit {
        TimeUnit::Second => amount * 1_000_000_000,
        TimeUnit::Millisecond => amount * 1_000_000,
        TimeUnit::Microsecond => amount * 1_000,
        TimeUnit::Nanosecond => amount,
    }
}

fn moment(value: &Value) -> String {
    match value {
        Value::Date32(days) => Timestamp::from_second(i64::from(*days) * 86_400)
            .map(|at| at.strftime("%Y-%m-%d").to_string())
            .unwrap_or_else(|_| days.to_string()),
        Value::Timestamp(unit, amount) => {
            let total = nanos(unit, *amount);
            Timestamp::from_nanosecond(total)
                .map(|at| {
                    let shown = at.strftime("%Y-%m-%d %H:%M:%S").to_string();
                    match total.rem_euclid(1_000_000_000) {
                        0 => shown,
                        fraction => format!("{shown}.{:06}", fraction / 1_000),
                    }
                })
                .unwrap_or_else(|_| amount.to_string())
        }
        Value::Time64(unit, amount) => {
            let total = nanos(unit, *amount);
            let seconds = total / 1_000_000_000;
            let shown = format!(
                "{:02}:{:02}:{:02}",
                seconds / 3600,
                seconds / 60 % 60,
                seconds % 60
            );
            match total % 1_000_000_000 {
                0 => shown,
                fraction => format!("{shown}.{:06}", fraction / 1_000),
            }
        }
        _ => String::new(),
    }
}

fn jsoned(value: &Value) -> serde_json::Value {
    match value {
        Value::Null => serde_json::Value::Null,
        Value::Boolean(flag) => json!(flag),
        Value::TinyInt(number) => json!(number),
        Value::SmallInt(number) => json!(number),
        Value::Int(number) => json!(number),
        Value::BigInt(number) => json!(number),
        Value::UTinyInt(number) => json!(number),
        Value::USmallInt(number) => json!(number),
        Value::UInt(number) => json!(number),
        Value::UBigInt(number) => json!(number),
        Value::Double(number) => Number::from_f64(*number)
            .map(serde_json::Value::Number)
            .unwrap_or(serde_json::Value::Null),
        Value::Float(number) => Number::from_f64(f64::from(*number))
            .map(serde_json::Value::Number)
            .unwrap_or(serde_json::Value::Null),
        Value::List(items) | Value::Array(items) => {
            serde_json::Value::Array(items.iter().map(jsoned).collect())
        }
        Value::Struct(fields) => serde_json::Value::Object(
            fields
                .iter()
                .map(|(name, value)| (name.clone(), jsoned(value)))
                .collect::<Map<_, _>>(),
        ),
        Value::Map(pairs) => serde_json::Value::Object(
            pairs
                .iter()
                .map(|(key, value)| (text(key).unwrap_or_default(), jsoned(value)))
                .collect::<Map<_, _>>(),
        ),
        Value::Union(inner) => jsoned(inner),
        other => text(other)
            .map(serde_json::Value::String)
            .unwrap_or(serde_json::Value::Null),
    }
}

fn scalar_of(value: &Value) -> Scalar {
    match value {
        Value::Null => Scalar::Null,
        Value::Float(number) => Scalar::real(f64::from(*number)),
        Value::Double(number) => Scalar::real(*number),
        Value::Blob(bytes) => Scalar::Bytes(bytes.clone()),
        Value::Boolean(flag) => Scalar::Number(u8::from(*flag).to_string()),
        value
            if kind_of(value) == Kind::Integer
                || matches!(
                    value,
                    Value::HugeInt(_) | Value::UHugeInt(_) | Value::Decimal(_)
                ) =>
        {
            Scalar::Number(text(value).unwrap_or_default())
        }
        other => Scalar::Text(text(other).unwrap_or_default()),
    }
}

impl Database for Duck {
    fn tables(&mut self) -> anyhow::Result<Vec<Table>> {
        let (_, rows, _) = self.fetch(
            &format!(
                "select {SCHEMA_OR_DEFAULT}, table_name from information_schema.tables \
                 where table_type = 'BASE TABLE' and table_catalog = current_database() \
                 order by table_schema <> current_schema(), table_schema, table_name"
            ),
            &[],
        )?;
        Ok(rows
            .into_iter()
            .filter_map(|mut row| {
                let name = row[1].take()?;
                Some(Table {
                    schema: row[0].take(),
                    name,
                })
            })
            .collect())
    }

    fn default_schema(&mut self) -> Option<String> {
        if self.default.is_none() {
            self.default = self
                .fetch("select current_schema()", &[])
                .ok()
                .and_then(|(_, mut rows, _)| rows.pop()?.pop()?);
        }
        self.default.clone()
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
        let (headers, rows, kinds) = self.fetch(&sql, &bindings)?;
        Ok(Page {
            statement: page(condition.as_ref().map(|condition| condition.readable())),
            headers,
            rows,
            kinds,
        })
    }

    fn run(&mut self, statement: &str) -> anyhow::Result<Outcome> {
        if !is_read_only(statement) {
            let affected = self.connection.execute(statement, []).map_err(plain)?;
            self.default = None;
            return Ok(outcome_or_affected(Vec::new(), Vec::new(), affected));
        }
        let (headers, rows, kinds) = self.fetch(statement, &[])?;
        Ok(Outcome {
            kinds,
            ..outcome_or_affected(headers, rows, 0)
        })
    }

    fn run_reading(&mut self, statement: &str) -> anyhow::Result<Outcome> {
        if let Some(word) = writing_word(statement) {
            bail!(
                "that statement has {word} in it, so tql did not run it; only statements that read run here"
            );
        }
        self.connection
            .execute_batch("begin transaction")
            .map_err(plain)?;
        let ran = self
            .fetch(statement, &[])
            .map(|(headers, rows, kinds)| Outcome {
                kinds,
                ..outcome_or_affected(headers, rows, 0)
            });
        let _ = self.connection.execute_batch("rollback");
        ran
    }

    fn scalars(&mut self, statement: &str) -> anyhow::Result<Scalars> {
        let mut prepared = self.connection.prepare(statement).map_err(plain)?;
        let mut found = prepared.query([]).map_err(plain)?;
        let headers: Vec<String> = found
            .as_ref()
            .map(|statement| statement.column_names())
            .unwrap_or_default();
        let mut rows = Vec::new();
        while let Some(row) = found.next().map_err(plain)? {
            let mut scalars = Vec::with_capacity(headers.len());
            for index in 0..headers.len() {
                let value: Value = row.get(index).map_err(plain)?;
                scalars.push(scalar_of(&value));
            }
            rows.push(scalars);
        }
        Ok(Scalars { headers, rows })
    }

    fn primary_keys(&mut self) -> anyhow::Result<HashMap<Table, String>> {
        Ok(single_column_keys(self.key_columns()?))
    }

    fn key_columns(&mut self) -> anyhow::Result<HashMap<Table, Vec<String>>> {
        let (_, rows, _) = self.fetch(
            "select case when schema_name = current_schema() then null else schema_name end, \
             table_name, unnest(constraint_column_names) from duckdb_constraints() \
             where constraint_type = 'PRIMARY KEY' and database_name = current_database()",
            &[],
        )?;
        Ok(keyed(
            rows.into_iter()
                .filter_map(|mut row| {
                    let name = row[1].take()?;
                    let column = row[2].take()?;
                    Some((
                        Table {
                            schema: row[0].take(),
                            name,
                        },
                        column,
                    ))
                })
                .collect(),
        ))
    }

    fn select(&mut self, statement: &str, values: &[String]) -> anyhow::Result<Page> {
        self.page(statement, values)
    }

    fn columns(&mut self, table: &Table) -> anyhow::Result<Vec<Column>> {
        let schema = self.schema_of(table);
        let (_, rows, _) = self.fetch(
            "select column_name, data_type, is_nullable = 'YES', column_default \
             from information_schema.columns where table_schema = ? and table_name = ? \
             and table_catalog = current_database() order by ordinal_position",
            &[schema, table.name.clone()],
        )?;
        Ok(rows
            .into_iter()
            .map(|mut row| {
                let full_type = row[1].take().unwrap_or_default();
                let name = row[0].take().unwrap_or_default();
                let default = row[3].take();
                Column {
                    type_name: full_type
                        .split('(')
                        .next()
                        .unwrap_or_default()
                        .to_lowercase(),
                    nullable: row[2].as_deref() == Some("true"),
                    auto_increment: default
                        .as_deref()
                        .is_some_and(|default| default.starts_with("nextval(")),
                    default,
                    full_type: full_type.to_lowercase(),
                    collation: None,
                    comment: None,
                    generation: None,
                    name,
                }
            })
            .collect())
    }

    fn indexes(&mut self, table: &Table) -> anyhow::Result<Vec<Index>> {
        let schema = self.schema_of(table);
        let (_, rows, _) = self.fetch(
            "select constraint_type, array_to_string(constraint_column_names, ',') \
             from duckdb_constraints() where schema_name = ? and table_name = ? \
             and database_name = current_database() \
             and constraint_type in ('PRIMARY KEY', 'UNIQUE')",
            &[schema, table.name.clone()],
        )?;
        Ok(rows
            .into_iter()
            .map(|mut row| {
                let primary = row[0].as_deref() == Some("PRIMARY KEY");
                let columns = super::split_list(row[1].take());
                Index {
                    name: match primary {
                        true => "primary".to_string(),
                        false => format!("unique_{}", columns.join("_")),
                    },
                    columns,
                    unique: true,
                    primary,
                }
            })
            .collect())
    }

    fn foreign_keys(&mut self) -> anyhow::Result<HashMap<Table, Vec<ForeignKey>>> {
        let rows = self
            .fetch(
                "select case when schema_name = current_schema() then null else schema_name end, \
                 table_name, array_to_string(constraint_column_names, ','), \
                 case when schema_name = current_schema() then null else schema_name end, \
                 referenced_table, array_to_string(referenced_column_names, ',') \
                 from duckdb_constraints() where constraint_type = 'FOREIGN KEY' \
                 and database_name = current_database()",
                &[],
            )
            .map(|(_, rows, _)| rows)
            .unwrap_or_default();
        Ok(super::schema::keys_by_schema(rows))
    }

    fn highest(&mut self, table: &Table, column: &str) -> anyhow::Result<Option<String>> {
        let (_, rows, _) = self.fetch(
            &format!(
                "select max({}) from {}",
                quote_double(column),
                table.quoted(quote_double)
            ),
            &[],
        )?;
        Ok(rows
            .into_iter()
            .next()
            .and_then(|mut row| row.swap_remove(0)))
    }

    fn update(
        &mut self,
        table: &Table,
        column: &str,
        key: &str,
        key_value: &str,
        value: Option<&str>,
    ) -> anyhow::Result<u64> {
        let sql = update_statement(quote_double, dollar, table, column, key);
        self.write(
            &sql,
            &[value, Some(key_value)],
            shown_update(quote_double, table, column, key, value, key_value),
        )
    }

    fn insert(&mut self, table: &Table, values: &Values) -> anyhow::Result<()> {
        let sql = insert_statement(quote_double, dollar, table, values, "default values");
        let bound: Vec<Option<&str>> = values.iter().map(|(_, value)| value.as_deref()).collect();
        self.write(
            &sql,
            &bound,
            shown_insert(quote_double, table, values, "default values"),
        )
        .map(|_| ())
    }

    fn delete(&mut self, table: &Table, key: &str, key_values: &[String]) -> anyhow::Result<u64> {
        let sql = delete_statement(quote_double, dollar, table, key, key_values.len());
        let bound: Vec<Option<&str>> = key_values
            .iter()
            .map(|value| Some(value.as_str()))
            .collect();
        self.write(
            &sql,
            &bound,
            shown_delete(quote_double, table, key, key_values),
        )
    }
}
