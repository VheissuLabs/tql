use std::collections::HashMap;

use anyhow::Context;
use mysql::prelude::Queryable;
use mysql::{Conn, OptsBuilder, Params, Value};

use super::schema::keys_by_table;
use super::{
    Column, Database, ForeignKey, Index, Page, Sort, select_page, single_column_keys, split_list,
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
        self.select(sql, &[])
    }
}

fn read<Protocol: mysql::prelude::Protocol>(
    mut result: mysql::QueryResult<'_, '_, '_, Protocol>,
) -> anyhow::Result<Page> {
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
                .push(row?.unwrap().into_iter().map(text).collect());
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

    fn rows(
        &mut self,
        table: &str,
        limit: usize,
        offset: usize,
        sort: Option<&Sort>,
    ) -> anyhow::Result<Page> {
        let statement = select_page(&quote_backtick(table), quote_backtick, limit, offset, sort);
        let mut page = self.query(&statement)?;
        page.statement = statement;
        Ok(page)
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
            .query_drop(format!("use {}", quote_backtick(name)))?;
        Ok(())
    }

    fn quote(&self, identifier: &str) -> String {
        quote_backtick(identifier)
    }

    fn select(&mut self, statement: &str, values: &[String]) -> anyhow::Result<Page> {
        let mut page = if values.is_empty() {
            read(self.connection.query_iter(statement)?)?
        } else {
            let values: Vec<Value> = values.iter().map(Value::from).collect();
            read(
                self.connection
                    .exec_iter(statement, Params::Positional(values))?,
            )?
        };
        page.statement = statement.to_string();
        Ok(page)
    }

    fn columns(&mut self, table: &str) -> anyhow::Result<Vec<Column>> {
        Ok(self
            .select(
                "select column_name as `name`, data_type as `type_name`, is_nullable as `nullable`, \
                 column_default as `default`, extra as `extra` from information_schema.columns \
                 where table_schema = database() and table_name = ? order by ordinal_position",
                &[table.to_string()],
            )?
            .rows
            .into_iter()
            .map(|mut row| Column {
                name: row[0].take().unwrap_or_default(),
                type_name: row[1].take().unwrap_or_default(),
                nullable: row[2].as_deref() == Some("YES"),
                default: row[3].take(),
                auto_increment: row[4].as_deref() == Some("auto_increment"),
            })
            .collect())
    }

    fn indexes(&mut self, table: &str) -> anyhow::Result<Vec<Index>> {
        Ok(self
            .select(
                "select index_name as `name`, group_concat(column_name order by seq_in_index) as `columns`, \
                 not non_unique as `unique` from information_schema.statistics \
                 where table_schema = database() and table_name = ? \
                 group by index_name, index_type, non_unique",
                &[table.to_string()],
            )?
            .rows
            .into_iter()
            .map(|mut row| {
                let name = row[0].take().unwrap_or_default().to_lowercase();
                Index {
                    primary: name == "primary",
                    name,
                    columns: split_list(row[1].take()),
                    unique: row[2].as_deref() == Some("1"),
                }
            })
            .collect())
    }

    fn foreign_keys(&mut self) -> anyhow::Result<HashMap<String, Vec<ForeignKey>>> {
        Ok(keys_by_table(
            self.query(
                "select kc.table_name as `table`, group_concat(kc.column_name order by kc.ordinal_position) as `columns`, \
                 kc.referenced_table_name as `foreign_table`, \
                 group_concat(kc.referenced_column_name order by kc.ordinal_position) as `foreign_columns` \
                 from information_schema.key_column_usage kc join information_schema.referential_constraints rc \
                 on kc.constraint_schema = rc.constraint_schema and kc.constraint_name = rc.constraint_name \
                 where kc.table_schema = database() and kc.referenced_table_name is not null \
                 group by kc.table_name, kc.constraint_name, kc.referenced_table_name",
            )?
            .rows,
        ))
    }
}
