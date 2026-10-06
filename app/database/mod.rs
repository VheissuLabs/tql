mod mysql;
mod postgres;
mod schema;
mod sqlite;

use std::collections::HashMap;

use anyhow::{anyhow, bail};

use crate::encrypter::Encrypter;
use crate::store::Connection;

pub use schema::{
    BackLink, Column, Count, ForeignKey, Index, Link, Pivot, Relations, is_plumbing, links,
    primary_key, readable, unique_columns,
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

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Page {
    pub statement: String,
    pub headers: Vec<String>,
    pub rows: Vec<Vec<Option<String>>>,
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

    fn quote(&self, identifier: &str) -> String;

    fn select(&mut self, statement: &str, values: &[String]) -> anyhow::Result<Page>;

    fn columns(&mut self, table: &str) -> anyhow::Result<Vec<Column>>;

    fn indexes(&mut self, table: &str) -> anyhow::Result<Vec<Index>>;

    fn foreign_keys(&mut self) -> anyhow::Result<HashMap<String, Vec<ForeignKey>>>;

    fn databases(&mut self) -> anyhow::Result<Vec<String>> {
        Ok(Vec::new())
    }

    fn use_database(&mut self, _name: &str) -> anyhow::Result<()> {
        Ok(())
    }
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

pub(crate) fn split_list(list: Option<String>) -> Vec<String> {
    list.filter(|list| !list.is_empty())
        .map(|list| list.split(',').map(String::from).collect())
        .unwrap_or_default()
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
