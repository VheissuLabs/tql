use std::collections::HashMap;
use std::sync::LazyLock;
use std::time::Duration;

use anyhow::{Context, anyhow};
use futures_util::TryStreamExt;
use jiff::civil::{Date, DateTime};
use jiff::{Span, ToSpan};
use regex::Regex;
use tiberius::numeric::Numeric;
use tiberius::time::{DateTime2, Time};
use tiberius::{
    AuthMethod, Client, ColumnData, ColumnType, Config, EncryptionLevel, Query, QueryItem,
    QueryStream,
};
use tokio::net::TcpStream;
use tokio::runtime::Runtime;
use tokio_util::compat::{Compat, TokioAsyncWriteCompatExt};

use super::schema::keys_by_table;
use super::{
    Column, Database, Filters, ForeignKey, Index, Outcome, Page, Scalar, Scalars, Sort, Values,
    delete_statement, insert_statement, outcome_or_affected, refused, single_column_keys,
    split_list, update_statement,
};
use crate::store::Connection;

const MASTER: &str = "master";
const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);

static ORDERED: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)\border\s+by\b").expect("the order by pattern is valid"));

type Wire = Client<Compat<TcpStream>>;

pub struct SqlServer {
    runtime: Runtime,
    client: Wire,
    connection: Connection,
    password: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Encryption {
    pub encrypt: bool,
    pub trust_server_certificate: bool,
}

pub fn encryption(ssl_mode: Option<&str>) -> Encryption {
    let mode = ssl_mode.unwrap_or_default().trim();
    Encryption {
        encrypt: mode != "disable",
        trust_server_certificate: !matches!(mode, "verify-ca" | "verify-full"),
    }
}

pub fn page(statement: &str, limit: usize, offset: usize) -> String {
    let ordered = if ORDERED.is_match(statement) {
        statement.to_string()
    } else {
        format!("{statement} order by (select null)")
    };
    format!("{ordered} offset {offset} rows fetch next {limit} rows only")
}

pub fn quote_bracket(identifier: &str) -> String {
    format!("[{}]", identifier.replace(']', "]]"))
}

pub fn numbered_parameters(statement: &str) -> String {
    let mut numbered = String::with_capacity(statement.len());
    let mut closing: Option<char> = None;
    let mut position = 0;
    for character in statement.chars() {
        match closing {
            Some(close) if character == close => closing = None,
            Some(_) => {}
            None => match character {
                '\'' => closing = Some('\''),
                '"' => closing = Some('"'),
                '[' => closing = Some(']'),
                '?' => {
                    position += 1;
                    numbered.push_str(&format!("@P{position}"));
                    continue;
                }
                _ => {}
            },
        }
        numbered.push(character);
    }
    numbered
}

fn configured(connection: &Connection, password: Option<&str>, database: Option<&str>) -> Config {
    let mut config = Config::new();
    config.host(connection.host.as_deref().unwrap_or("127.0.0.1"));
    config.port(connection.port.unwrap_or(1433));
    config.application_name("tql");
    config.authentication(AuthMethod::sql_server(
        connection.username.as_deref().unwrap_or_default(),
        password.unwrap_or_default(),
    ));
    if let Some(database) = database.filter(|database| !database.trim().is_empty()) {
        config.database(database);
    }

    let chosen = encryption(connection.ssl_mode.as_deref());
    config.encryption(if chosen.encrypt {
        EncryptionLevel::Required
    } else {
        EncryptionLevel::Off
    });
    if chosen.trust_server_certificate {
        config.trust_cert();
    } else if let Some(authority) = connection
        .ssl_ca
        .as_deref()
        .filter(|authority| !authority.trim().is_empty())
    {
        config.trust_cert_ca(authority);
    }
    config
}

async fn reach(config: Config) -> tiberius::Result<Wire> {
    let stream = TcpStream::connect(config.get_addr()).await?;
    stream.set_nodelay(true)?;
    match Client::connect(config.clone(), stream.compat_write()).await {
        Err(tiberius::error::Error::Routing { host, port }) => {
            let mut routed = config;
            routed.host(&host);
            routed.port(port);
            let stream = TcpStream::connect(routed.get_addr()).await?;
            stream.set_nodelay(true)?;
            Client::connect(routed, stream.compat_write()).await
        }
        other => other,
    }
}

fn dial(runtime: &Runtime, config: Config) -> anyhow::Result<Wire> {
    runtime.block_on(async {
        tokio::time::timeout(CONNECT_TIMEOUT, reach(config))
            .await
            .map_err(|_| anyhow!("timed out"))?
            .map_err(said)
    })
}

fn said(error: tiberius::error::Error) -> anyhow::Error {
    match error {
        tiberius::error::Error::Server(token) => anyhow!(token.message().to_string()),
        other => anyhow!(other),
    }
}

impl SqlServer {
    pub fn open(connection: &Connection, password: Option<String>) -> anyhow::Result<Self> {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_io()
            .enable_time()
            .build()?;
        let client = dial(
            &runtime,
            configured(
                connection,
                password.as_deref(),
                connection.database.as_deref(),
            ),
        )
        .with_context(|| format!("cannot connect to {}", connection.describe()))?;
        Ok(SqlServer {
            runtime,
            client,
            connection: connection.clone(),
            password,
        })
    }

