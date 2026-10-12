use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{anyhow, bail};
use serde_json::Value;

use super::http;
use super::schema::fill;
use super::tls::Tls;
use super::{
    Column, Database, Filters, ForeignKey, Index, Kind, Outcome, Page, Sort, Table, Values, keyed,
    outcome_or_affected, select_page, single_column_keys, split_list,
};
use crate::store::Connection;
use crate::tunnel::Tunnel;

const TIMEOUT: Duration = Duration::from_secs(300);
const FORMAT: &str = "JSONCompactEachRowWithNamesAndTypes";
pub const READ_ONLY: &str =
    "ClickHouse tables are read-only in tql: its updates are background mutations";

pub struct ClickHouse {
    base: String,
    user: String,
    password: Option<String>,
    database: String,
    _tunnel: Option<Arc<Tunnel>>,
}

struct Answer {
    headers: Vec<String>,
    rows: Vec<Vec<Option<String>>>,
    kinds: Vec<Vec<Kind>>,
}

impl ClickHouse {
    pub fn open(
        connection: &Connection,
        password: Option<String>,
        tunnel: Option<Arc<Tunnel>>,
    ) -> anyhow::Result<Self> {
        let secure = !matches!(
            Tls::of(connection)?.mode,
            super::tls::Mode::Unset | super::tls::Mode::Disable
        );
        let host = match &tunnel {
            Some(_) => "127.0.0.1".to_string(),
            None => connection
                .host
                .clone()
                .filter(|host| !host.trim().is_empty())
                .unwrap_or_else(|| "127.0.0.1".to_string()),
        };
        let port = tunnel
            .as_ref()
            .map(|tunnel| tunnel.port)
            .or(connection.port)
            .unwrap_or(if secure { 8443 } else { 8123 });
        let scheme = if secure { "https" } else { "http" };
        let database = ClickHouse {
            base: format!("{scheme}://{host}:{port}/"),
            user: connection
                .username
                .clone()
                .filter(|user| !user.is_empty())
                .unwrap_or_else(|| "default".to_string()),
            password,
            database: connection
                .database
                .clone()
                .filter(|database| !database.trim().is_empty())
                .unwrap_or_else(|| "default".to_string()),
            _tunnel: tunnel,
        };
        database.ask("select 1", false)?;
        Ok(database)
    }

    fn ask(&self, sql: &str, reading: bool) -> anyhow::Result<Answer> {
        let mut url = format!(
            "{}?database={}&default_format={FORMAT}&output_format_json_quote_64bit_integers=1\
             &output_format_json_quote_decimals=1&output_format_json_quote_denormals=1",
            self.base,
            crate::dsn::encode(&self.database)
        );
        if reading {
            url.push_str("&readonly=2");
        }
        let mut headers = vec![("X-ClickHouse-User", self.user.clone())];
        if let Some(password) = &self.password {
            headers.push(("X-ClickHouse-Key", password.clone()));
        }
        let reply = http::post(&url, &headers, sql.to_string(), TIMEOUT)?;
        if reply.status >= 400 {
            bail!("{}", said(&reply.body));
        }
        answer(&reply.body)
    }

    fn page(&self, sql: &str) -> anyhow::Result<Page> {
        let answer = self.ask(sql, false)?;
        Ok(Page {
            statement: sql.to_string(),
            headers: answer.headers,
            rows: answer.rows,
            kinds: answer.kinds,
        })
    }
}

fn said(body: &str) -> String {
    let body = body.trim();
    let message = body
        .split_once("DB::Exception: ")
        .map(|(_, rest)| rest)
        .unwrap_or(body);
    message
        .split(" (version ")
        .next()
        .unwrap_or(message)
        .trim()
        .to_string()
}

fn answer(body: &str) -> anyhow::Result<Answer> {
    let mut lines = body.lines().filter(|line| !line.trim().is_empty());
    let parse = |line: Option<&str>| -> anyhow::Result<Vec<Value>> {
        match line {
            None => Ok(Vec::new()),
            Some(line) => {
                serde_json::from_str::<Vec<Value>>(line).map_err(|_| anyhow!("{}", said(line)))
            }
        }
    };
    let headers: Vec<String> = parse(lines.next())?
        .into_iter()
        .map(|name| name.as_str().unwrap_or_default().to_string())
        .collect();
    let types: Vec<String> = parse(lines.next())?
        .into_iter()
        .map(|kind| kind.as_str().unwrap_or_default().to_string())
        .collect();
    let row_kinds: Vec<Kind> = types.iter().map(|kind| kind_of(kind)).collect();
    let mut rows = Vec::new();
    let mut kinds = Vec::new();
    for line in lines {
        let cells = parse(Some(line))?;
        rows.push(cells.iter().map(text).collect());
        kinds.push(row_kinds.clone());
    }
    Ok(Answer {
        headers,
        rows,
        kinds,
    })
}

