use super::Table;
use std::collections::HashMap;
use std::sync::Arc;

use anyhow::Context;
use mysql::consts::CapabilityFlags;
use mysql::consts::ColumnType;
use mysql::prelude::{Protocol, Queryable};
use mysql::{Conn, DriverError, OptsBuilder, QueryResult, Value};

use super::schema::keys_by_table;
use super::tls::{Mode, Tls};
use super::{
    Column, Database, Filters, ForeignKey, Generation, Index, Kind, Outcome, Page, Scalar, Scalars,
    Sort, Values, delete_statement, insert_statement, keyed, outcome_or_affected, refused,
    select_page, shown_delete, shown_insert, shown_update, single_column_keys, split_list,
    update_statement,
};
use crate::store::Connection;
use crate::tunnel::Tunnel;

pub struct MySql {
    connection: Conn,
    _tunnel: Option<Arc<Tunnel>>,
}

impl MySql {
    pub fn open(
        connection: &Connection,
        password: Option<String>,
        tunnel: Option<Arc<Tunnel>>,
    ) -> anyhow::Result<Self> {
        let settings = Tls::of(connection)?;
        let options = OptsBuilder::new()
            .prefer_socket(false)
            .additional_capabilities(CapabilityFlags::CLIENT_FOUND_ROWS)
            .ip_or_hostname(Some(match &tunnel {
                Some(_) => "127.0.0.1".to_string(),
                None => connection
                    .host
                    .clone()
                    .unwrap_or_else(|| "127.0.0.1".to_string()),
            }))
            .tcp_port(
                tunnel
                    .as_ref()
                    .map(|tunnel| tunnel.port)
                    .or(connection.port)
                    .unwrap_or(3306),
            )
            .user(connection.username.clone())
            .pass(password)
            .db_name(
                connection
                    .database
                    .clone()
                    .filter(|database| !database.trim().is_empty()),
            );

        let connected = match Conn::new(options.clone().ssl_opts(settings.mysql_options())) {
            Err(mysql::Error::DriverError(DriverError::TlsNotSupported))
                if settings.mode == Mode::Prefer =>
            {
                Conn::new(options)
            }
            other => other,
        };
        Ok(MySql {
            connection: connected
                .with_context(|| format!("cannot connect to {}", connection.describe()))?,
            _tunnel: tunnel,
        })
    }

    fn query(&mut self, sql: &str) -> anyhow::Result<Page> {
        page_of(self.connection.query_iter(sql).map_err(said)?)
    }

    fn bound_query(&mut self, sql: &str, bindings: Vec<String>) -> anyhow::Result<Page> {
        page_of(self.connection.exec_iter(sql, bindings).map_err(said)?)
    }

    fn bit_columns(&mut self, table: &Table) -> anyhow::Result<Vec<String>> {
        Ok(self
            .bound_query(
                "select column_name from information_schema.columns \
                 where table_schema = database() and table_name = ? and data_type = 'bit'",
                vec![table.name.clone()],
            )?
            .rows
            .into_iter()
            .filter_map(|mut row| row.swap_remove(0))
            .collect())
    }
}

fn bound_for(bits: &[String], column: &str, value: Option<&str>) -> Value {
    match value.and_then(|value| value.trim().parse::<u64>().ok()) {
        Some(number) if bits.iter().any(|bit| bit == column) => Value::UInt(number),
        _ => bound(value),
    }
}

fn kind(column_type: ColumnType) -> Kind {
    match column_type {
        ColumnType::MYSQL_TYPE_TINY
        | ColumnType::MYSQL_TYPE_SHORT
        | ColumnType::MYSQL_TYPE_INT24
        | ColumnType::MYSQL_TYPE_LONG
        | ColumnType::MYSQL_TYPE_LONGLONG
        | ColumnType::MYSQL_TYPE_YEAR
        | ColumnType::MYSQL_TYPE_BIT => Kind::Integer,
        ColumnType::MYSQL_TYPE_FLOAT | ColumnType::MYSQL_TYPE_DOUBLE => Kind::Float,
        _ => Kind::Text,
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
        let columns = set.columns();
        page.headers = columns
            .as_ref()
            .iter()
            .map(|column| column.name_str().into_owned())
            .collect();
        let kinds: Vec<Kind> = columns
            .as_ref()
            .iter()
            .map(|column| kind(column.column_type()))
            .collect();
        let shapes: Vec<(ColumnType, u8)> = columns
            .as_ref()
            .iter()
            .map(|column| (column.column_type(), column.decimals()))
            .collect();
        for row in set {
            page.rows.push(
                row.map_err(said)?
                    .unwrap()
                    .into_iter()
                    .zip(&shapes)
                    .map(|(value, (column_type, decimals))| shown(value, *column_type, *decimals))
                    .collect(),
            );
            page.kinds.push(kinds.clone());
        }
    }
    Ok(page)
}

const BINARY: u16 = 63;

