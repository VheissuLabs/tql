use std::collections::HashMap;

use anyhow::Context;
use mysql::prelude::{Protocol, Queryable};
use mysql::{Conn, OptsBuilder, QueryResult, Value};

use super::{
    Column, Database, Filters, Outcome, Page, Sort, Values, delete_statement, insert_statement,
    outcome_or_affected, refused, select_page, single_column_keys, update_statement,
};
use crate::store::Connection;

pub struct MySql {
    connection: Conn,
}

impl MySql {
    pub fn open(connection: &Connection, password: Option<String>) -> anyhow::Result<Self> {
        let options = OptsBuilder::new()
            .ip_or_hostname(Some(
                connection
                    .host
                    .clone()
                    .unwrap_or_else(|| "127.0.0.1".to_string()),
            ))
            .tcp_port(connection.port.unwrap_or(3306))
            .user(connection.username.clone())
            .pass(password)
            .db_name(
                connection
                    .database
                    .clone()
                    .filter(|database| !database.trim().is_empty()),
            );

        let connection = Conn::new(options)
            .with_context(|| format!("cannot connect to {}", connection.describe()))?;
        Ok(MySql { connection })
    }

    fn query(&mut self, sql: &str) -> anyhow::Result<Page> {
        page_of(self.connection.query_iter(sql).map_err(said)?)
    }

    fn bound_query(&mut self, sql: &str, bindings: Vec<String>) -> anyhow::Result<Page> {
        page_of(self.connection.exec_iter(sql, bindings).map_err(said)?)
    }
}

fn said(error: mysql::Error) -> anyhow::Error {
    match error {
        mysql::Error::MySqlError(server) => anyhow::anyhow!(server.message),
        other => other.into(),
    }
}

fn page_of<P: Protocol>(mut result: QueryResult<'_, '_, '_, P>) -> anyhow::Result<Page> {
    let mut page = Page::default();
    if let Some(set) = result.iter() {
        page.headers = set
            .columns()
            .as_ref()
            .iter()
            .map(|column| column.name_str().into_owned())
            .collect();
        for row in set {
            page.rows
                .push(row.map_err(said)?.unwrap().into_iter().map(text).collect());
        }
    }
    Ok(page)
}

fn text(value: Value) -> Option<String> {
    Some(match value {
        Value::NULL => return None,
        Value::Bytes(bytes) => String::from_utf8_lossy(&bytes).into_owned(),
        Value::Int(number) => number.to_string(),
        Value::UInt(number) => number.to_string(),
        Value::Float(number) => number.to_string(),
        Value::Double(number) => number.to_string(),
        Value::Date(year, month, day, hour, minute, second, micros) => {
            let date = format!("{year:04}-{month:02}-{day:02} {hour:02}:{minute:02}:{second:02}");
            if micros == 0 {
                date
            } else {
                format!("{date}.{micros:06}")
            }
        }
        Value::Time(negative, days, hours, minutes, seconds, _) => {
            let sign = if negative { "-" } else { "" };
            format!(
                "{sign}{:02}:{minutes:02}:{seconds:02}",
                days * 24 + u32::from(hours)
            )
        }
    })
}

fn bound(value: Option<&str>) -> Value {
    value
        .map(|value| Value::Bytes(value.as_bytes().to_vec()))
        .unwrap_or(Value::NULL)
}

fn question(_: usize) -> String {
    "?".to_string()
}

fn quote_backtick(identifier: &str) -> String {
    format!("`{}`", identifier.replace('`', "``"))
}

impl Database for MySql {
    fn tables(&mut self) -> anyhow::Result<Vec<String>> {
        Ok(self
            .query(
                "select table_name from information_schema.tables \
                 where table_schema = database() and table_type in ('BASE TABLE', 'SYSTEM VERSIONED') \
                 order by table_name",
            )?
            .rows
            .into_iter()
            .filter_map(|mut row| row.swap_remove(0))
            .collect())
    }

    fn filtered_rows(
        &mut self,
        table: &str,
        limit: usize,
        offset: usize,
        sort: Option<&Sort>,
        filters: Option<&Filters>,
    ) -> anyhow::Result<Page> {
        let condition = filters.and_then(|filters| filters.condition(&quote_backtick));
        let page = |shown: Option<String>| {
            select_page(
                &quote_backtick(table),
                quote_backtick,
                limit,
                offset,
                sort,
                shown.as_deref(),
            )
        };
        let Some(condition) = condition else {
            let statement = page(None);
            let mut rows = self.query(&statement)?;
            rows.statement = statement;
            return Ok(rows);
        };
        let mut rows =
            self.bound_query(&page(Some(condition.placeheld())), condition.bindings())?;
        rows.statement = page(Some(condition.readable()));
        Ok(rows)
    }

