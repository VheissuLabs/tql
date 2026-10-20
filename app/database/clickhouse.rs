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
    outcome_or_affected, single_column_keys, split_list,
};
use super::{Total, first_count};
use crate::store::Connection;
use crate::tunnel::{Route, Tunnel};
use rustls::ClientConfig;

const TIMEOUT: Duration = Duration::from_secs(300);
const FORMAT: &str = "TabSeparatedWithNamesAndTypes";
const SESSION_LOCKED: &str = "SESSION_IS_LOCKED";
const UNMATCHED: [&str; 18] = [
    "float32",
    "float64",
    "bfloat16",
    "json",
    "object",
    "map",
    "array",
    "tuple",
    "nested",
    "variant",
    "dynamic",
    "aggregatefunction",
    "simpleaggregatefunction",
    "point",
    "ring",
    "linestring",
    "polygon",
    "multipolygon",
];

pub struct ClickHouse {
    base: String,
    user: String,
    password: Option<String>,
    database: String,
    tls: Option<Arc<ClientConfig>>,
    session: String,
    locked: bool,
    orders: HashMap<Table, String>,
    types: HashMap<Table, HashMap<String, String>>,
    route: Route,
    tunnel: Option<Arc<Tunnel>>,
}

#[derive(Debug)]
struct Answer {
    headers: Vec<String>,
    rows: Vec<Vec<Option<String>>>,
    kinds: Vec<Vec<Kind>>,
    written: usize,
}