fn scalar(value: Value, column_type: ColumnType, character_set: u16) -> Scalar {
    let Value::Bytes(bytes) = value else {
        return Scalar::text(text(value));
    };
    let numeric = matches!(
        column_type,
        ColumnType::MYSQL_TYPE_TINY
            | ColumnType::MYSQL_TYPE_SHORT
            | ColumnType::MYSQL_TYPE_LONG
            | ColumnType::MYSQL_TYPE_LONGLONG
            | ColumnType::MYSQL_TYPE_INT24
            | ColumnType::MYSQL_TYPE_YEAR
            | ColumnType::MYSQL_TYPE_FLOAT
            | ColumnType::MYSQL_TYPE_DOUBLE
    );
    let binary = matches!(column_type, ColumnType::MYSQL_TYPE_BIT)
        || (character_set == BINARY
            && matches!(
                column_type,
                ColumnType::MYSQL_TYPE_STRING
                    | ColumnType::MYSQL_TYPE_VAR_STRING
                    | ColumnType::MYSQL_TYPE_VARCHAR
                    | ColumnType::MYSQL_TYPE_BLOB
                    | ColumnType::MYSQL_TYPE_TINY_BLOB
                    | ColumnType::MYSQL_TYPE_MEDIUM_BLOB
                    | ColumnType::MYSQL_TYPE_LONG_BLOB
            ));
    match String::from_utf8(bytes) {
        Ok(text) if numeric => Scalar::Number(text),
        Ok(text) if !binary => Scalar::Text(text),
        Ok(text) => Scalar::Bytes(text.into_bytes()),
        Err(error) => Scalar::Bytes(error.into_bytes()),
    }
}

