mod column_default;
mod mysql;
mod postgres;
mod sqlite;

use std::collections::HashMap;

use anyhow::{anyhow, bail};

pub use column_default::ColumnDefault;

use crate::encrypter::Encrypter;
use crate::store::Connection;

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

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Page {
    pub statement: String,
    pub headers: Vec<String>,
    pub rows: Vec<Vec<Option<String>>>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Column {
    pub name: String,
    pub type_name: String,
    pub nullable: bool,
    pub default: Option<String>,
    pub auto_increment: bool,
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

pub trait Database {
    fn tables(&mut self) -> anyhow::Result<Vec<String>>;

    fn rows(
        &mut self,
        table: &str,
        limit: usize,
        offset: usize,
        sort: Option<&Sort>,
    ) -> anyhow::Result<Page>;

    fn primary_keys(&mut self) -> anyhow::Result<HashMap<String, String>>;

    fn databases(&mut self) -> anyhow::Result<Vec<String>> {
        Ok(Vec::new())
    }

    fn use_database(&mut self, _name: &str) -> anyhow::Result<()> {
        Ok(())
    }

    fn columns(&mut self, _table: &str) -> anyhow::Result<Vec<Column>> {
        Ok(Vec::new())
    }

    fn highest(&mut self, _table: &str, _column: &str) -> anyhow::Result<Option<String>> {
        Ok(None)
    }

    fn update(
        &mut self,
        _table: &str,
        _column: &str,
        _key: &str,
        _key_value: &str,
        _value: Option<&str>,
    ) -> anyhow::Result<()> {
        bail!("this database cannot change rows")
    }

    fn insert(&mut self, _table: &str, _values: &Values) -> anyhow::Result<()> {
        bail!("this database cannot add rows")
    }

    fn delete(&mut self, _table: &str, _key: &str, _key_values: &[String]) -> anyhow::Result<()> {
        bail!("this database cannot delete rows")
    }
}

fn listed(count: usize, placeholder: &impl Fn(usize) -> String) -> String {
    (1..=count).map(placeholder).collect::<Vec<_>>().join(", ")
}

pub(crate) fn update_statement(
    quote: impl Fn(&str) -> String,
    placeholder: impl Fn(usize) -> String,
    table: &str,
    column: &str,
    key: &str,
) -> String {
    format!(
        "update {} set {} = {} where {} = {}",
        quote(table),
        quote(column),
        placeholder(1),
        quote(key),
        placeholder(2)
    )
}

pub(crate) fn insert_statement(
    quote: impl Fn(&str) -> String,
    placeholder: impl Fn(usize) -> String,
    table: &str,
    values: &Values,
    defaults_only: &str,
) -> String {
    if values.is_empty() {
        return format!("insert into {} {defaults_only}", quote(table));
    }
    format!(
        "insert into {} ({}) values ({})",
        quote(table),
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
    table: &str,
    key: &str,
    count: usize,
) -> String {
    format!(
        "delete from {} where {} in ({})",
        quote(table),
        quote(key),
        listed(count, &placeholder)
    )
}

pub(crate) fn refused(error: impl std::fmt::Display, statement: &str) -> anyhow::Error {
    anyhow!("{error}\n\n{statement}")
}

pub fn connect(
    connection: &Connection,
    encrypter: Option<&Encrypter>,
) -> anyhow::Result<Box<dyn Database>> {
    if connection.uses_ssh() {
        bail!(
            "SSH tunnels are not ported yet; use the PHP tql for {}",
            connection.name
        );
    }

    let password = match (&connection.password, encrypter) {
        (None, _) => None,
        (Some(payload), Some(encrypter)) => Some(encrypter.decrypt_string(payload)?),
        (Some(_), None) => {
            return Err(anyhow!(
                "the tql key is missing, so the saved password cannot be read"
            ));
        }
    };

    match connection.driver.as_str() {
        "sqlite" => Ok(Box::new(sqlite::Sqlite::open(connection)?)),
        "pgsql" => Ok(Box::new(postgres::Postgres::open(connection, password)?)),
        "mysql" | "mariadb" => Ok(Box::new(mysql::MySql::open(connection, password)?)),
        other => bail!("the {other} driver is not ported yet"),
    }
}

pub(crate) fn select_page(
    quoted_table: &str,
    quote: impl Fn(&str) -> String,
    limit: usize,
    offset: usize,
    sort: Option<&Sort>,
) -> String {
    let order = sort
        .map(|sort| {
            format!(
                " order by {} {}",
                quote(&sort.column),
                sort.direction.keyword()
            )
        })
        .unwrap_or_default();
    format!("select * from {quoted_table}{order} limit {limit} offset {offset}")
}

pub(crate) fn quote_double(identifier: &str) -> String {
    format!("\"{}\"", identifier.replace('"', "\"\""))
}

pub(crate) fn single_column_keys(pairs: Vec<(String, String)>) -> HashMap<String, String> {
    let mut columns: HashMap<String, Vec<String>> = HashMap::new();
    for (table, column) in pairs {
        columns.entry(table).or_default().push(column);
    }
    columns
        .into_iter()
        .filter_map(|(table, mut key)| (key.len() == 1).then(|| (table, key.remove(0))))
        .collect()
}