impl ClickHouse {
    pub fn open(
        connection: &Connection,
        password: Option<String>,
        route: Route,
    ) -> anyhow::Result<Self> {
        let tunnel = route.tunnel()?;
        let tls = Tls::of(connection)?;
        let secure = !matches!(
            tls.mode,
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
        let mut database = ClickHouse {
            base: format!("{scheme}://{}:{port}/", bracketed(&host)),
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
            tls: match secure {
                true => Some(Arc::new(tls.client_config()?)),
                false => None,
            },
            session: session(),
            locked: false,
            orders: HashMap::new(),
            types: HashMap::new(),
            route,
            tunnel,
        };
        let readonly = database.ask("select getSetting('readonly')", false)?;
        database.locked = readonly
            .rows
            .first()
            .and_then(|row| row.first().cloned().flatten())
            .is_some_and(|level| level != "0");
        Ok(database)
    }

    fn ask(&mut self, sql: &str, reading: bool) -> anyhow::Result<Answer> {
        if let Some(gone) = self.tunnel.as_ref().filter(|tunnel| !tunnel.running()) {
            let dead = format!(":{}/", gone.port);
            let tunnel = self.route.tunnel()?;
            if let Some(alive) = &tunnel {
                self.base = self.base.replace(&dead, &format!(":{}/", alive.port));
            }
            self.tunnel = tunnel;
        }
        match self.asked(sql, reading) {
            Err(error) if error.to_string().contains(SESSION_LOCKED) => {
                self.session = session();
                self.asked(sql, reading)
            }
            answered => answered,
        }
    }

    fn asked(&self, sql: &str, reading: bool) -> anyhow::Result<Answer> {
        let mut url = format!(
            "{}?database={}&default_format={FORMAT}&session_id={}&session_timeout=3600",
            self.base,
            crate::dsn::encode(&self.database),
            self.session
        );
        if reading && !self.locked {
            url.push_str("&readonly=1");
        }
        let mut headers = vec![("X-ClickHouse-User", self.user.clone())];
        if let Some(password) = &self.password {
            headers.push(("X-ClickHouse-Key", password.clone()));
        }
        let reply = http::send(&url, &headers, sql.to_string(), TIMEOUT, self.tls.clone())?;
        if reply.status >= 400 {
            bail!("{}", said(&reply.body));
        }
        if let Some(failure) = failure(&reply.body, reply.header("X-ClickHouse-Exception-Tag")) {
            bail!("{failure}");
        }
        if let Some(broken) = &reply.broken {
            bail!("the answer broke off partway: {broken}");
        }
        let written = reply
            .header("X-ClickHouse-Summary")
            .and_then(|summary| serde_json::from_str::<Value>(summary).ok())
            .and_then(|summary| summary["written_rows"].as_str()?.parse().ok())
            .unwrap_or(0);
        let mut answer = match reply.header("X-ClickHouse-Format") {
            Some(format) if format != FORMAT => raw(format, &reply.body),
            _ => answer(&reply.body)?,
        };
        answer.written = written;
        Ok(answer)
    }

    fn page(&mut self, sql: &str) -> anyhow::Result<Page> {
        let answer = self.ask(sql, false)?;
        Ok(Page {
            statement: sql.to_string(),
            headers: answer.headers,
            rows: answer.rows,
            kinds: answer.kinds,
        })
    }

    fn outcome(&mut self, statement: &str, reading: bool) -> anyhow::Result<Outcome> {
        let answer = self.ask(statement, reading)?;
        Ok(Outcome {
            kinds: answer.kinds,
            ..outcome_or_affected(answer.headers, answer.rows, answer.written)
        })
    }
}

fn session() -> String {
    let mut random = [0u8; 16];
    let _ = getrandom::fill(&mut random);
    format!(
        "tql-{}",
        random
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>()
    )
}

fn bracketed(host: &str) -> String {
    match host.contains(':') && !host.starts_with('[') {
        true => format!("[{host}]"),
        false => host.to_string(),
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

fn failure(body: &str, tag: Option<&str>) -> Option<String> {
    let (_, after) = body.split_once("__exception__")?;
    let message = after
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && Some(*line) != tag)
        .find(|line| line.contains("Exception") || line.starts_with("Code:"))
        .unwrap_or_else(|| after.trim());
    Some(said(message))
}

fn raw(format: &str, body: &str) -> Answer {
    let rows: Vec<Vec<Option<String>>> = body
        .lines()
        .map(|line| vec![Some(line.to_string())])
        .collect();
    Answer {
        headers: vec![format.to_string()],
        kinds: vec![vec![Kind::Text]; rows.len()],
        rows,
        written: 0,
    }
}

fn answer(body: &str) -> anyhow::Result<Answer> {
    let mut lines = body.lines();
    let fields = |line: &str| -> Vec<Option<String>> { line.split('\t').map(unescaped).collect() };
    let names = |line: Option<&str>| -> Vec<String> {
        line.map(fields)
            .unwrap_or_default()
            .into_iter()
            .map(Option::unwrap_or_default)
            .collect()
    };
    let headers = names(lines.next().filter(|line| !line.is_empty()));
    let types = names(lines.next());
    let row_kinds: Vec<Kind> = types.iter().map(|kind| kind_of(kind)).collect();
    let mut rows = Vec::new();
    let mut kinds = Vec::new();
    let lines: Vec<&str> = lines.collect();
    let last = lines.iter().rposition(|line| !line.is_empty());
    for (at, line) in lines.iter().enumerate() {
        if Some(at) == last && line.starts_with("Code: ") && line.contains("DB::Exception: ") {
            bail!("{}", said(line));
        }
        let cells = fields(line);
        if cells.len() != headers.len() {
            bail!("ClickHouse sent a row tql could not read: {line}");
        }
        rows.push(cells);
        kinds.push(row_kinds.clone());
    }
    Ok(Answer {
        headers,
        rows,
        kinds,
        written: 0,
    })
}

fn unescaped(field: &str) -> Option<String> {
    if field == "\\N" {
        return None;
    }
    let mut text = String::with_capacity(field.len());
    let mut characters = field.chars();
    while let Some(character) = characters.next() {
        if character != '\\' {
            text.push(character);
            continue;
        }
        match characters.next() {
            Some('t') => text.push('\t'),
            Some('n') => text.push('\n'),
            Some('r') => text.push('\r'),
            Some('0') => text.push('\0'),
            Some('b') => text.push('\u{8}'),
            Some('f') => text.push('\u{c}'),
            Some(other) => text.push(other),
            None => text.push('\\'),
        }
    }
    Some(text)
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
    if (kind.starts_with("Int") && !kind.starts_with("Interval")) || kind.starts_with("UInt") {
        return Kind::Integer;
    }
    Kind::Text
}

fn comparable(kind: &str) -> bool {
    ![
        "Map(",
        "JSON",
        "Object(",
        "AggregateFunction(",
        "Dynamic",
        "Variant(",
    ]
    .iter()
    .any(|uncomparable| kind.contains(uncomparable))
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
        let mut order: Vec<String> = sort
            .map(|sort| format!("{} {}", quote(&sort.column), sort.direction.keyword()))
            .into_iter()
            .collect();
        order.extend(self.paging_order(table));
        let sql = format!(
            "select * from {}{}{} limit {limit} offset {offset}",
            table.quoted(quote),
            condition
                .map(|condition| format!(" where {condition}"))
                .unwrap_or_default(),
            match order.is_empty() {
                true => String::new(),
                false => format!(" order by {}", order.join(", ")),
            }
        );
        self.page(&sql)
    }

    fn paging_order(&mut self, table: &Table) -> Option<String> {
        if let Some(order) = self.orders.get(table) {
            return Some(order.clone()).filter(|order| !order.is_empty());
        }
        let key = self
            .ask(
                &format!(
                    "select sorting_key from system.tables \
                     where database = currentDatabase() and name = {}",
                    literal(&table.name)
                ),
                false,
            )
            .ok()?
            .rows
            .into_iter()
            .next()
            .and_then(|row| row.into_iter().next().flatten())
            .filter(|key| !key.is_empty());
        let comparable: Vec<String> = self
            .columns(table)
            .ok()?
            .into_iter()
            .filter(|column| column.generation.is_none() && comparable(&column.full_type))
            .map(|column| quote(&column.name))
            .collect();
        let order = key
            .into_iter()
            .chain(comparable)
            .collect::<Vec<_>>()
            .join(", ");
        self.orders.insert(table.clone(), order.clone());
        Some(order).filter(|order| !order.is_empty())
    }

    fn count_rows(&mut self, table: &Table, filters: Option<&Filters>) -> anyhow::Result<usize> {
        let condition = filters
            .and_then(|filters| filters.condition(&quote))
            .map(|condition| format!(" where {}", condition.render(literal)))
            .unwrap_or_default();
        first_count(self.page(&format!(
            "select count(*) from {}{condition}",
            table.quoted(quote)
        ))?)
        .ok_or_else(|| anyhow!("the count came back empty"))
    }

    fn run(&mut self, statement: &str) -> anyhow::Result<Outcome> {
        self.outcome(statement, false)
    }

    fn run_reading(&mut self, statement: &str) -> anyhow::Result<Outcome> {
        self.outcome(statement, true)
    }

    fn quote(&self, identifier: &str) -> String {
        quote(identifier)
    }

    fn estimated_rows(&mut self, table: &Table) -> Option<Total> {
        let page = self
            .select(
                "select total_rows from system.tables \
                 where database = currentDatabase() and name = ?",
                std::slice::from_ref(&table.name),
            )
            .ok()?;
        first_count(page).map(Total::About)
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
        table: &Table,
        column: &str,
        key: &str,
        key_value: &str,
        value: Option<&str>,
    ) -> anyhow::Result<u64> {
        let condition = format!(
            "{} = {}",
            quote(key),
            self.typed(table, key, Some(key_value))
        );
        let assigned = self.assigned(table, &vec![(column.to_string(), value.map(String::from))]);
        self.mutated(table, &format!("update {assigned}"), &condition, 1)
    }

    fn insert(&mut self, table: &Table, values: &Values) -> anyhow::Result<()> {
        let columns: Vec<String> = values.iter().map(|(column, _)| quote(column)).collect();
        let typed: Vec<String> = values
            .iter()
            .map(|(column, value)| self.typed(table, column, value.as_deref()))
            .collect();
        let statement = match values.is_empty() {
            true => format!("insert into {} values ()", table.quoted(quote)),
            false => format!(
                "insert into {} ({}) select {}",
                table.quoted(quote),
                columns.join(", "),
                typed.join(", ")
            ),
        };
        self.ask(&statement, false)?;
        Ok(())
    }

    fn delete(&mut self, table: &Table, key: &str, key_values: &[String]) -> anyhow::Result<u64> {
        let listed: Vec<String> = key_values
            .iter()
            .map(|value| self.typed(table, key, Some(value)))
            .collect();
        let condition = format!("{} in ({})", quote(key), listed.join(", "));
        self.mutated(table, "delete", &condition, key_values.len())
    }

    fn matches_rows(&self) -> bool {
        true
    }

    fn update_row(&mut self, table: &Table, row: &Values, changes: &Values) -> anyhow::Result<u64> {
        let condition = self.row_matched(table, row)?;
        let assigned = self.assigned(table, changes);
        self.mutated(table, &format!("update {assigned}"), &condition, 1)
    }

    fn delete_row(&mut self, table: &Table, row: &Values) -> anyhow::Result<u64> {
        let condition = self.row_matched(table, row)?;
        self.mutated(table, "delete", &condition, 1)
    }
}

impl ClickHouse {
    fn column_types(&mut self, table: &Table) -> HashMap<String, String> {
        if let Some(types) = self.types.get(table) {
            return types.clone();
        }
        let types: HashMap<String, String> = self
            .columns(table)
            .unwrap_or_default()
            .into_iter()
            .map(|column| (column.name, column.full_type))
            .collect();
        self.types.insert(table.clone(), types.clone());
        types
    }

