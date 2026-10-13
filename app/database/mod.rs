pub mod clickhouse;
mod column_default;
mod duckdb;
mod exporter;
mod filters;
mod http;
mod lexed;
mod libsql;
mod mysql;
pub mod order_by;
mod postgres;
mod schema;
mod sqlite;
mod sqlsrv;
mod table;
pub mod tls;

use std::collections::HashMap;
use std::sync::LazyLock;

use anyhow::{anyhow, bail};
use regex::Regex;

pub use column_default::ColumnDefault;
pub use exporter::{Exported, Exporter, Scalar, Scalars, real_text as exporter_real_text};
pub use table::{Table, split_name};

use crate::driver::Driver;
use crate::encrypter::Encrypter;
use crate::store::Connection;
use crate::tunnel;

pub use filters::{Condition, Filter, Filters, Joiner, OPERATORS, operator_names, readable_value};
pub use schema::{
    BackLink, Column, Count, ForeignKey, Generation, Index, Link, Pivot, Relations, is_plumbing,
    links, primary_key, unique_columns,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    Asc,
    Desc,
}

impl Direction {
    pub fn keyword(self) -> &'static str {
        match self {
            Direction::Asc => "asc",
            Direction::Desc => "desc",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sort {
    pub column: String,
    pub direction: Direction,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Kind {
    #[default]
    Text,
    Integer,
    Float,
    Boolean,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Page {
    pub statement: String,
    pub headers: Vec<String>,
    pub rows: Vec<Vec<Option<String>>>,
    pub kinds: Vec<Vec<Kind>>,
}

pub type Values = Vec<(String, Option<String>)>;

pub trait Named {
    fn named(&self, name: &str) -> Option<&Option<String>>;
    fn put(&mut self, name: &str, value: Option<String>);
    fn forget(&mut self, name: &str);
}

impl Named for Values {
    fn named(&self, name: &str) -> Option<&Option<String>> {
        self.iter()
            .find(|(column, _)| column == name)
            .map(|(_, value)| value)
    }

    fn put(&mut self, name: &str, value: Option<String>) {
        match self.iter_mut().find(|(column, _)| column == name) {
            Some((_, held)) => *held = value,
            None => self.push((name.to_string(), value)),
        }
    }

    fn forget(&mut self, name: &str) {
        self.retain(|(column, _)| column != name);
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Outcome {
    pub headers: Vec<String>,
    pub rows: Vec<Vec<Option<String>>>,
    pub kinds: Vec<Vec<Kind>>,
    pub affected: Option<usize>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Total {
    Exact(usize),
    About(usize),
}

impl std::fmt::Display for Total {
    fn fmt(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
        match *self {
            Total::Exact(rows) => formatter.write_str(&grouped(rows)),
            Total::About(rows) => {
                let (scale, suffix) = match rows {
                    1_000_000_000.. => (1e9, "B"),
                    1_000_000.. => (1e6, "M"),
                    1_000.. => (1e3, "K"),
                    _ => return write!(formatter, "~{rows}"),
                };
                let scaled = rows as f64 / scale;
                if scaled >= 10.0 {
                    write!(formatter, "~{scaled:.0}{suffix}")
                } else {
                    write!(formatter, "~{scaled:.1}{suffix}")
                }
            }
        }
    }
}

pub fn grouped(number: usize) -> String {
    let digits = number.to_string();
    let mut grouped = String::new();
    for (at, digit) in digits.chars().enumerate() {
        if at > 0 && (digits.len() - at).is_multiple_of(3) {
            grouped.push(',');
        }
        grouped.push(digit);
    }
    grouped
}

fn first_count(page: Page) -> Option<usize> {
    page.rows
        .into_iter()
        .next()?
        .into_iter()
        .next()
        .flatten()?
        .parse()
        .ok()
}

pub trait Database {
    fn tables(&mut self) -> anyhow::Result<Vec<Table>>;

    fn default_schema(&mut self) -> Option<String> {
        None
    }

    fn rows(
        &mut self,
        table: &Table,
        limit: usize,
        offset: usize,
        sort: Option<&Sort>,
    ) -> anyhow::Result<Page> {
        self.filtered_rows(table, limit, offset, sort, None)
    }

    fn filtered_rows(
        &mut self,
        table: &Table,
        limit: usize,
        offset: usize,
        sort: Option<&Sort>,
        filters: Option<&Filters>,
    ) -> anyhow::Result<Page>;

    fn estimated_rows(&mut self, _table: &Table) -> Option<Total> {
        None
    }

    fn paging_order(&mut self, _table: &Table) -> Option<String> {
        None
    }

    fn count_rows(&mut self, table: &Table, filters: Option<&Filters>) -> anyhow::Result<usize> {
        let condition = filters.and_then(|filters| filters.condition(&|name| self.quote(name)));
        let statement = format!(
            "select count(*) from {}{}",
            table.quoted(|name| self.quote(name)),
            condition
                .as_ref()
                .map(|condition| format!(" where {}", condition.placeheld()))
                .unwrap_or_default()
        );
        let values = condition
            .map(|condition| condition.bindings())
            .unwrap_or_default();
        first_count(self.select(&statement, &values)?)
            .ok_or_else(|| anyhow!("the count came back empty"))
    }

    fn primary_keys(&mut self) -> anyhow::Result<HashMap<Table, String>>;

    fn key_columns(&mut self) -> anyhow::Result<HashMap<Table, Vec<String>>> {
        Ok(self
            .primary_keys()?
            .into_iter()
            .map(|(table, column)| (table, vec![column]))
            .collect())
    }

    fn select(&mut self, statement: &str, values: &[String]) -> anyhow::Result<Page>;

    fn columns(&mut self, table: &Table) -> anyhow::Result<Vec<Column>>;

    fn indexes(&mut self, table: &Table) -> anyhow::Result<Vec<Index>>;

    fn foreign_keys(&mut self) -> anyhow::Result<HashMap<Table, Vec<ForeignKey>>>;

    fn databases(&mut self) -> anyhow::Result<Vec<String>> {
        Ok(Vec::new())
    }

    fn use_database(&mut self, _name: &str) -> anyhow::Result<()> {
        Ok(())
    }

    fn run(&mut self, _statement: &str) -> anyhow::Result<Outcome> {
        bail!("this connection cannot run statements")
    }

    fn run_reading(&mut self, _statement: &str) -> anyhow::Result<Outcome> {
        bail!("this connection cannot run statements")
    }

    fn run_server_wide(&mut self, statement: &str) -> anyhow::Result<Outcome> {
        self.run(statement)
    }

    fn quote(&self, identifier: &str) -> String {
        quote_double(identifier)
    }

    fn page(&self, statement: &str, limit: usize, offset: usize) -> String {
        format!("{statement} limit {limit} offset {offset}")
    }

    fn scalars(&mut self, statement: &str) -> anyhow::Result<Scalars> {
        let page = self.select(statement, &[])?;
        Ok(Scalars {
            headers: page.headers,
            rows: page
                .rows
                .into_iter()
                .map(|row| row.into_iter().map(Scalar::text).collect())
                .collect(),
        })
    }

    fn column_names(&mut self, table: &Table) -> anyhow::Result<Vec<String>> {
        Ok(self.rows(table, 0, 0, None)?.headers)
    }

    fn highest(&mut self, _table: &Table, _column: &str) -> anyhow::Result<Option<String>> {
        Ok(None)
    }

    fn update(
        &mut self,
        _table: &Table,
        _column: &str,
        _key: &str,
        _key_value: &str,
        _value: Option<&str>,
    ) -> anyhow::Result<u64> {
        bail!("this database cannot change rows")
    }

    fn insert(&mut self, _table: &Table, _values: &Values) -> anyhow::Result<()> {
        bail!("this database cannot add rows")
    }

    fn delete(
        &mut self,
        _table: &Table,
        _key: &str,
        _key_values: &[String],
    ) -> anyhow::Result<u64> {
        bail!("this database cannot delete rows")
    }
}

fn listed(count: usize, placeholder: &impl Fn(usize) -> String) -> String {
    (1..=count).map(placeholder).collect::<Vec<_>>().join(", ")
}

pub(crate) fn update_statement(
    quote: impl Fn(&str) -> String,
    placeholder: impl Fn(usize) -> String,
    table: &Table,
    column: &str,
    key: &str,
) -> String {
    format!(
        "update {} set {} = {} where {} = {}",
        table.quoted(&quote),
        quote(column),
        placeholder(1),
        quote(key),
        placeholder(2)
    )
}

pub(crate) fn insert_statement(
    quote: impl Fn(&str) -> String,
    placeholder: impl Fn(usize) -> String,
    table: &Table,
    values: &Values,
    defaults_only: &str,
) -> String {
    if values.is_empty() {
        return format!("insert into {} {defaults_only}", table.quoted(&quote));
    }
    format!(
        "insert into {} ({}) values ({})",
        table.quoted(&quote),
        values
            .iter()
            .map(|(column, _)| quote(column))
            .collect::<Vec<_>>()
            .join(", "),
        listed(values.len(), &placeholder)
    )
}

pub(crate) fn delete_statement(
    quote: impl Fn(&str) -> String,
    placeholder: impl Fn(usize) -> String,
    table: &Table,
    key: &str,
    count: usize,
) -> String {
    format!(
        "delete from {} where {} in ({})",
        table.quoted(&quote),
        quote(key),
        listed(count, &placeholder)
    )
}

// The statement as tql shows it in a refusal: each placeholder filled with its
// value as written, and one with no value left as a question mark.
fn filled_in(statement: &str, values: &[Option<&str>]) -> String {
    let mut segments = statement.split('?');
    let mut shown = segments.next().unwrap_or_default().to_string();
    let mut values = values.iter();
    for segment in segments {
        shown.push_str(values.next().copied().flatten().unwrap_or("?"));
        shown.push_str(segment);
    }
    shown
}

fn question(_: usize) -> String {
    "?".to_string()
}

pub(crate) fn shown_update(
    quote: impl Fn(&str) -> String,
    table: &Table,
    column: &str,
    key: &str,
    value: Option<&str>,
    key_value: &str,
) -> String {
    filled_in(
        &update_statement(quote, question, table, column, key),
        &[value, Some(key_value)],
    )
}

pub(crate) fn shown_insert(
    quote: impl Fn(&str) -> String,
    table: &Table,
    values: &Values,
    defaults_only: &str,
) -> String {
    let filled: Vec<Option<&str>> = values.iter().map(|(_, value)| value.as_deref()).collect();
    filled_in(
        &insert_statement(quote, question, table, values, defaults_only),
        &filled,
    )
}

pub(crate) fn shown_delete(
    quote: impl Fn(&str) -> String,
    table: &Table,
    key: &str,
    key_values: &[String],
) -> String {
    let filled: Vec<Option<&str>> = key_values
        .iter()
        .map(|value| Some(value.as_str()))
        .collect();
    filled_in(
        &delete_statement(quote, question, table, key, key_values.len()),
        &filled,
    )
}

pub(crate) fn refused(error: impl std::fmt::Display, statement: &str) -> anyhow::Error {
    anyhow!("{error}\n\n{statement}")
}

static LEADING_NOISE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?s)^(--[^\n]*\n|/\*.*?\*/|\s)+").expect("the leading noise pattern is valid")
});

static READING: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)^(select|show|explain|describe|desc|pragma|with)\b")
        .expect("the read-only pattern is valid")
});

pub fn is_read_only(statement: &str) -> bool {
    let normalised = LEADING_NOISE.replace(statement.trim_start(), "");
    if normalised
        .trim_end_matches([';', ' ', '\n', '\r', '\t'])
        .contains(';')
    {
        return false;
    }
    READING.is_match(&normalised)
}

pub fn connect(
    connection: &Connection,
    encrypter: Option<&Encrypter>,
) -> anyhow::Result<Box<dyn Database>> {
    tls::choose_ring();
    let password = decrypted(&connection.password, encrypter)?;

    match connection.driver {
        Driver::Sqlite => return Ok(Box::new(sqlite::Sqlite::open(connection)?)),
        Driver::Duckdb => return Ok(Box::new(duckdb::Duck::open(connection)?)),
        _ => {}
    }

    let tunnel = match connection.uses_ssh() {
        true => Some(tunnel::for_connection(
            connection,
            decrypted(&connection.ssh_password, encrypter)?,
        )?),
        false => None,
    };

    match connection.driver {
        Driver::Pgsql => Ok(Box::new(postgres::Postgres::open(
            connection, password, tunnel,
        )?)),
        Driver::Sqlsrv => Ok(Box::new(sqlsrv::SqlServer::through(
            connection, password, tunnel,
        )?)),
        Driver::Mysql => Ok(Box::new(mysql::MySql::open(connection, password, tunnel)?)),
        Driver::Libsql => Ok(Box::new(libsql::Libsql::open(connection, password)?)),
        Driver::Redis => bail!("a Redis connection is not a SQL database"),
        Driver::Clickhouse => Ok(Box::new(clickhouse::ClickHouse::open(
            connection, password, tunnel,
        )?)),
        Driver::Sqlite | Driver::Duckdb => bail!("a file connection opened as a server"),
    }
}

pub fn decrypted(
    secret: &Option<String>,
    encrypter: Option<&Encrypter>,
) -> anyhow::Result<Option<String>> {
    match (secret, encrypter) {
        (None, _) => Ok(None),
        (Some(payload), Some(encrypter)) => Ok(Some(encrypter.decrypt_string(payload)?)),
        (Some(_), None) => Err(anyhow!(
            "the tql key is missing, so the saved password cannot be read"
        )),
    }
}

pub(crate) fn select_page(
    quoted_table: &str,
    quote: impl Fn(&str) -> String,
    limit: usize,
    offset: usize,
    sort: Option<&Sort>,
    condition: Option<&str>,
) -> String {
    let filter = condition
        .map(|condition| format!(" where {condition}"))
        .unwrap_or_default();
    let order = sort
        .map(|sort| {
            format!(
                " order by {} {}",
                quote(&sort.column),
                sort.direction.keyword()
            )
        })
        .unwrap_or_default();
    format!("select * from {quoted_table}{filter}{order} limit {limit} offset {offset}")
}

pub(crate) fn quote_double(identifier: &str) -> String {
    format!("\"{}\"", identifier.replace('"', "\"\""))
}

pub(crate) fn split_list(list: Option<String>) -> Vec<String> {
    list.filter(|list| !list.is_empty())
        .map(|list| list.split(',').map(String::from).collect())
        .unwrap_or_default()
}

pub fn json_float(text: &str) -> Option<serde_json::Value> {
    let number = text.parse::<f64>().ok()?;
    if number.fract() == 0.0 && number.abs() < 1e15 {
        return Some(serde_json::Value::from(number as i64));
    }
    serde_json::Number::from_f64(number).map(serde_json::Value::Number)
}

pub(crate) fn all_text(rows: &[Vec<Option<String>>]) -> Vec<Vec<Kind>> {
    rows.iter().map(|row| vec![Kind::Text; row.len()]).collect()
}

pub(crate) fn outcome_or_affected(
    headers: Vec<String>,
    rows: Vec<Vec<Option<String>>>,
    affected: usize,
) -> Outcome {
    if headers.is_empty() {
        return Outcome {
            affected: Some(affected),
            ..Outcome::default()
        };
    }
    Outcome {
        headers,
        rows,
        affected: None,
        ..Outcome::default()
    }
}

pub(crate) fn keyed(pairs: Vec<(Table, String)>) -> HashMap<Table, Vec<String>> {
    let mut columns: HashMap<Table, Vec<String>> = HashMap::new();
    for (table, column) in pairs {
        columns.entry(table).or_default().push(column);
    }
    columns
}

pub(crate) fn single_column_keys(keys: HashMap<Table, Vec<String>>) -> HashMap<Table, String> {
    keys.into_iter()
        .filter_map(|(table, mut key)| (key.len() == 1).then(|| (table, key.remove(0))))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_whole_fraction_is_written_as_a_whole_number_as_tql_always_has() {
        assert_eq!(json_float("3").unwrap().to_string(), "3");
        assert_eq!(json_float("3.0").unwrap().to_string(), "3");
        assert_eq!(json_float("2.5").unwrap().to_string(), "2.5");
        assert_eq!(json_float("-0.25").unwrap().to_string(), "-0.25");
        assert_eq!(json_float("NaN"), None);
        assert_eq!(json_float("x"), None);
    }

    #[test]
    fn only_a_single_reading_statement_is_read_only() {
        for reading in [
            "select 1",
            "  -- note\n/* block */ SHOW tables;",
            "with t as (select 1) select * from t",
            "pragma table_info(x)",
        ] {
            assert!(is_read_only(reading), "{reading}");
        }
        for writing in [
            "update t set a = 1",
            "select 1; drop table t",
            "delete from t",
            "selection",
        ] {
            assert!(!is_read_only(writing), "{writing}");
        }
    }
}