    fn reconnect(&self, database: &str) -> anyhow::Result<Wire> {
        dial(
            &self.runtime,
            configured(&self.connection, self.password.as_deref(), Some(database)),
        )
    }

    fn query(&mut self, statement: &str, values: &[Option<&str>]) -> anyhow::Result<Page> {
        let (headers, rows) = self.fetch(statement, values, text)?;
        Ok(Page {
            statement: statement.to_string(),
            headers,
            rows,
        })
    }

    fn fetch<Cell>(
        &mut self,
        statement: &str,
        values: &[Option<&str>],
        convert: fn(ColumnData<'static>, ColumnType) -> Cell,
    ) -> anyhow::Result<(Vec<String>, Vec<Vec<Cell>>)> {
        let client = &mut self.client;
        self.runtime.block_on(async {
            let stream = if values.is_empty() {
                client.simple_query(statement).await.map_err(said)?
            } else {
                let mut query = Query::new(numbered_parameters(statement));
                for value in values {
                    query.bind(*value);
                }
                query.query(client).await.map_err(said)?
            };
            first_result(stream, convert).await
        })
    }

    fn execute(&mut self, statement: &str, values: &[Option<&str>]) -> anyhow::Result<u64> {
        let client = &mut self.client;
        self.runtime.block_on(async {
            let mut query = Query::new(numbered_parameters(statement));
            for value in values {
                query.bind(*value);
            }
            Ok(query.execute(client).await.map_err(said)?.total())
        })
    }

    fn column(&mut self, statement: &str) -> anyhow::Result<Vec<String>> {
        Ok(self
            .query(statement, &[])?
            .rows
            .into_iter()
            .filter_map(|mut row| row.swap_remove(0))
            .collect())
    }

    fn write(&mut self, statement: &str, values: &[Option<&str>]) -> anyhow::Result<()> {
        self.execute(statement, values)
            .map(|_| ())
            .map_err(|error| refused(error, statement))
    }
}

async fn first_result<Cell>(
    mut stream: QueryStream<'_>,
    convert: fn(ColumnData<'static>, ColumnType) -> Cell,
) -> anyhow::Result<(Vec<String>, Vec<Vec<Cell>>)> {
    let mut headers = Vec::new();
    let mut types = Vec::new();
    let mut rows = Vec::new();
    while let Some(item) = stream.try_next().await.map_err(said)? {
        match item {
            QueryItem::Metadata(metadata) if metadata.result_index() == 0 => {
                headers = metadata
                    .columns()
                    .iter()
                    .map(|column| column.name().to_string())
                    .collect();
                types = metadata
                    .columns()
                    .iter()
                    .map(|column| column.column_type())
                    .collect();
            }
            QueryItem::Row(row) if row.result_index() == 0 => {
                rows.push(
                    row.into_iter()
                        .zip(&types)
                        .map(|(value, column_type)| convert(value, *column_type))
                        .collect(),
                );
            }
            _ => {}
        }
    }
    Ok((headers, rows))
}

fn decimal(numeric: Numeric) -> String {
    let scale = numeric.scale() as usize;
    let digits = numeric.value().unsigned_abs().to_string();
    let sign = if numeric.value() < 0 { "-" } else { "" };
    if scale == 0 {
        return format!("{sign}{digits}");
    }
    let padded = format!("{digits:0>width$}", width = scale + 1);
    let (whole, fraction) = padded.split_at(padded.len() - scale);
    format!("{sign}{whole}.{fraction}")
}

fn day(days_since_year_one: i64) -> Date {
    Date::constant(1, 1, 1)
        .checked_add(days_since_year_one.days())
        .unwrap_or(Date::MAX)
}

fn day_since_1900(days: i64) -> Date {
    Date::constant(1900, 1, 1)
        .checked_add(days.days())
        .unwrap_or(Date::MAX)
}

fn clock(time: Time) -> String {
    let scale = time.scale() as u32;
    let per_second = 10u64.pow(scale);
    let seconds = time.increments() / per_second;
    let fraction = time.increments() % per_second;
    let whole = format!(
        "{:02}:{:02}:{:02}",
        seconds / 3600,
        seconds / 60 % 60,
        seconds % 60
    );
    if scale == 0 {
        return whole;
    }
    format!("{whole}.{fraction:0>width$}", width = scale as usize)
}

fn moment(datetime: DateTime2) -> String {
    format!(
        "{} {}",
        day(i64::from(datetime.date().days())),
        clock(datetime.time())
    )
}

fn offset_moment(datetime: DateTime2, offset_minutes: i16) -> String {
    let scale = datetime.time().scale() as u32;
    let per_second = 10u64.pow(scale) as i64;
    let increments = datetime.time().increments() as i64;
    let utc = DateTime::from(day(i64::from(datetime.date().days())))
        .checked_add(Span::new().seconds(increments / per_second))
        .unwrap_or(DateTime::MAX);
    let local = utc
        .checked_add(Span::new().minutes(i64::from(offset_minutes)))
        .unwrap_or(DateTime::MAX);
    let seconds_into_day =
        i64::from(local.hour()) * 3600 + i64::from(local.minute()) * 60 + i64::from(local.second());
    let shown_time = clock(Time::new(
        (seconds_into_day * per_second + increments % per_second) as u64,
        scale as u8,
    ));
    let sign = if offset_minutes < 0 { '-' } else { '+' };
    let magnitude = offset_minutes.unsigned_abs();
    format!(
        "{} {shown_time} {sign}{:02}:{:02}",
        local.date(),
        magnitude / 60,
        magnitude % 60
    )
}

fn hexadecimal(bytes: &[u8]) -> String {
    let digits: String = bytes.iter().map(|byte| format!("{byte:02X}")).collect();
    format!("0x{digits}")
}

fn text(value: ColumnData<'static>, column_type: ColumnType) -> Option<String> {
    Some(match value {
        ColumnData::U8(number) => number?.to_string(),
        ColumnData::I16(number) => number?.to_string(),
        ColumnData::I32(number) => number?.to_string(),
        ColumnData::I64(number) => number?.to_string(),
        ColumnData::F32(number) => number?.to_string(),
        ColumnData::F64(number)
            if matches!(column_type, ColumnType::Money | ColumnType::Money4) =>
        {
            format!("{:.4}", number?)
        }
        ColumnData::F64(number) => number?.to_string(),
        ColumnData::Bit(bit) => if bit? { "1" } else { "0" }.to_string(),
        ColumnData::String(text) => text?.into_owned(),
        ColumnData::Guid(guid) => guid?.to_string().to_uppercase(),
        ColumnData::Binary(bytes) => hexadecimal(&bytes?),
        ColumnData::Numeric(numeric) => decimal(numeric?),
        ColumnData::Xml(xml) => xml?.to_string(),
        ColumnData::DateTime(datetime) => {
            let datetime = datetime?;
            let milliseconds = (u64::from(datetime.seconds_fragments()) * 1000 + 150) / 300;
            format!(
                "{} {}",
                day_since_1900(i64::from(datetime.days())),
                clock(Time::new(milliseconds, 3))
            )
        }
        ColumnData::SmallDateTime(datetime) => {
            let datetime = datetime?;
            format!(
                "{} {}",
                day_since_1900(i64::from(datetime.days())),
                clock(Time::new(u64::from(datetime.seconds_fragments()) * 60, 0))
            )
        }
        ColumnData::Time(time) => clock(time?),
        ColumnData::Date(date) => day(i64::from(date?.days())).to_string(),
        ColumnData::DateTime2(datetime) => moment(datetime?),
        ColumnData::DateTimeOffset(datetime) => {
            let datetime = datetime?;
            offset_moment(datetime.datetime2(), datetime.offset())
        }
    })
}

fn scalar(value: ColumnData<'static>, column_type: ColumnType) -> Scalar {
    match value {
        ColumnData::Binary(Some(bytes)) => Scalar::Bytes(bytes.into_owned()),
        ColumnData::U8(_)
        | ColumnData::I16(_)
        | ColumnData::I32(_)
        | ColumnData::I64(_)
        | ColumnData::Bit(_)
        | ColumnData::Numeric(_) => text(value, column_type)
            .map(Scalar::Number)
            .unwrap_or(Scalar::Null),
        ColumnData::F32(Some(number)) => Scalar::real(f64::from(number)),
        ColumnData::F64(Some(number))
            if !matches!(column_type, ColumnType::Money | ColumnType::Money4) =>
        {
            Scalar::real(number)
        }
        ColumnData::F64(Some(number)) => Scalar::Number(format!("{number:.4}")),
        other => Scalar::text(text(other, column_type)),
    }
}

fn flag(value: &Option<String>) -> bool {
    value.as_deref() == Some("1")
}

impl Database for SqlServer {
    fn tables(&mut self) -> anyhow::Result<Vec<String>> {
        self.column(
            "select t.name as name from sys.tables as t \
             where t.is_ms_shipped = 0 and t.name <> 'sysdiagrams' \
             and schema_name(t.schema_id) = schema_name() order by t.name",
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
        let condition = filters.and_then(|filters| filters.condition(&quote_bracket));
        let statement = |shown: Option<String>| {
            let filter = shown
                .map(|condition| format!(" where {condition}"))
                .unwrap_or_default();
            let order = sort
                .map(|sort| {
                    format!(
                        " order by {} {}",
                        quote_bracket(&sort.column),
                        sort.direction.keyword()
                    )
                })
                .unwrap_or_default();
            page(
                &format!("select * from {}{filter}{order}", quote_bracket(table)),
                limit,
                offset,
            )
        };
        let Some(condition) = condition else {
            return self.query(&statement(None), &[]);
        };
        let bindings = condition.bindings();
        let values: Vec<Option<&str>> = bindings.iter().map(|value| Some(value.as_str())).collect();
        let mut rows = self.query(&statement(Some(condition.placeheld())), &values)?;
        rows.statement = statement(Some(condition.readable()));
        Ok(rows)
    }

    fn primary_keys(&mut self) -> anyhow::Result<HashMap<String, String>> {
        let pairs = self
            .query(
                "select tbl.name, col.name from sys.indexes as idx \
                 join sys.tables as tbl on idx.object_id = tbl.object_id \
                 join sys.index_columns as idxcol on idx.object_id = idxcol.object_id and idx.index_id = idxcol.index_id \
                 join sys.columns as col on idxcol.object_id = col.object_id and idxcol.column_id = col.column_id \
                 where idx.is_primary_key = 1 and schema_name(tbl.schema_id) = schema_name() \
                 order by tbl.name, idxcol.key_ordinal",
                &[],
            )?
            .rows
            .into_iter()
            .filter_map(|row| Some((row.first()?.clone()?, row.get(1)?.clone()?)))
            .collect();
        Ok(single_column_keys(pairs))
    }

    fn select(&mut self, statement: &str, values: &[String]) -> anyhow::Result<Page> {
        let bound: Vec<Option<&str>> = values.iter().map(|value| Some(value.as_str())).collect();
        self.query(statement, &bound)
    }

    fn columns(&mut self, table: &str) -> anyhow::Result<Vec<Column>> {
        Ok(self
            .select(
                "select col.name, type.name as type_name, col.is_nullable as nullable, \
                 def.definition as [default], col.is_identity as autoincrement \
                 from sys.columns as col \
                 join sys.types as type on col.user_type_id = type.user_type_id \
                 join sys.objects as obj on col.object_id = obj.object_id \
                 join sys.schemas as scm on obj.schema_id = scm.schema_id \
                 left join sys.default_constraints def on col.default_object_id = def.object_id and col.object_id = def.parent_object_id \
                 where obj.type in ('U', 'V') and obj.name = ? and scm.name = schema_name() \
                 order by col.column_id",
                &[table.to_string()],
            )?
            .rows
            .into_iter()
            .map(|mut row| Column {
                name: row[0].take().unwrap_or_default(),
                type_name: row[1].take().unwrap_or_default(),
                nullable: flag(&row[2]),
                default: row[3].take(),
                auto_increment: flag(&row[4]),
            })
            .collect())
    }

    fn indexes(&mut self, table: &str) -> anyhow::Result<Vec<Index>> {
        Ok(self
            .select(
                "select idx.name as name, string_agg(col.name, ',') within group (order by idxcol.key_ordinal) as columns, \
                 idx.is_unique as [unique], idx.is_primary_key as [primary] \
                 from sys.indexes as idx \
                 join sys.tables as tbl on idx.object_id = tbl.object_id \
                 join sys.schemas as scm on tbl.schema_id = scm.schema_id \
                 join sys.index_columns as idxcol on idx.object_id = idxcol.object_id and idx.index_id = idxcol.index_id \
                 join sys.columns as col on idxcol.object_id = col.object_id and idxcol.column_id = col.column_id \
                 where tbl.name = ? and scm.name = schema_name() \
                 group by idx.name, idx.type_desc, idx.is_unique, idx.is_primary_key",
                &[table.to_string()],
            )?
            .rows
            .into_iter()
            .map(|mut row| Index {
                name: row[0].take().unwrap_or_default().to_lowercase(),
                columns: split_list(row[1].take()),
                unique: flag(&row[2]),
                primary: flag(&row[3]),
            })
            .collect())
    }

    fn foreign_keys(&mut self) -> anyhow::Result<HashMap<String, Vec<ForeignKey>>> {
        Ok(keys_by_table(
            self.query(
                "select lt.name as [table], \
                 string_agg(lc.name, ',') within group (order by fkc.constraint_column_id) as columns, \
                 ft.name as foreign_table, \
                 string_agg(fc.name, ',') within group (order by fkc.constraint_column_id) as foreign_columns \
                 from sys.foreign_keys as fk \
                 join sys.foreign_key_columns as fkc on fkc.constraint_object_id = fk.object_id \
                 join sys.tables as lt on lt.object_id = fk.parent_object_id \
                 join sys.schemas as ls on lt.schema_id = ls.schema_id \
                 join sys.columns as lc on fkc.parent_object_id = lc.object_id and fkc.parent_column_id = lc.column_id \
                 join sys.tables as ft on ft.object_id = fk.referenced_object_id \
                 join sys.columns as fc on fkc.referenced_object_id = fc.object_id and fkc.referenced_column_id = fc.column_id \
                 where ls.name = schema_name() \
                 group by lt.name, fk.name, ft.name",
                &[],
            )?
            .rows,
        ))
    }

    fn databases(&mut self) -> anyhow::Result<Vec<String>> {
        self.column("select name from sys.databases order by name")
    }

    fn use_database(&mut self, name: &str) -> anyhow::Result<()> {
        self.client = self
            .reconnect(name)
            .with_context(|| format!("cannot switch to {name}"))?;
        self.connection.database = Some(name.to_string());
        Ok(())
    }

    fn run(&mut self, statement: &str) -> anyhow::Result<Outcome> {
        let (headers, rows) = self.fetch(statement, &[], text)?;
        if !headers.is_empty() {
            return Ok(outcome_or_affected(headers, rows, 0));
        }
        let affected = self
            .column("select @@rowcount")?
            .first()
            .and_then(|count| count.parse().ok())
            .unwrap_or_default();
        Ok(outcome_or_affected(Vec::new(), Vec::new(), affected))
    }

    fn run_server_wide(&mut self, statement: &str) -> anyhow::Result<Outcome> {
        let mut master = self.reconnect(MASTER)?;
        self.runtime.block_on(async {
            first_result(master.simple_query(statement).await.map_err(said)?, text).await
        })?;
        Ok(Outcome::default())
    }

    fn quote(&self, identifier: &str) -> String {
        quote_bracket(identifier)
    }

    fn page(&self, statement: &str, limit: usize, offset: usize) -> String {
        page(statement, limit, offset)
    }

    fn scalars(&mut self, statement: &str) -> anyhow::Result<Scalars> {
        let (headers, rows) = self.fetch(statement, &[], scalar)?;
        Ok(Scalars { headers, rows })
    }

    fn highest(&mut self, table: &str, column: &str) -> anyhow::Result<Option<String>> {
        let page = self.query(
            &format!(
                "select max({}) from {}",
                quote_bracket(column),
                quote_bracket(table)
            ),
            &[],
        )?;
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
        let statement = update_statement(quote_bracket, question, table, column, key);
        self.write(&statement, &[value, Some(key_value)])
    }

    fn insert(&mut self, table: &str, values: &Values) -> anyhow::Result<()> {
        let statement = insert_statement(quote_bracket, question, table, values, "default values");
        let bound: Vec<Option<&str>> = values.iter().map(|(_, value)| value.as_deref()).collect();
        self.write(&statement, &bound)
    }

    fn delete(&mut self, table: &str, key: &str, key_values: &[String]) -> anyhow::Result<()> {
        let statement = delete_statement(quote_bracket, question, table, key, key_values.len());
        let bound: Vec<Option<&str>> = key_values
            .iter()
            .map(|value| Some(value.as_str()))
            .collect();
        self.write(&statement, &bound)
    }
}

fn question(_: usize) -> String {
    "?".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encrypts_the_way_the_same_ssl_mode_does_for_postgres() {
        let expectations = [
            (None, true, true),
            (Some("disable"), false, true),
            (Some("prefer"), true, true),
            (Some("require"), true, true),
            (Some("verify-ca"), true, false),
            (Some("verify-full"), true, false),
            (Some(" verify-full "), true, false),
        ];
        for (mode, encrypt, trust_server_certificate) in expectations {
            assert_eq!(
                encryption(mode),
                Encryption {
                    encrypt,
                    trust_server_certificate
                },
                "{mode:?}"
            );
        }
    }

    #[test]
    fn pages_with_offset_and_fetch_giving_it_the_order_it_insists_on() {
        assert_eq!(
            page("select * from [fruit]", 50, 0),
            "select * from [fruit] order by (select null) offset 0 rows fetch next 50 rows only"
        );
        assert_eq!(
            page("select * from [fruit] order by [name] desc", 50, 100),
            "select * from [fruit] order by [name] desc offset 100 rows fetch next 50 rows only"
        );
    }

    #[test]
    fn quotes_with_brackets_doubling_a_closing_one() {
        assert_eq!(quote_bracket("fruit"), "[fruit]");
        assert_eq!(quote_bracket("odd]name"), "[odd]]name]");
    }

    #[test]
    fn numbers_placeholders_outside_quotes() {
        assert_eq!(
            numbered_parameters("select * from [a?] where x = ? and y = '?' and z in (?, ?)"),
            "select * from [a?] where x = @P1 and y = '?' and z in (@P2, @P3)"
        );
    }

    #[test]
    fn writes_decimals_with_a_leading_zero_and_their_scale() {
        assert_eq!(decimal(Numeric::new_with_scale(50, 2)), "0.50");
        assert_eq!(decimal(Numeric::new_with_scale(-5, 2)), "-0.05");
        assert_eq!(decimal(Numeric::new_with_scale(1250, 2)), "12.50");
        assert_eq!(decimal(Numeric::new_with_scale(42, 0)), "42");
    }

    #[test]
    fn writes_dates_and_times_the_way_sql_server_spells_them() {
        let datetime = ColumnData::DateTime(Some(tiberius::time::DateTime::new(45291, 3_349_620)));
        assert_eq!(
            text(datetime, ColumnType::Datetimen).as_deref(),
            Some("2024-01-02 03:06:05.400")
        );
        let small = ColumnData::SmallDateTime(Some(tiberius::time::SmallDateTime::new(45291, 184)));
        assert_eq!(
            text(small, ColumnType::Datetime4).as_deref(),
            Some("2024-01-02 03:04:00")
        );
        let date = tiberius::time::Date::new(738_886);
        assert_eq!(
            text(ColumnData::Date(Some(date)), ColumnType::Daten).as_deref(),
            Some("2024-01-02")
        );
        let time = Time::new(11_045_000_000 + 1, 7);
        assert_eq!(
            text(ColumnData::Time(Some(time)), ColumnType::Timen).as_deref(),
            Some("00:18:24.5000001")
        );
        let offset = tiberius::time::DateTimeOffset::new(
            DateTime2::new(date, Time::new(3600 * 23 * 10, 1)),
            120,
        );
        assert_eq!(
            text(
                ColumnData::DateTimeOffset(Some(offset)),
                ColumnType::DatetimeOffsetn
            )
            .as_deref(),
            Some("2024-01-03 01:00:00.0 +02:00")
        );
    }

    #[test]
    fn writes_money_to_four_places_bits_as_digits_and_bytes_as_hex() {
        assert_eq!(
            text(ColumnData::F64(Some(12.5)), ColumnType::Money).as_deref(),
            Some("12.5000")
        );
        assert_eq!(
            text(ColumnData::F64(Some(1.5)), ColumnType::Float8).as_deref(),
            Some("1.5")
        );
        assert_eq!(
            text(ColumnData::Bit(Some(true)), ColumnType::Bitn).as_deref(),
            Some("1")
        );
        assert_eq!(
            text(
                ColumnData::Binary(Some(vec![1, 171].into())),
                ColumnType::BigVarBin
            )
            .as_deref(),
            Some("0x01AB")
        );
        assert_eq!(text(ColumnData::I32(None), ColumnType::Intn), None);
    }

    #[test]
    fn exports_numbers_bare_and_bytes_as_bytes() {
        assert_eq!(
            scalar(ColumnData::I32(Some(7)), ColumnType::Intn),
            Scalar::Number("7".into())
        );
        assert_eq!(
            scalar(
                ColumnData::Binary(Some(vec![1].into())),
                ColumnType::BigVarBin
            ),
            Scalar::Bytes(vec![1])
        );
        assert_eq!(
            scalar(ColumnData::String(None), ColumnType::NVarchar),
            Scalar::Null
        );
    }
}