fn shown(value: Value, column_type: ColumnType, decimals: u8) -> Option<String> {
    let fraction = |micros: u32| match decimals {
        0 => String::new(),
        1..=6 => format!(".{micros:06}")[..1 + usize::from(decimals)].to_string(),
        _ if micros == 0 => String::new(),
        _ => format!(".{micros:06}"),
    };
    Some(match value {
        Value::Bytes(bytes) if column_type == ColumnType::MYSQL_TYPE_BIT => bytes
            .iter()
            .fold(0u64, |number, byte| number << 8 | u64::from(*byte))
            .to_string(),
        Value::Date(year, month, day, ..)
            if matches!(
                column_type,
                ColumnType::MYSQL_TYPE_DATE | ColumnType::MYSQL_TYPE_NEWDATE
            ) =>
        {
            format!("{year:04}-{month:02}-{day:02}")
        }
        Value::Date(year, month, day, hour, minute, second, micros) => format!(
            "{year:04}-{month:02}-{day:02} {hour:02}:{minute:02}:{second:02}{}",
            fraction(micros)
        ),
        Value::Time(negative, days, hours, minutes, seconds, micros) => format!(
            "{}{:02}:{minutes:02}:{seconds:02}{}",
            if negative { "-" } else { "" },
            days * 24 + u32::from(hours),
            fraction(micros)
        ),
        other => return text(other),
    })
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
    fn tables(&mut self) -> anyhow::Result<Vec<Table>> {
        Ok(self
            .query(
                "select table_name from information_schema.tables \
                 where table_schema = database() and table_type in ('BASE TABLE', 'SYSTEM VERSIONED') \
                 order by table_name",
            )?
            .rows
            .into_iter()
            .filter_map(|mut row| row.swap_remove(0))
            .map(Table::bare)
            .collect())
    }

    fn filtered_rows(
        &mut self,
        table: &Table,
        limit: usize,
        offset: usize,
        sort: Option<&Sort>,
        filters: Option<&Filters>,
    ) -> anyhow::Result<Page> {
        let condition = filters.and_then(|filters| filters.condition(&quote_backtick));
        let page = |shown: Option<String>| {
            select_page(
                &table.quoted(quote_backtick),
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

    fn primary_keys(&mut self) -> anyhow::Result<HashMap<Table, String>> {
        Ok(single_column_keys(self.key_columns()?))
    }

    fn key_columns(&mut self) -> anyhow::Result<HashMap<Table, Vec<String>>> {
        let pairs = self
            .query(
                "select table_name, column_name from information_schema.statistics \
                 where table_schema = database() and index_name = 'PRIMARY' \
                 order by table_name, seq_in_index",
            )?
            .rows
            .into_iter()
            .filter_map(|row| Some((Table::bare(row.first()?.clone()?), row.get(1)?.clone()?)))
            .collect();
        Ok(keyed(pairs))
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

    fn highest(&mut self, table: &Table, column: &str) -> anyhow::Result<Option<String>> {
        let page = self.query(&format!(
            "select max({}) from {}",
            quote_backtick(column),
            table.quoted(quote_backtick)
        ))?;
        Ok(page
            .rows
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
        let sql = update_statement(quote_backtick, question, table, column, key);
        let bits = self.bit_columns(table)?;
        self.connection
            .exec_drop(
                &sql,
                vec![
                    bound_for(&bits, column, value),
                    bound_for(&bits, key, Some(key_value)),
                ],
            )
            .map_err(|error| {
                refused(
                    said(error),
                    &shown_update(quote_backtick, table, column, key, value, key_value),
                )
            })?;
        Ok(self.connection.affected_rows())
    }

    fn insert(&mut self, table: &Table, values: &Values) -> anyhow::Result<()> {
        let sql = insert_statement(quote_backtick, question, table, values, "() values ()");
        let bits = self.bit_columns(table)?;
        let bindings: Vec<Value> = values
            .iter()
            .map(|(column, value)| bound_for(&bits, column, value.as_deref()))
            .collect();
        let outcome = if bindings.is_empty() {
            self.connection.query_drop(&sql)
        } else {
            self.connection.exec_drop(&sql, bindings)
        };
        outcome.map_err(|error| {
            refused(
                said(error),
                &shown_insert(quote_backtick, table, values, "() values ()"),
            )
        })
    }

    fn delete(&mut self, table: &Table, key: &str, key_values: &[String]) -> anyhow::Result<u64> {
        let sql = delete_statement(quote_backtick, question, table, key, key_values.len());
        let bindings: Vec<Value> = key_values
            .iter()
            .map(|key_value| bound(Some(key_value)))
            .collect();
        self.connection.exec_drop(&sql, bindings).map_err(|error| {
            refused(
                said(error),
                &shown_delete(quote_backtick, table, key, key_values),
            )
        })?;
        Ok(self.connection.affected_rows())
    }

    fn run(&mut self, statement: &str) -> anyhow::Result<Outcome> {
        let result = self.connection.query_iter(statement).map_err(said)?;
        let affected = result.affected_rows() as usize;
        let page = page_of(result)?;
        Ok(Outcome {
            kinds: page.kinds,
            ..outcome_or_affected(page.headers, page.rows, affected)
        })
    }

    fn run_reading(&mut self, statement: &str) -> anyhow::Result<Outcome> {
        self.connection
            .query_drop("start transaction read only")
            .map_err(said)?;
        let ran = self.run(statement);
        let _ = self.connection.query_drop("rollback");
        ran
    }

    fn quote(&self, identifier: &str) -> String {
        quote_backtick(identifier)
    }

    fn scalars(&mut self, statement: &str) -> anyhow::Result<Scalars> {
        let mut result = self.connection.query_iter(statement).map_err(said)?;
        let mut scalars = Scalars::default();
        let Some(set) = result.iter() else {
            return Ok(scalars);
        };
        let columns: Vec<(ColumnType, u16)> = set
            .columns()
            .as_ref()
            .iter()
            .map(|column| (column.column_type(), column.character_set()))
            .collect();
        scalars.headers = set
            .columns()
            .as_ref()
            .iter()
            .map(|column| column.name_str().into_owned())
            .collect();
        for row in set {
            let values = row.map_err(said)?.unwrap();
            scalars.rows.push(
                values
                    .into_iter()
                    .zip(&columns)
                    .map(|(value, (column_type, character_set))| {
                        scalar(value, *column_type, *character_set)
                    })
                    .collect(),
            );
        }
        Ok(scalars)
    }

    fn select(&mut self, statement: &str, values: &[String]) -> anyhow::Result<Page> {
        let mut page = if values.is_empty() {
            self.query(statement)?
        } else {
            self.bound_query(statement, values.to_vec())?
        };
        page.statement = statement.to_string();
        Ok(page)
    }

    fn columns(&mut self, table: &Table) -> anyhow::Result<Vec<Column>> {
        Ok(self
            .select(
                "select column_name as `name`, data_type as `type_name`, column_type as `type`, \
                 collation_name as `collation`, is_nullable as `nullable`, \
                 column_default as `default`, column_comment as `comment`, \
                 generation_expression as `expression`, extra as `extra` \
                 from information_schema.columns where table_schema = database() and table_name = ? \
                 order by ordinal_position asc",
                std::slice::from_ref(&table.name),
            )?
            .rows
            .into_iter()
            .map(|mut row| {
                let expression = row[7].take().filter(|expression| !expression.is_empty());
                Column {
                    name: row[0].take().unwrap_or_default(),
                    type_name: row[1].take().unwrap_or_default(),
                    full_type: row[2].take().unwrap_or_default(),
                    collation: row[3].take(),
                    nullable: row[4].as_deref() == Some("YES"),
                    default: row[5].take(),
                    comment: row[6].take().filter(|comment| !comment.is_empty()),
                    auto_increment: row[8].as_deref() == Some("auto_increment"),
                    generation: expression.map(|expression| Generation {
                        kind: match row[8].as_deref() {
                            Some("STORED GENERATED") => Some("stored".to_string()),
                            Some("VIRTUAL GENERATED") => Some("virtual".to_string()),
                            _ => None,
                        },
                        expression: Some(expression),
                    }),
                }
            })
            .collect())
    }

    fn indexes(&mut self, table: &Table) -> anyhow::Result<Vec<Index>> {
        Ok(self
            .select(
                "select index_name as `name`, group_concat(column_name order by seq_in_index) as `columns`, \
                 not non_unique as `unique` from information_schema.statistics \
                 where table_schema = database() and table_name = ? \
                 group by index_name, index_type, non_unique",
                std::slice::from_ref(&table.name),
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

    fn foreign_keys(&mut self) -> anyhow::Result<HashMap<Table, Vec<ForeignKey>>> {
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
