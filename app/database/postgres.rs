use std::collections::HashMap;

use anyhow::Context;
use postgres::{Client, NoTls, SimpleQueryMessage};

use super::{Database, Page, Sort, quote_double, select_page, single_column_keys};
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
}