    fn typed(&mut self, table: &Table, column: &str, value: Option<&str>) -> String {
        let Some(value) = value else {
            return "NULL".to_string();
        };
        let Some(kind) = self.column_types(table).get(column).cloned() else {
            return literal(value);
        };
        let tab_separated = value
            .replace('\\', "\\\\")
            .replace('\t', "\\t")
            .replace('\n', "\\n");
        format!(
            "(select v from format(TabSeparated, {}, {}))",
            literal(&format!("v {kind}")),
            literal(&tab_separated)
        )
    }

    fn assigned(&mut self, table: &Table, changes: &Values) -> String {
        changes
            .iter()
            .map(|(column, value)| {
                format!(
                    "{} = {}",
                    quote(column),
                    self.typed(table, column, value.as_deref())
                )
            })
            .collect::<Vec<_>>()
            .join(", ")
    }

    fn row_matched(&mut self, table: &Table, row: &Values) -> anyhow::Result<String> {
        let types = self.column_types(table);
        let matched: Vec<String> = row
            .iter()
            .filter_map(|(column, value)| {
                let kind = types.get(column).map(String::as_str).unwrap_or_default();
                let base = unwrapped(unwrapped(kind))
                    .split('(')
                    .next()
                    .unwrap_or_default()
                    .to_lowercase();
                match value {
                    None => Some(format!("isNull({})", quote(column))),
                    Some(_) if UNMATCHED.contains(&base.as_str()) => None,
                    Some(value) => Some(format!(
                        "{} = {}",
                        quote(column),
                        self.typed(table, column, Some(value))
                    )),
                }
            })
            .collect();
        if matched.is_empty() {
            bail!(
                "tql cannot tell this row apart: none of its columns hold a value it can match exactly"
            );
        }
        Ok(matched.join(" and "))
    }

