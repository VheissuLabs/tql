use std::collections::HashMap;

use std::net::Ipv4Addr;
use std::sync::Arc;

use anyhow::{Context, bail};
use postgres::types::Type;

type Rows = Vec<Vec<Option<String>>>;
use postgres::{Client, SimpleQueryMessage};
use tokio_postgres_rustls::MakeRustlsConnect;

use super::schema::{fill, keys_by_table};
use super::tls::Tls;
use super::{
    Column, Database, Filters, ForeignKey, Index, Kind, Outcome, Page, Sort, Values,
    delete_statement, insert_statement, outcome_or_affected, quote_double, refused, select_page,
    shown_delete, shown_insert, shown_update, single_column_keys, split_list, update_statement,
};
use crate::store::Connection;
use crate::tunnel::Tunnel;

pub struct Postgres {
    client: Client,
    config: postgres::Config,
    tls: MakeRustlsConnect,
    _tunnel: Option<Arc<Tunnel>>,
}

impl Postgres {
    pub fn open(
        connection: &Connection,
        password: Option<String>,
        tunnel: Option<Arc<Tunnel>>,
    ) -> anyhow::Result<Self> {
        let settings = Tls::of(connection)?;
        let mut config = postgres::Config::new();
        config
            .host(connection.host.as_deref().unwrap_or("127.0.0.1"))
            .port(
                tunnel
                    .as_ref()
                    .map(|tunnel| tunnel.port)
                    .or(connection.port)
                    .unwrap_or(5432),
            )
            .ssl_mode(settings.postgres_mode())
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
        if tunnel.is_some() {
            config.hostaddr(Ipv4Addr::LOCALHOST.into());
        }

        let tls = MakeRustlsConnect::new(settings.client_config()?);
        let client = config
            .connect(tls.clone())
            .with_context(|| format!("cannot connect to {}", connection.describe()))?;
        Ok(Postgres {
            client,
            config,
            tls,
            _tunnel: tunnel,
        })
    }

    fn simple(&mut self, statement: &str) -> anyhow::Result<(Vec<String>, Rows, usize)> {
        let mut headers = Vec::new();
        let mut rows = Vec::new();
        let mut affected = 0;
        for message in self.client.simple_query(statement)? {
            match message {
                SimpleQueryMessage::RowDescription(columns) => {
                    headers = columns
                        .iter()
                        .map(|column| column.name().to_string())
                        .collect();
                }
                SimpleQueryMessage::Row(row) => {
                    rows.push(
                        (0..row.len())
                            .map(|index| row.get(index).map(String::from))
                            .collect(),
                    );
                }
                SimpleQueryMessage::CommandComplete(count) => affected = count as usize,
                _ => {}
            }
        }
        Ok((headers, rows, affected))
    }

    fn query(&mut self, sql: &str) -> anyhow::Result<Page> {
        let (headers, rows, _) = self.simple(sql)?;
        Ok(Page {
            headers,
            rows,
            ..Page::default()
        })
    }

    fn shown(&mut self, sql: &str) -> anyhow::Result<Page> {
        let outcome = self.run(sql)?;
        Ok(Page {
            headers: outcome.headers,
            rows: outcome.rows,
            kinds: outcome.kinds,
            ..Page::default()
        })
    }

    fn filled(statement: &str, values: &[String]) -> anyhow::Result<String> {
        if values.iter().any(|value| value.contains('\0')) {
            bail!("a value cannot contain a NUL character in PostgreSQL");
        }
        Ok(fill(statement, values, escaped_literal))
    }