    fn primary_keys(&mut self) -> anyhow::Result<HashMap<String, String>> {
        let pairs = self
            .query(
                "select table_name, column_name from information_schema.statistics \
                 where table_schema = database() and index_name = 'PRIMARY' \
                 order by table_name, seq_in_index",
            )?
            .rows
            .into_iter()
            .filter_map(|row| Some((row.first()?.clone()?, row.get(1)?.clone()?)))
            .collect();
        Ok(single_column_keys(pairs))
    }

    fn databases(&mut self) -> anyhow::Result<Vec<String>> {
        Ok(self
            .query("show databases")?
            .rows
            .into_iter()
            .filter_map(|mut row| row.swap_remove(0))
            .collect())
    }

    fn use_database(&mut self, name: &str) -> anyhow::Result<()> {
        self.connection
            .query_drop(format!("use {}", quote_backtick(name)))
            .map_err(said)?;
        Ok(())
    }

    fn columns(&mut self, table: &str) -> anyhow::Result<Vec<Column>> {
        let rows: Vec<mysql::Row> = self.connection.exec(
            "select column_name, data_type, is_nullable, column_default, extra \
             from information_schema.columns where table_schema = database() and table_name = ? \
             order by ordinal_position",
            (table,),
        )?;
        Ok(rows
            .into_iter()
            .map(|row| {
                let cells: Vec<Option<String>> = row.unwrap().into_iter().map(text).collect();
                let cell = |index: usize| cells.get(index).cloned().flatten();
                Column {
                    name: cell(0).unwrap_or_default(),
                    type_name: cell(1).unwrap_or_default(),
                    nullable: cell(2).as_deref() == Some("YES"),
                    default: cell(3),
                    auto_increment: cell(4).as_deref() == Some("auto_increment"),
                }
            })
            .collect())
    }

    fn highest(&mut self, table: &str, column: &str) -> anyhow::Result<Option<String>> {
        let page = self.query(&format!(
            "select max({}) from {}",
            quote_backtick(column),
            quote_backtick(table)
        ))?;
        Ok(page
            .rows
            .into_iter()
            .next()
            .and_then(|mut row| row.swap_remove(0)))
    }

    fn update(
        &mut self,
        table: &str,
        column: &str,
        key: &str,
        key_value: &str,
        value: Option<&str>,
    ) -> anyhow::Result<()> {
        let sql = update_statement(quote_backtick, question, table, column, key);
        self.connection
            .exec_drop(&sql, vec![bound(value), bound(Some(key_value))])
            .map_err(|error| refused(said(error), &sql))
    }

    fn insert(&mut self, table: &str, values: &Values) -> anyhow::Result<()> {
        let sql = insert_statement(quote_backtick, question, table, values, "() values ()");
        let bindings: Vec<Value> = values
            .iter()
            .map(|(_, value)| bound(value.as_deref()))
            .collect();
        let outcome = if bindings.is_empty() {
            self.connection.query_drop(&sql)
        } else {
            self.connection.exec_drop(&sql, bindings)
        };
        outcome.map_err(|error| refused(said(error), &sql))
    }

    fn delete(&mut self, table: &str, key: &str, key_values: &[String]) -> anyhow::Result<()> {
        let sql = delete_statement(quote_backtick, question, table, key, key_values.len());
        let bindings: Vec<Value> = key_values
            .iter()
            .map(|key_value| bound(Some(key_value)))
            .collect();
        self.connection
            .exec_drop(&sql, bindings)
            .map_err(|error| refused(said(error), &sql))
    }

    fn run(&mut self, statement: &str) -> anyhow::Result<Outcome> {
        let result = self.connection.query_iter(statement).map_err(said)?;
        let affected = result.affected_rows() as usize;
        let page = page_of(result)?;
        Ok(outcome_or_affected(page.headers, page.rows, affected))
    }

    fn quote(&self, identifier: &str) -> String {
        quote_backtick(identifier)
    }
}