    fn mutated(
        &mut self,
        table: &Table,
        change: &str,
        condition: &str,
        most: usize,
    ) -> anyhow::Result<u64> {
        let reached = first_count(self.page(&format!(
            "select count() from {} where {condition}",
            table.quoted(quote)
        ))?)
        .ok_or_else(|| anyhow!("the count came back empty"))?;
        if reached > most {
            bail!(
                "{reached} rows match where {most} {} meant to, so tql changed nothing. ClickHouse keys need not be unique, so rows can repeat; change them from the SQL editor.",
                if most == 1 { "was" } else { "were" }
            );
        }
        if reached == 0 {
            return Ok(0);
        }
        self.ask(
            &format!(
                "alter table {} {change} where {condition} settings mutations_sync = 2",
                table.quoted(quote)
            ),
            false,
        )?;
        Ok(reached as u64)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_names_types_and_rows_of_an_answer() {
        let read = answer(
            "id\ttags\tmaybe\tprice\tnote\nUInt64\tArray(String)\tNullable(Int32)\tDecimal(18, 4)\tString\n1\t['a']\t\\N\t12.3456\tC:\\\\dir\\ttab\\\\N\n",
        )
        .unwrap();
        assert_eq!(read.headers, ["id", "tags", "maybe", "price", "note"]);
        assert_eq!(
            read.rows,
            [[
                Some("1".to_string()),
                Some("['a']".to_string()),
                None,
                Some("12.3456".to_string()),
                Some("C:\\dir\ttab\\N".to_string()),
            ]]
        );
        assert_eq!(
            read.kinds[0],
            [
                Kind::Integer,
                Kind::Text,
                Kind::Integer,
                Kind::Text,
                Kind::Text
            ]
        );
    }

    #[test]
    fn intervals_are_not_integers() {
        assert_eq!(kind_of("IntervalDay"), Kind::Text);
        assert_eq!(kind_of("Int128"), Kind::Integer);
    }

    #[test]
    fn a_failure_partway_through_the_answer_is_an_error_not_short_results() {
        let failed = answer(
            "n\nUInt8\n1\nCode: 241. DB::Exception: Memory limit exceeded. (MEMORY_LIMIT_EXCEEDED) (version 26.9.16.11 (official build))\n",
        )
        .err()
        .unwrap();
        assert_eq!(
            failed.to_string(),
            "Memory limit exceeded. (MEMORY_LIMIT_EXCEEDED)"
        );
    }

    #[test]
    fn reads_the_servers_words_from_a_tagged_exception_mid_answer() {
        assert_eq!(
            failure(
                "n\nUInt64\n1\n\r\n__exception__\r\nabcdef\r\nCode: 395. DB::Exception: Value passed to 'throwIf' function is non-zero: boom. (FUNCTION_THROW_IF_VALUE_IS_NON_ZERO) (version 26.9.16.11 (official build))\r\n150 abcdef\r\n__exception__\r\n",
                Some("abcdef")
            )
            .unwrap(),
            "Value passed to 'throwIf' function is non-zero: boom. (FUNCTION_THROW_IF_VALUE_IS_NON_ZERO)"
        );
        assert_eq!(failure("n\nUInt8\n1\n", None), None);
    }

    #[test]
    fn an_answer_in_a_format_the_query_asked_for_is_shown_as_lines() {
        let read = raw("CSV", "1,\"a\"\n2,\"b\"\n");
        assert_eq!(read.headers, ["CSV"]);
        assert_eq!(read.rows.len(), 2);
    }

    #[test]
    fn brackets_an_ipv6_host() {
        assert_eq!(bracketed("::1"), "[::1]");
        assert_eq!(bracketed("db.example"), "db.example");
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
