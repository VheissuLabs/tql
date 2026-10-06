use std::collections::HashMap;

use anyhow::Context;
use postgres::{Client, NoTls, SimpleQueryMessage};

use super::{
    Column, Database, Page, Sort, Values, delete_statement, insert_statement, quote_double,
    refused, select_page, single_column_keys, update_statement,
};
use crate::store::Connection;

pub struct Postgres {
    client: Client,
    config: postgres::Config,
}

impl Postgres {
    pub fn open(connection: &Connection, password: Option<String>) -> anyhow::Result<Self> {
        let mut config = postgres::Config::new();
        config
            .host(connection.host.as_deref().unwrap_or("127.0.0.1"))
            .port(connection.port.unwrap_or(5432))
            .user(connection.username.as_deref().unwrap_or("postgres"))
            .dbname(
                connection
                    .database
                    .as_deref()
                    .filter(|database| !database.trim().is_empty())
                    .unwrap_or("postgres"),
            );
        if let Some(password) = password {
            config.password(password);
        }

        let client = config
            .connect(NoTls)
            .with_context(|| format!("cannot connect to {}", connection.describe()))?;
        Ok(Postgres { client, config })
    }

    fn query(&mut self, sql: &str) -> anyhow::Result<Page> {
        let mut page = Page::default();
        for message in self.client.simple_query(sql)? {
            match message {
                SimpleQueryMessage::RowDescription(columns) => {
                    page.headers = columns
                        .iter()
                        .map(|column| column.name().to_string())
                        .collect();
                }
                SimpleQueryMessage::Row(row) => {
                    page.rows.push(
                        (0..row.len())
                            .map(|index| row.get(index).map(String::from))
                            .collect(),
                    );
                }
                _ => {}
            }
        }
        Ok(page)
    }

    fn column(&mut self, sql: &str) -> anyhow::Result<Vec<String>> {
        Ok(self
            .query(sql)?
            .rows
            .into_iter()
            .filter_map(|mut row| row.swap_remove(0))
            .collect())
    }
}

fn literal(value: Option<&str>) -> String {
    value
        .map(|value| format!("'{}'", value.replace('\'', "''")))
        .unwrap_or_else(|| "null".to_string())
}

impl Postgres {
    fn write(&mut self, sql: String) -> anyhow::Result<()> {
        self.client
            .batch_execute(&sql)
            .map_err(|error| refused(error, &sql))
    }
}

impl Database for Postgres {
    fn tables(&mut self) -> anyhow::Result<Vec<String>> {
        self.column(
            "select table_name from information_schema.tables \
             where table_schema = current_schema() and table_type in ('BASE TABLE', 'VIEW') order by table_name",
        )
    }

    fn rows(
        &mut self,
        table: &str,
        limit: usize,
        offset: usize,
        sort: Option<&Sort>,
    ) -> anyhow::Result<Page> {
        let statement = select_page(&quote_double(table), quote_double, limit, offset, sort);
        let mut page = self.query(&statement)?;
        page.statement = statement;
        Ok(page)
    }

    fn primary_keys(&mut self) -> anyhow::Result<HashMap<String, String>> {
        let pairs = self
            .query(
                "select c.relname, a.attname from pg_index i \
                 join pg_class c on c.oid = i.indrelid \
                 join pg_namespace n on n.oid = c.relnamespace \
                 join pg_attribute a on a.attrelid = i.indrelid and a.attnum = any(i.indkey) \
                 where i.indisprimary and n.nspname = current_schema() \
                 order by c.relname, array_position(i.indkey, a.attnum)",
            )?
            .rows
            .into_iter()
            .filter_map(|row| Some((row.first()?.clone()?, row.get(1)?.clone()?)))
            .collect();
        Ok(single_column_keys(pairs))
    }

    fn databases(&mut self) -> anyhow::Result<Vec<String>> {
        self.column("select datname from pg_database where datistemplate = false order by datname")
    }

    fn use_database(&mut self, name: &str) -> anyhow::Result<()> {
        let mut config = self.config.clone();
        config.dbname(name);
        self.client = config
            .connect(NoTls)
            .with_context(|| format!("cannot switch to {name}"))?;
        self.config = config;
        Ok(())
    }

    fn columns(&mut self, table: &str) -> anyhow::Result<Vec<Column>> {
        let page = self.query(&format!(
            "select a.attname, t.typname, not a.attnotnull, \
             (select pg_get_expr(adbin, adrelid) from pg_attrdef where c.oid = pg_attrdef.adrelid and pg_attrdef.adnum = a.attnum), \
             a.attgenerated \
             from pg_attribute a, pg_class c, pg_type t, pg_namespace n \
             where c.relname = {} and n.nspname = current_schema() and a.attnum > 0 and not a.attisdropped \
             and a.attrelid = c.oid and a.atttypid = t.oid and n.oid = c.relnamespace \
             order by a.attnum",
            literal(Some(table))
        ))?;
        Ok(page
            .rows
            .into_iter()
            .map(|row| {
                let cell = |index: usize| row.get(index).cloned().flatten();
                let generated = cell(4).is_some_and(|generated| !generated.is_empty());
                let default = cell(3).filter(|_| !generated);
                Column {
                    name: cell(0).unwrap_or_default(),
                    type_name: cell(1).unwrap_or_default(),
                    nullable: cell(2).as_deref() == Some("t"),
                    auto_increment: default
                        .as_deref()
                        .is_some_and(|default| default.starts_with("nextval(")),
                    default,
                }
            })
            .collect())
    }

    fn highest(&mut self, table: &str, column: &str) -> anyhow::Result<Option<String>> {
        let page = self.query(&format!(
            "select max({}) from {}",
            quote_double(column),
            quote_double(table)
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
        let values = [value, Some(key_value)];
        let sql = update_statement(
            quote_double,
            |position| literal(values[position - 1]),
            table,
            column,
            key,
        );
        self.write(sql)
    }

    fn insert(&mut self, table: &str, values: &Values) -> anyhow::Result<()> {
        let sql = insert_statement(
            quote_double,
            |position| literal(values[position - 1].1.as_deref()),
            table,
            values,
            "default values",
        );
        self.write(sql)
    }

    fn delete(&mut self, table: &str, key: &str, key_values: &[String]) -> anyhow::Result<()> {
        let sql = delete_statement(
            quote_double,
            |position| literal(Some(&key_values[position - 1])),
            table,
            key,
            key_values.len(),
        );
        self.write(sql)
    }
}