fn unwrapped(kind: &str) -> &str {
    let mut kind = kind;
    for wrapper in ["Nullable(", "LowCardinality("] {
        if let Some(inner) = kind
            .strip_prefix(wrapper)
            .and_then(|rest| rest.strip_suffix(')'))
        {
            kind = inner;
        }
    }
    kind
}

fn kind_of(kind: &str) -> Kind {
    let kind = unwrapped(unwrapped(kind));
    if kind == "Bool" {
        return Kind::Boolean;
    }
    if kind.starts_with("Float") {
        return Kind::Float;
    }
    if kind.starts_with("Int") || kind.starts_with("UInt") {
        return Kind::Integer;
    }
    Kind::Text
}

fn text(cell: &Value) -> Option<String> {
    Some(match cell {
        Value::Null => return None,
        Value::String(text) => text.clone(),
        Value::Bool(flag) => flag.to_string(),
        Value::Number(number) => match number.as_f64() {
            Some(real) if number.is_f64() => super::exporter::real_text(real),
            _ => number.to_string(),
        },
        other => other.to_string(),
    })
}

fn quote(identifier: &str) -> String {
    format!("`{}`", identifier.replace('\\', "\\\\").replace('`', "\\`"))
}

fn literal(value: &str) -> String {
    format!("'{}'", value.replace('\\', "\\\\").replace('\'', "\\'"))
}