    fn query_with(&mut self, statement: &str, values: &[String]) -> anyhow::Result<Page> {
        self.query(&Self::filled(statement, values)?)
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

impl Postgres {
    fn kinds(&mut self, statement: &str, width: usize) -> Option<Vec<Kind>> {
        let prepared = self.client.prepare(statement).ok()?;
        let kinds: Vec<Kind> = prepared
            .columns()
            .iter()
            .map(|column| match *column.type_() {
                Type::INT2 | Type::INT4 | Type::INT8 => Kind::Integer,
                Type::BOOL => Kind::Boolean,
                _ => Kind::Text,
            })
            .collect();
        (kinds.len() == width).then_some(kinds)
    }
}

fn spell_booleans(row: &mut [Option<String>], kinds: &[Kind]) {
    for (value, kind) in row.iter_mut().zip(kinds) {
        if *kind != Kind::Boolean {
            continue;
        }
        *value = match value.as_deref() {
            Some("t") => Some("true".to_string()),
            Some("f") => Some("false".to_string()),
            _ => value.take(),
        };
    }
}

fn literal(value: Option<&str>) -> String {
    value
        .map(escaped_literal)
        .unwrap_or_else(|| "null".to_string())
}

impl Postgres {
    fn write(&mut self, sql: String, shown: String) -> anyhow::Result<()> {
        self.client
            .batch_execute(&sql)
            .map_err(|error| refused(error, &shown))
    }
}

fn escaped_literal(value: &str) -> String {
    format!("E'{}'", value.replace('\\', "\\\\").replace('\'', "''"))
}

impl Database for Postgres {
    fn tables(&mut self) -> anyhow::Result<Vec<String>> {
        self.column(
            "select c.relname from pg_class c join pg_namespace n on n.oid = c.relnamespace \
             where c.relkind in ('r', 'p') and n.nspname = current_schema() order by c.relname",
        )
    }

    fn filtered_rows(
        &mut self,
        table: &str,
        limit: usize,
        offset: usize,
        sort: Option<&Sort>,
        filters: Option<&Filters>,
    ) -> anyhow::Result<Page> {
        let condition = filters.and_then(|filters| filters.condition(&quote_double));
        let page = |shown: Option<String>| {
            select_page(
                &quote_double(table),
                quote_double,
                limit,
                offset,
                sort,
                shown.as_deref(),
            )
        };
        if let Some(condition) = &condition
            && condition
                .bindings()
                .iter()
                .any(|value| value.contains('\0'))
        {
            bail!("a filter value cannot contain a NUL character in PostgreSQL");
        }
        let mut rows = self.shown(&page(
            condition
                .as_ref()
                .map(|condition| condition.render(escaped_literal)),
        ))?;
        rows.statement = page(condition.as_ref().map(|condition| condition.readable()));
        Ok(rows)
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
            .connect(self.tls.clone())
            .with_context(|| format!("cannot switch to {name}"))?;
        self.config = config;
        Ok(())
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
        self.write(
            sql,
            shown_update(quote_double, table, column, key, value, key_value),
        )
    }

    fn insert(&mut self, table: &str, values: &Values) -> anyhow::Result<()> {
        let sql = insert_statement(
            quote_double,
            |position| literal(values[position - 1].1.as_deref()),
            table,
            values,
            "default values",
        );
        self.write(
            sql,
            shown_insert(quote_double, table, values, "default values"),
        )
    }

    fn delete(&mut self, table: &str, key: &str, key_values: &[String]) -> anyhow::Result<()> {
        let sql = delete_statement(
            quote_double,
            |position| literal(Some(&key_values[position - 1])),
            table,
            key,
            key_values.len(),
        );
        self.write(sql, shown_delete(quote_double, table, key, key_values))
    }

    fn run(&mut self, statement: &str) -> anyhow::Result<Outcome> {
        let (headers, mut rows, affected) = self.simple(statement)?;
        if headers.is_empty() {
            return Ok(outcome_or_affected(headers, rows, affected));
        }
        let kinds = self.kinds(statement, headers.len());
        if let Some(kinds) = &kinds {
            for row in &mut rows {
                spell_booleans(row, kinds);
            }
        }
        Ok(Outcome {
            kinds: kinds
                .map(|kinds| vec![kinds; rows.len()])
                .unwrap_or_default(),
            ..outcome_or_affected(headers, rows, affected)
        })
    }

    fn select(&mut self, statement: &str, values: &[String]) -> anyhow::Result<Page> {
        let filled = Self::filled(statement, values)?;
        let mut page = self.shown(&filled)?;
        page.statement = filled;
        Ok(page)
    }

    fn columns(&mut self, table: &str) -> anyhow::Result<Vec<Column>> {
        Ok(self
            .query_with(
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
    matches!(value.as_deref(), Some("t" | "true"))
}

#[cfg(test)]
mod tests {
    use super::escaped_literal;

    #[test]
    fn escapes_quotes_and_backslashes_whatever_the_server_thinks_of_backslashes() {
        assert_eq!(escaped_literal("plain"), "E'plain'");
        assert_eq!(escaped_literal("O'Brien"), "E'O''Brien'");
        assert_eq!(
            escaped_literal("\\'; drop table x; --"),
            "E'\\\\''; drop table x; --'"
        );
    }
}
