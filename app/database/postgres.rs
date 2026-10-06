use std::collections::HashMap;

use anyhow::Context;
use postgres::{Client, NoTls, SimpleQueryMessage};

use super::schema::{fill, keys_by_table, quoted};
use super::{
    Column, Database, ForeignKey, Index, Page, Sort, quote_double, select_page, single_column_keys,
    split_list,
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

    fn quote(&self, identifier: &str) -> String {
        quote_double(identifier)
    }

    fn select(&mut self, statement: &str, values: &[String]) -> anyhow::Result<Page> {
        let filled = fill(statement, values, quoted);
        let mut page = self.query(&filled)?;
        page.statement = filled;
        Ok(page)
    }

    fn columns(&mut self, table: &str) -> anyhow::Result<Vec<Column>> {
        Ok(self
            .select(
                "select a.attname as name, t.typname as type_name, not a.attnotnull as nullable, \
                 (select pg_get_expr(adbin, adrelid) from pg_attrdef where c.oid = pg_attrdef.adrelid and pg_attrdef.adnum = a.attnum) as default, \
                 a.attgenerated as generated \
                 from pg_attribute a, pg_class c, pg_type t, pg_namespace n \
                 where c.relname = ? and n.nspname = current_schema() and a.attnum > 0 and a.attrelid = c.oid \
                 and a.atttypid = t.oid and n.oid = c.relnamespace order by a.attnum",
                &[table.to_string()],
            )?
            .rows
            .into_iter()
            .map(|mut row| {
                let generated = row[4].as_deref().is_some_and(|kind| !kind.is_empty());
                let default = row[3].take();
                Column {
                    name: row[0].take().unwrap_or_default(),
                    type_name: row[1].take().unwrap_or_default(),
                    nullable: truthy(&row[2]),
                    auto_increment: default
                        .as_deref()
                        .is_some_and(|default| default.starts_with("nextval(")),
                    default: default.filter(|_| !generated),
                }
            })
            .collect())
    }

    fn indexes(&mut self, table: &str) -> anyhow::Result<Vec<Index>> {
        Ok(self
            .select(
                "select ic.relname as name, string_agg(a.attname, ',' order by indseq.ord) as columns, \
                 i.indisunique as \"unique\", i.indisprimary as \"primary\" \
                 from pg_index i \
                 join pg_class tc on tc.oid = i.indrelid \
                 join pg_namespace tn on tn.oid = tc.relnamespace \
                 join pg_class ic on ic.oid = i.indexrelid \
                 join pg_am am on am.oid = ic.relam \
                 join lateral unnest(i.indkey) with ordinality as indseq(num, ord) on true \
                 left join pg_attribute a on a.attrelid = i.indrelid and a.attnum = indseq.num \
                 where tc.relname = ? and tn.nspname = current_schema() \
                 group by ic.relname, am.amname, i.indisunique, i.indisprimary",
                &[table.to_string()],
            )?
            .rows
            .into_iter()
            .map(|mut row| Index {
                name: row[0].take().unwrap_or_default().to_lowercase(),
                columns: split_list(row[1].take()),
                unique: truthy(&row[2]),
                primary: truthy(&row[3]),
            })
            .collect())
    }

    fn foreign_keys(&mut self) -> anyhow::Result<HashMap<String, Vec<ForeignKey>>> {
        Ok(keys_by_table(
            self.query(
                "select tc.relname as table, string_agg(la.attname, ',' order by conseq.ord) as columns, \
                 fc.relname as foreign_table, string_agg(fa.attname, ',' order by conseq.ord) as foreign_columns \
                 from pg_constraint c \
                 join pg_class tc on c.conrelid = tc.oid \
                 join pg_namespace tn on tn.oid = tc.relnamespace \
                 join pg_class fc on c.confrelid = fc.oid \
                 join pg_namespace fn on fn.oid = fc.relnamespace \
                 join lateral unnest(c.conkey) with ordinality as conseq(num, ord) on true \
                 join pg_attribute la on la.attrelid = c.conrelid and la.attnum = conseq.num \
                 join pg_attribute fa on fa.attrelid = c.confrelid and fa.attnum = c.confkey[conseq.ord] \
                 where c.contype = 'f' and tn.nspname = current_schema() \
                 group by tc.relname, c.conname, fn.nspname, fc.relname, c.confupdtype, c.confdeltype",
            )?
            .rows,
        ))
    }
}

fn truthy(value: &Option<String>) -> bool {
    value.as_deref() == Some("t")
}