impl Database for ClickHouse {
    fn tables(&mut self) -> anyhow::Result<Vec<Table>> {
        Ok(self
            .ask(
                "select name from system.tables where database = currentDatabase() \
                 and not is_temporary and not startsWith(name, '.inner') order by name",
                false,
            )?
            .rows
            .into_iter()
            .filter_map(|mut row| row.get_mut(0).and_then(Option::take))
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
        let condition = filters
            .and_then(|filters| filters.condition(&quote))
            .map(|condition| condition.render(literal));
        let sql = select_page(
            &table.quoted(quote),
            quote,
            limit,
            offset,
            sort,
            condition.as_deref(),
        );
        self.page(&sql)
    }

    fn run(&mut self, statement: &str) -> anyhow::Result<Outcome> {
        let answer = self.ask(statement, false)?;
        Ok(Outcome {
            kinds: answer.kinds,
            ..outcome_or_affected(answer.headers, answer.rows, 0)
        })
    }

    fn run_reading(&mut self, statement: &str) -> anyhow::Result<Outcome> {
        let answer = self.ask(statement, true)?;
        Ok(Outcome {
            kinds: answer.kinds,
            ..outcome_or_affected(answer.headers, answer.rows, 0)
        })
    }

    fn quote(&self, identifier: &str) -> String {
        quote(identifier)
    }

    fn primary_keys(&mut self) -> anyhow::Result<HashMap<Table, String>> {
        Ok(single_column_keys(self.key_columns()?))
    }

    fn key_columns(&mut self) -> anyhow::Result<HashMap<Table, Vec<String>>> {
        Ok(keyed(
            self.ask(
                "select table, name from system.columns where database = currentDatabase() \
                 and is_in_primary_key order by table, position",
                false,
            )?
            .rows
            .into_iter()
            .filter_map(|mut row| {
                Some((
                    Table::bare(row.get_mut(0)?.take()?),
                    row.get_mut(1)?.take()?,
                ))
            })
            .collect(),
        ))
    }

    fn select(&mut self, statement: &str, values: &[String]) -> anyhow::Result<Page> {
        self.page(&fill(statement, values, literal))
    }

    fn columns(&mut self, table: &Table) -> anyhow::Result<Vec<Column>> {
        Ok(self
            .ask(
                &format!(
                    "select name, type, default_kind, default_expression, comment \
                     from system.columns where database = currentDatabase() and table = {} \
                     order by position",
                    literal(&table.name)
                ),
                false,
            )?
            .rows
            .into_iter()
            .map(|mut row| {
                let full_type = row[1].take().unwrap_or_default();
                let kind = row[2].take().unwrap_or_default();
                let expression = row[3].take().filter(|expression| !expression.is_empty());
                Column {
                    name: row[0].take().unwrap_or_default(),
                    type_name: unwrapped(unwrapped(&full_type))
                        .split('(')
                        .next()
                        .unwrap_or_default()
                        .to_lowercase(),
                    nullable: full_type.starts_with("Nullable(")
                        || full_type.starts_with("LowCardinality(Nullable("),
                    default: expression.clone().filter(|_| kind == "DEFAULT"),
                    auto_increment: false,
                    collation: None,
                    comment: row[4].take().filter(|comment| !comment.is_empty()),
                    generation: matches!(kind.as_str(), "MATERIALIZED" | "ALIAS").then(|| {
                        super::Generation {
                            kind: Some(kind.to_lowercase()),
                            expression,
                        }
                    }),
                    full_type,
                }
            })
            .collect())
    }

    fn indexes(&mut self, table: &Table) -> anyhow::Result<Vec<Index>> {
        let row = self
            .ask(
                &format!(
                    "select primary_key, sorting_key, partition_key, engine from system.tables \
                     where database = currentDatabase() and name = {}",
                    literal(&table.name)
                ),
                false,
            )?
            .rows
            .into_iter()
            .next()
            .unwrap_or_default();
        let named = |at: usize, name: &str, primary: bool| {
            row.get(at)
                .cloned()
                .flatten()
                .filter(|expression| !expression.is_empty())
                .map(|expression| Index {
                    name: name.to_string(),
                    columns: split_list(Some(expression.replace(", ", ","))),
                    unique: false,
                    primary,
                })
        };
        Ok([
            named(0, "primary key", true),
            named(1, "sorting key", false),
            named(2, "partition key", false),
            named(3, "engine", false),
        ]
        .into_iter()
        .flatten()
        .collect())
    }

    fn foreign_keys(&mut self) -> anyhow::Result<HashMap<Table, Vec<ForeignKey>>> {
        Ok(HashMap::new())
    }

    fn databases(&mut self) -> anyhow::Result<Vec<String>> {
        Ok(self
            .ask(
                "select name from system.databases where name not in \
                 ('system', 'INFORMATION_SCHEMA', 'information_schema') order by name",
                false,
            )?
            .rows
            .into_iter()
            .filter_map(|mut row| row.get_mut(0).and_then(Option::take))
            .collect())
    }

    fn use_database(&mut self, name: &str) -> anyhow::Result<()> {
        let was = std::mem::replace(&mut self.database, name.to_string());
        if let Err(error) = self.ask("select 1", false) {
            self.database = was;
            return Err(error);
        }
        Ok(())
    }

    fn update(
        &mut self,
        _table: &Table,
        _column: &str,
        _key: &str,
        _key_value: &str,
        _value: Option<&str>,
    ) -> anyhow::Result<u64> {
        bail!(READ_ONLY)
    }

    fn insert(&mut self, _table: &Table, _values: &Values) -> anyhow::Result<()> {
        bail!(READ_ONLY)
    }

    fn delete(
        &mut self,
        _table: &Table,
        _key: &str,
        _key_values: &[String],
    ) -> anyhow::Result<u64> {
        bail!(READ_ONLY)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_names_types_and_rows_of_an_answer() {
        let read = answer(
            "[\"id\",\"tags\",\"maybe\",\"price\"]\n[\"UInt64\",\"Array(String)\",\"Nullable(Int32)\",\"Decimal(18, 4)\"]\n[\"1\",[\"a\"],null,\"12.3456\"]\n",
        )
        .unwrap();
        assert_eq!(read.headers, ["id", "tags", "maybe", "price"]);
        assert_eq!(
            read.rows,
            [[
                Some("1".to_string()),
                Some("[\"a\"]".to_string()),
                None,
                Some("12.3456".to_string())
            ]]
        );
        assert_eq!(
            read.kinds[0],
            [Kind::Integer, Kind::Text, Kind::Integer, Kind::Text]
        );
    }

    #[test]
    fn a_failure_partway_through_the_answer_is_an_error_not_short_results() {
        let failed = answer(
            "[\"n\"]\n[\"UInt8\"]\n[1]\nCode: 241. DB::Exception: Memory limit exceeded. (MEMORY_LIMIT_EXCEEDED) (version 26.9.16.11 (official build))\n",
        )
        .err()
        .unwrap();
        assert_eq!(
            failed.to_string(),
            "Memory limit exceeded. (MEMORY_LIMIT_EXCEEDED)"
        );
    }

    #[test]
    fn keeps_the_servers_words_from_an_exception() {
        assert_eq!(
            said(
                "Code: 60. DB::Exception: Unknown table expression identifier 'nope'. (UNKNOWN_TABLE) (version 26.9.16.11 (official build))"
            ),
            "Unknown table expression identifier 'nope'. (UNKNOWN_TABLE)"
        );
    }

    #[test]
    fn escapes_backslashes_and_quotes_in_literals_and_names() {
        assert_eq!(literal("O'Brien\\path"), "'O\\'Brien\\\\path'");
        assert_eq!(quote("we`ird"), "`we\\`ird`");
    }
}
