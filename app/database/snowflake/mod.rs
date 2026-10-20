use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use anyhow::{anyhow, bail};
use serde_json::{Map, Value, json};

use super::http::{self, Reply};
use super::lexed::writing_word;
use super::schema::fill;
use super::snowflake_oauth::{Save, Session as OAuth};
use super::{
    Column, Database, Filters, ForeignKey, Index, Kind, Outcome, Page, Sort, Table, Total, Values,
    first_count, is_read_only, keyed, outcome_or_affected, quote_double, single_column_keys,
};
use crate::paths;
use crate::store::Connection;

mod browser;
mod keypair;
mod session;

pub use browser::{Opener, open_with};
use session::{Keeper, Place, Sent, Session};

const TIMEOUT: Duration = Duration::from_secs(300);
const DOMAIN: &str = ".snowflakecomputing.com";
const FIRST_WAIT: Duration = Duration::from_millis(250);
const LONGEST_WAIT: Duration = Duration::from_secs(2);
const FRESH_FOR: Duration = Duration::from_secs(300);
const TOKEN_HINT: &str = "Check the access token saved on this connection.";
const KEY_HINT: &str =
    "Check that the user's RSA_PUBLIC_KEY in Snowflake matches the private key on this connection.";
const SIGN_IN_HINT: &str = "Sign in again: sign out of this connection and open it.";
const UNMATCHED: [&str; 9] = [
    "float",
    "variant",
    "object",
    "array",
    "map",
    "binary",
    "geography",
    "geometry",
    "vector",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Authenticator {
    #[default]
    Token,
    KeyPair,
    OAuth,
    Browser,
}

impl Authenticator {
    pub const ALL: [Authenticator; 4] = [
        Authenticator::Token,
        Authenticator::KeyPair,
        Authenticator::OAuth,
        Authenticator::Browser,
    ];

    pub fn parse(value: Option<&str>) -> Authenticator {
        match value
            .unwrap_or_default()
            .trim()
            .to_lowercase()
            .replace([' ', '-'], "_")
            .as_str()
        {
            "snowflake_jwt" | "jwt" | "keypair" | "key_pair" => Authenticator::KeyPair,
            "oauth_authorization_code" | "oauth" | "browser_sign_in" => Authenticator::OAuth,
            "externalbrowser" | "external_browser" | "sso" | "browser_sso" | "saml" => {
                Authenticator::Browser
            }
            _ => Authenticator::Token,
        }
    }

    pub fn chosen(value: Option<&str>, secret: Option<&str>) -> Authenticator {
        match Authenticator::parse(value) {
            Authenticator::Token if secret.is_none_or(|secret| secret.trim().is_empty()) => {
                Authenticator::OAuth
            }
            chosen => chosen,
        }
    }

    pub fn stored(self) -> Option<&'static str> {
        match self {
            Authenticator::Token => None,
            Authenticator::KeyPair => Some("snowflake_jwt"),
            Authenticator::OAuth => Some("oauth_authorization_code"),
            Authenticator::Browser => Some("externalbrowser"),
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Authenticator::Token => "access token",
            Authenticator::KeyPair => "key pair",
            Authenticator::OAuth => "browser sign-in",
            Authenticator::Browser => "SSO through the browser",
        }
    }

    pub fn signs_in_with_browser(self) -> bool {
        matches!(self, Authenticator::OAuth | Authenticator::Browser)
    }
}

enum Auth {
    Token(String),
    KeyPair {
        signer: keypair::Signer,
        minted: Mutex<Option<(String, Instant)>>,
    },
    OAuth(OAuth),
    Browser(Session),
}

pub struct Snowflake {
    client: http::Client,
    base: String,
    auth: Auth,
    database: Option<String>,
    warehouse: Option<String>,
    role: Option<String>,
    schema: Option<String>,
    orders: HashMap<Table, Vec<String>>,
    types: HashMap<Table, HashMap<String, String>>,
    keys: Option<HashMap<Table, Vec<String>>>,
    early_keys: Option<HashMap<Table, Vec<String>>>,
    early_links: Option<HashMap<Table, Vec<ForeignKey>>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Field {
    name: String,
    kind: String,
    scale: usize,
}

struct Link {
    key: (String, String, String),
    sequence: usize,
    column: String,
    target: (String, String),
    reference: String,
}

#[derive(Debug, Default)]
struct Answer {
    headers: Vec<String>,
    rows: Vec<Vec<Option<String>>>,
    kinds: Vec<Vec<Kind>>,
    written: Option<usize>,
}

impl Answer {
    fn picked(self, names: &[&str]) -> Vec<Vec<Option<String>>> {
        let positions: Vec<Option<usize>> = names
            .iter()
            .map(|name| {
                self.headers
                    .iter()
                    .position(|header| header.eq_ignore_ascii_case(name))
            })
            .collect();
        self.rows
            .into_iter()
            .map(|mut row| {
                positions
                    .iter()
                    .map(|position| position.and_then(|at| row.get_mut(at)?.take()))
                    .collect()
            })
            .collect()
    }

    fn first(self) -> Option<String> {
        self.rows.into_iter().next()?.into_iter().next().flatten()
    }
}

impl Snowflake {
    pub fn open(
        connection: &Connection,
        secret: Option<String>,
        sign_in: Option<String>,
        save: Option<Save>,
    ) -> anyhow::Result<Self> {
        let account = connection
            .host
            .as_deref()
            .map(str::trim)
            .filter(|host| !host.is_empty())
            .ok_or_else(|| anyhow!("Snowflake needs an account, such as myorg-myaccount"))?;
        let named = |value: &Option<String>| {
            value
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(String::from)
        };
        let base = base_url(account);
        let role = named(&connection.role);
        let authenticator =
            Authenticator::chosen(connection.authenticator.as_deref(), secret.as_deref());
        let user = || {
            named(&connection.username).ok_or_else(|| {
                anyhow!(
                    "Snowflake {} needs the user's login name",
                    authenticator.label()
                )
            })
        };
        let auth = match authenticator {
            Authenticator::Token => match secret.filter(|token| !token.trim().is_empty()) {
                Some(token) => Auth::Token(token.trim().to_string()),
                None => bail!(
                    "Snowflake needs a programmatic access token, saved as the connection's password"
                ),
            },
            Authenticator::KeyPair => {
                let user = user()?;
                let path = named(&connection.private_key).ok_or_else(|| {
                    anyhow!("Snowflake key pair sign-in needs the path to the user's private key")
                })?;
                Auth::KeyPair {
                    signer: keypair::Signer::load(
                        &path,
                        secret.as_deref(),
                        &account_name(account),
                        &user,
                    )?,
                    minted: Mutex::new(None),
                }
            }
            Authenticator::OAuth => Auth::OAuth(OAuth::start(
                &base,
                role.as_deref(),
                sign_in.as_deref(),
                save,
            )?),
            Authenticator::Browser => Auth::Browser(Session::open(
                base.clone(),
                account_name(account),
                user()?,
                &Place {
                    role: role.clone(),
                    warehouse: named(&connection.warehouse),
                    database: named(&connection.database),
                    schema: None,
                },
                (connection.id > 0).then(|| Keeper {
                    store: paths::store(),
                    key: paths::key(),
                    id: connection.id,
                }),
                sign_in,
                TIMEOUT,
            )?),
        };
        let mut database = Snowflake {
            client: http::Client::new(TIMEOUT),
            base,
            auth,
            database: named(&connection.database),
            warehouse: named(&connection.warehouse),
            role,
            schema: None,
            orders: HashMap::new(),
            types: HashMap::new(),
            keys: None,
            early_keys: None,
            early_links: None,
        };
        database.settle_schema()?;
        Ok(database)
    }

    fn settle_schema(&mut self) -> anyhow::Result<()> {
        self.schema = None;
        self.orders.clear();
        self.keys = None;
        self.early_keys = None;
        self.early_links = None;
        let this = &*self;
        let (current, public) = std::thread::scope(|scope| {
            let public = scope.spawn(|| {
                this.database
                    .as_ref()
                    .map(|_| this.ask("show terse schemas like 'PUBLIC'"))
                    .transpose()
            });
            let current = this.ask("select current_schema()");
            (
                current,
                public
                    .join()
                    .unwrap_or_else(|_| Err(anyhow!("looking for the PUBLIC schema failed"))),
            )
        });
        self.schema = match current?.first() {
            Some(schema) => Some(schema),
            None => public?.and_then(|answer| {
                answer
                    .picked(&["name"])
                    .into_iter()
                    .next()
                    .and_then(|mut row| row[0].take())
            }),
        };
        Ok(())
    }

    fn bearer(&self) -> anyhow::Result<(String, Option<&'static str>)> {
        Ok(match &self.auth {
            Auth::Token(token) => (token.clone(), None),
            Auth::KeyPair { signer, minted } => {
                let mut minted = minted
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
                let token = match minted.as_ref() {
                    Some((token, at)) if at.elapsed() < keypair::LIFETIME - FRESH_FOR => {
                        token.clone()
                    }
                    _ => {
                        let token = signer
                            .token(jiff::Timestamp::now().as_second().max(0).unsigned_abs())?;
                        *minted = Some((token.clone(), Instant::now()));
                        token
                    }
                };
                (token, Some("KEYPAIR_JWT"))
            }
            Auth::OAuth(session) => (session.access_token()?, Some("OAUTH")),
            Auth::Browser(_) => bail!("a browser SSO session does not use the SQL API"),
        })
    }

    fn headers(
        &self,
        (token, kind): &(String, Option<&'static str>),
        body: bool,
    ) -> Vec<(&'static str, String)> {
        let mut headers = vec![
            ("Authorization", format!("Bearer {token}")),
            ("Accept", "application/json".to_string()),
        ];
        if body {
            headers.push(("Content-Type", "application/json".to_string()));
        }
        if let Some(kind) = kind {
            headers.push(("X-Snowflake-Authorization-Token-Type", kind.to_string()));
        }
        headers
    }

    fn sent(
        &self,
        send: impl Fn(&(String, Option<&'static str>)) -> anyhow::Result<Reply>,
    ) -> anyhow::Result<Reply> {
        let bearer = self.bearer()?;
        let reply = send(&bearer)?;
        match &self.auth {
            Auth::OAuth(session) if reply.status == 401 => {
                session.renew(&bearer.0)?;
                send(&self.bearer()?)
            }
            _ => Ok(reply),
        }
    }

    fn hint(&self) -> &'static str {
        match self.auth {
            Auth::KeyPair { .. } => KEY_HINT,
            Auth::OAuth(_) | Auth::Browser(_) => SIGN_IN_HINT,
            Auth::Token(_) => TOKEN_HINT,
        }
    }

    fn request(&self, sql: &str) -> String {
        let mut body = Map::new();
        body.insert("statement".to_string(), json!(sql));
        body.insert("timeout".to_string(), json!(TIMEOUT.as_secs()));
        for (key, value) in [
            ("database", &self.database),
            ("schema", &self.schema),
            ("warehouse", &self.warehouse),
            ("role", &self.role),
        ] {
            if let Some(value) = value {
                body.insert(key.to_string(), json!(value));
            }
        }
        Value::Object(body).to_string()
    }

    fn ask(&self, sql: &str) -> anyhow::Result<Answer> {
        if let Auth::Browser(session) = &self.auth {
            return Ok(answer_of(session.ask(
                sql,
                &Place {
                    role: self.role.clone(),
                    warehouse: self.warehouse.clone(),
                    database: self.database.clone(),
                    schema: self.schema.clone(),
                },
            )?));
        }
        let url = format!("{}/api/v2/statements", self.base);
        let body = self.request(sql);
        let reply = self.sent(|bearer| {
            self.client
                .post(&url, &self.headers(bearer, true), body.clone())
        })?;
        let body = self.settled(reply)?;
        let fields = fields(&body["resultSetMetaData"]["rowType"]);
        let mut rows = rows(&body["data"], &fields);
        let partitions = body["resultSetMetaData"]["partitionInfo"]
            .as_array()
            .map_or(0, Vec::len);
        if partitions > 1 {
            let handle = body["statementHandle"]
                .as_str()
                .ok_or_else(|| anyhow!("Snowflake sent part of an answer with no handle"))?;
            for partition in 1..partitions {
                let part = self.fetched(&format!(
                    "{}/api/v2/statements/{handle}?partition={partition}",
                    self.base
                ))?;
                rows.extend(partition_rows(self.parsed(part)?, &fields)?);
            }
        }
        Ok(Answer {
            headers: fields.iter().map(|field| field.name.clone()).collect(),
            kinds: vec![fields.iter().map(kind_of).collect(); rows.len()],
            rows,
            written: written(&body),
        })
    }

    fn settled(&self, reply: Reply) -> anyhow::Result<Value> {
        let started = Instant::now();
        let mut wait = FIRST_WAIT;
        let mut reply = reply;
        loop {
            if reply.status != 202 && reply.status != 429 {
                return self.parsed(reply);
            }
            let status = serde_json::from_str::<Value>(&reply.body).unwrap_or_default();
            let Some(place) = status["statementStatusUrl"].as_str() else {
                bail!("{}", said(&reply, self.hint()));
            };
            if started.elapsed() > TIMEOUT {
                bail!(
                    "Snowflake was still running the statement after {} seconds",
                    TIMEOUT.as_secs()
                );
            }
            std::thread::sleep(wait);
            wait = (wait * 2).min(LONGEST_WAIT);
            reply = self.fetched(&format!("{}{place}", self.base))?;
        }
    }

    fn fetched(&self, url: &str) -> anyhow::Result<Reply> {
        self.sent(|bearer| self.client.get(url, &self.headers(bearer, false)))
    }

    fn parsed(&self, reply: Reply) -> anyhow::Result<Value> {
        if reply.status >= 400 {
            bail!("{}", said(&reply, self.hint()));
        }
        serde_json::from_str(&reply.body).map_err(|_| {
            anyhow!(
                "Snowflake sent an answer tql could not read: {}",
                reply.body.trim()
            )
        })
    }

    fn page(&self, sql: &str) -> anyhow::Result<Page> {
        let answer = self.ask(sql)?;
        Ok(Page {
            statement: sql.to_string(),
            headers: answer.headers,
            rows: answer.rows,
            kinds: answer.kinds,
        })
    }

    fn listed(&self, table: &Table) -> anyhow::Result<Option<(Option<String>, Option<String>)>> {
        let place = table
            .schema
            .as_ref()
            .or(self.schema.as_ref())
            .map(|schema| format!(" in schema {}", quote_double(schema)))
            .unwrap_or_default();
        Ok(self
            .ask(&format!("show tables like {}{place}", literal(&table.name)))?
            .picked(&["name", "rows", "cluster_by"])
            .into_iter()
            .find(|row| row[0].as_deref() == Some(table.name.as_str()))
            .map(|mut row| (row[1].take(), row[2].take().filter(|key| !key.is_empty()))))
    }

    fn table_of(&self, schema: Option<String>, name: String) -> Table {
        match schema.filter(|schema| Some(schema) != self.schema.as_ref()) {
            Some(schema) => Table::in_schema(schema, name),
            None => Table::bare(name),
        }
    }

    fn fetch_keys(&self) -> anyhow::Result<HashMap<Table, Vec<String>>> {
        if self.database.is_none() {
            return Ok(HashMap::new());
        }
        let mut keys: Vec<(Option<String>, String, usize, String)> = self
            .ask("show primary keys in database")?
            .picked(&["schema_name", "table_name", "key_sequence", "column_name"])
            .into_iter()
            .filter_map(|mut row| {
                Some((
                    row[0].take(),
                    row[1].take()?,
                    row[2].take()?.parse().ok()?,
                    row[3].take()?,
                ))
            })
            .collect();
        keys.sort();
        Ok(keyed(
            keys.into_iter()
                .map(|(schema, table, _, column)| (self.table_of(schema, table), column))
                .collect(),
        ))
    }

    fn fetch_links(&self) -> anyhow::Result<HashMap<Table, Vec<ForeignKey>>> {
        if self.database.is_none() {
            return Ok(HashMap::new());
        }
        let mut links: Vec<Link> = self
            .ask("show imported keys in database")?
            .picked(&[
                "fk_schema_name",
                "fk_table_name",
                "fk_name",
                "key_sequence",
                "fk_column_name",
                "pk_schema_name",
                "pk_table_name",
                "pk_column_name",
            ])
            .into_iter()
            .filter_map(|row| {
                let [
                    schema,
                    table,
                    name,
                    sequence,
                    column,
                    target_schema,
                    target,
                    reference,
                ]: [String; 8] = row
                    .into_iter()
                    .collect::<Option<Vec<_>>>()?
                    .try_into()
                    .ok()?;
                Some(Link {
                    key: (schema, table, name),
                    sequence: sequence.parse().ok()?,
                    column,
                    target: (target_schema, target),
                    reference,
                })
            })
            .collect();
        links.sort_by(|left, right| (&left.key, left.sequence).cmp(&(&right.key, right.sequence)));
        let mut keys: HashMap<Table, Vec<ForeignKey>> = HashMap::new();
        let mut previous: Option<(String, String, String)> = None;
        for link in links {
            let (schema, table, _) = link.key.clone();
            let found = keys.entry(self.table_of(Some(schema), table)).or_default();
            match found.last_mut() {
                Some(key) if previous.as_ref() == Some(&link.key) => {
                    key.columns.push(link.column);
                    key.references.push(link.reference);
                }
                _ => found.push(ForeignKey {
                    columns: vec![link.column],
                    table: self.table_of(Some(link.target.0), link.target.1),
                    references: vec![link.reference],
                }),
            }
            previous = Some(link.key);
        }
        Ok(keys)
    }

    fn paging_columns(&mut self, table: &Table) -> Vec<String> {
        if let Some(keys) = &self.keys {
            return keys.get(table).cloned().unwrap_or_default();
        }
        if let Some(columns) = self.orders.get(table) {
            return columns.clone();
        }
        let columns = self
            .table_keys(&format!(
                "show primary keys in table {}",
                table.quoted(quote_double)
            ))
            .ok()
            .and_then(|keys| keys.into_iter().next())
            .map(|(_, columns)| columns)
            .unwrap_or_default();
        self.orders.insert(table.clone(), columns.clone());
        columns
    }

    fn column_types(&mut self, table: &Table) -> anyhow::Result<HashMap<String, String>> {
        if let Some(types) = self.types.get(table) {
            return Ok(types.clone());
        }
        let types: HashMap<String, String> = self
            .columns(table)?
            .into_iter()
            .map(|column| (column.name, column.type_name))
            .collect();
        self.types.insert(table.clone(), types.clone());
        Ok(types)
    }

    fn guarded(&self, statement: &str, most: usize) -> anyhow::Result<u64> {
        let changed = self
            .ask(&format!(
                "begin\n  begin transaction;\n  {statement};\n  let changed integer := sqlrowcount;\n  if (changed > {most}) then\n    rollback;\n  else\n    commit;\n  end if;\n  return changed;\nend;"
            ))?
            .first()
            .and_then(|changed| changed.parse::<u64>().ok())
            .ok_or_else(|| anyhow!("Snowflake did not say how many rows the change reached"))?;
        if changed > most as u64 {
            bail!(
                "{changed} rows matched where {most} {} meant to, so tql rolled the change back. Snowflake does not enforce keys, so rows can repeat; change them from the SQL editor.",
                if most == 1 { "was" } else { "were" }
            );
        }
        Ok(changed)
    }

    fn row_matched(&mut self, table: &Table, row: &Values) -> anyhow::Result<String> {
        let types = self.column_types(table)?;
        let matched: Vec<String> = row
            .iter()
            .filter_map(|(column, value)| {
                let kind = types.get(column).map(String::as_str).unwrap_or_default();
                match value {
                    None => Some(format!("{} is null", quote_double(column))),
                    Some(_) if UNMATCHED.contains(&kind) => None,
                    Some(value) => Some(format!("{} = {}", quote_double(column), literal(value))),
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

    fn written_value(&mut self, table: &Table, column: &str, value: Option<&str>) -> String {
        let kind = self
            .column_types(table)
            .ok()
            .and_then(|types| types.get(column).cloned())
            .unwrap_or_default();
        match value {
            None => "null".to_string(),
            Some(value) if matches!(kind.as_str(), "variant" | "object" | "array" | "map") => {
                format!("parse_json({})", literal(value))
            }
            Some(value) => literal(value),
        }
    }

    fn assigned(&mut self, table: &Table, changes: &Values) -> String {
        changes
            .iter()
            .map(|(column, value)| {
                format!(
                    "{} = {}",
                    quote_double(column),
                    self.written_value(table, column, value.as_deref())
                )
            })
            .collect::<Vec<_>>()
            .join(", ")
    }

    fn table_keys(&self, statement: &str) -> anyhow::Result<Vec<(String, Vec<String>)>> {
        let mut keys: Vec<(String, usize, String)> = self
            .ask(statement)?
            .picked(&["constraint_name", "key_sequence", "column_name"])
            .into_iter()
            .filter_map(|mut row| {
                Some((
                    row[0].take().unwrap_or_default(),
                    row[1].take()?.parse().ok()?,
                    row[2].take()?,
                ))
            })
            .collect();
        keys.sort();
        let mut grouped: Vec<(String, Vec<String>)> = Vec::new();
        for (constraint, _, column) in keys {
            match grouped.last_mut() {
                Some((name, columns)) if *name == constraint => columns.push(column),
                _ => grouped.push((constraint, vec![column])),
            }
        }
        Ok(grouped)
    }
}

fn base_url(account: &str) -> String {
    if account.starts_with("http://") || account.starts_with("https://") {
        return account.trim_end_matches('/').to_string();
    }
    let host = account.trim_end_matches('/');
    match host.contains(".snowflakecomputing.") {
        true => format!("https://{host}"),
        false => format!("https://{}{DOMAIN}", host.replace('_', "-")),
    }
}

fn account_name(account: &str) -> String {
    let host = account
        .split_once("://")
        .map_or(account, |(_, host)| host)
        .trim_end_matches('/');
    let host = host
        .split_once(".snowflakecomputing.")
        .map_or(host, |(name, _)| name);
    host.split('.').next().unwrap_or(host).to_uppercase()
}

fn answer_of(sent: Sent) -> Answer {
    let fields = fields(&sent.data["rowtype"]);
    let mut rows = rows(&sent.data["rowset"], &fields);
    for chunk in &sent.chunks {
        rows.extend(self::rows(chunk, &fields));
    }
    Answer {
        headers: fields.iter().map(|field| field.name.clone()).collect(),
        kinds: vec![fields.iter().map(kind_of).collect(); rows.len()],
        written: sent.written(),
        rows,
    }
}

fn said(reply: &Reply, hint: &str) -> String {
    let message = serde_json::from_str::<Value>(&reply.body)
        .ok()
        .and_then(|body| body["message"].as_str().map(String::from))
        .map(|message| message.split_whitespace().collect::<Vec<_>>().join(" "))
        .unwrap_or_else(|| format!("Snowflake answered {}: {}", reply.status, reply.body.trim()));
    let message = message
        .strip_suffix(" null")
        .map(String::from)
        .unwrap_or(message);
    if reply.status == 401 {
        return format!("{message} {hint}");
    }
    match message.contains("No active warehouse") || message.contains("specify the warehouse") {
        true => format!("{message} Set a warehouse on the connection."),
        false => message,
    }
}

fn fields(row_type: &Value) -> Vec<Field> {
    row_type
        .as_array()
        .map(|columns| {
            columns
                .iter()
                .map(|column| Field {
                    name: column["name"].as_str().unwrap_or_default().to_string(),
                    kind: column["type"].as_str().unwrap_or_default().to_lowercase(),
                    scale: column["scale"]
                        .as_u64()
                        .and_then(|scale| usize::try_from(scale).ok())
                        .unwrap_or(9),
                })
                .collect()
        })
        .unwrap_or_default()
}

fn partition_rows(part: Value, fields: &[Field]) -> anyhow::Result<Vec<Vec<Option<String>>>> {
    let data = match part {
        Value::Array(_) => part,
        mut body => body["data"].take(),
    };
    if !data.is_array() {
        bail!("Snowflake sent part of an answer with no rows in it");
    }
    Ok(rows(&data, fields))
}

fn rows(data: &Value, fields: &[Field]) -> Vec<Vec<Option<String>>> {
    data.as_array()
        .map(|rows| {
            rows.iter()
                .map(|row| {
                    fields
                        .iter()
                        .enumerate()
                        .map(|(at, field)| row.get(at)?.as_str().map(|value| shown(value, field)))
                        .collect()
                })
                .collect()
        })
        .unwrap_or_default()
}

fn written(body: &Value) -> Option<usize> {
    let stats = body["stats"].as_object()?;
    Some(
        ["numRowsInserted", "numRowsUpdated", "numRowsDeleted"]
            .iter()
            .filter_map(|name| stats.get(*name)?.as_u64())
            .map(|count| usize::try_from(count).unwrap_or(usize::MAX))
            .fold(0, usize::saturating_add),
    )
}

fn kind_of(field: &Field) -> Kind {
    match field.kind.as_str() {
        "fixed" if field.scale == 0 => Kind::Integer,
        "real" => Kind::Float,
        "boolean" => Kind::Boolean,
        _ => Kind::Text,
    }
}

fn shown(value: &str, field: &Field) -> String {
    let converted = match field.kind.as_str() {
        "date" => date(value),
        "time" => time(value, field.scale),
        "timestamp_ntz" => stamp(value, None, field.scale),
        "timestamp_ltz" => stamp(value, Some(0), field.scale),
        "timestamp_tz" => value.split_once(' ').and_then(|(seconds, offset)| {
            stamp(
                seconds,
                Some(offset.parse::<i32>().ok()? - 1440),
                field.scale,
            )
        }),
        "variant" | "object" | "array" | "map" => serde_json::from_str::<Value>(value)
            .ok()
            .map(|parsed| parsed.to_string()),
        _ => None,
    };
    converted.unwrap_or_else(|| value.to_string())
}

fn nanoseconds(value: &str) -> Option<i128> {
    let (negative, digits) = match value.strip_prefix('-') {
        Some(digits) => (true, digits),
        None => (false, value),
    };
    let (whole, fraction) = digits.split_once('.').unwrap_or((digits, ""));
    if fraction.len() > 9 || !fraction.chars().all(|digit| digit.is_ascii_digit()) {
        return None;
    }
    let total = whole.parse::<i128>().ok()? * 1_000_000_000
        + format!("{fraction:0<9}").parse::<i128>().ok()?;
    Some(match negative {
        true => -total,
        false => total,
    })
}

fn fraction(nanoseconds: i128, scale: usize) -> String {
    match scale.min(9) {
        0 => String::new(),
        scale => format!(".{:09}", nanoseconds.rem_euclid(1_000_000_000))[..=scale].to_string(),
    }
}

fn date(value: &str) -> Option<String> {
    let days = value.parse::<i32>().ok()?;
    Some(
        jiff::civil::date(1970, 1, 1)
            .checked_add(jiff::Span::new().try_days(days).ok()?)
            .ok()?
            .strftime("%Y-%m-%d")
            .to_string(),
    )
}

fn time(value: &str, scale: usize) -> Option<String> {
    let nanoseconds = nanoseconds(value)?;
    let seconds = nanoseconds.div_euclid(1_000_000_000);
    Some(format!(
        "{:02}:{:02}:{:02}{}",
        seconds / 3600,
        seconds / 60 % 60,
        seconds % 60,
        fraction(nanoseconds, scale)
    ))
}

fn stamp(value: &str, offset_minutes: Option<i32>, scale: usize) -> Option<String> {
    let nanoseconds = nanoseconds(value)?;
    let offset = jiff::tz::Offset::from_seconds(offset_minutes.unwrap_or(0) * 60).ok()?;
    let zoned = jiff::Timestamp::from_nanosecond(nanoseconds)
        .ok()?
        .to_zoned(jiff::tz::TimeZone::fixed(offset));
    let wall = format!(
        "{}{}",
        zoned.strftime("%Y-%m-%d %H:%M:%S"),
        fraction(nanoseconds, scale)
    );
    Some(match offset_minutes {
        None => wall,
        Some(_) => format!("{wall} {}", zoned.strftime("%:z")),
    })
}

fn literal(value: &str) -> String {
    format!("'{}'", value.replace('\\', "\\\\").replace('\'', "\\'"))
}

fn typed(data_type: &str) -> (String, String) {
    let Ok(described) = serde_json::from_str::<Value>(data_type) else {
        return (data_type.to_string(), data_type.to_string());
    };
    let number = |key: &str| described[key].as_u64();
    let kind = match described["type"].as_str().unwrap_or(data_type) {
        "FIXED" => "NUMBER",
        "REAL" => "FLOAT",
        other => other,
    };
    let full = match (kind, number("precision"), number("scale"), number("length")) {
        ("NUMBER", Some(precision), Some(scale), _) => format!("NUMBER({precision},{scale})"),
        ("TEXT" | "BINARY", _, _, Some(length)) => format!("{kind}({length})"),
        (time, _, Some(scale), _) if time.starts_with("TIME") => format!("{kind}({scale})"),
        _ => kind.to_string(),
    };
    (kind.to_string(), full)
}

impl Database for Snowflake {
    fn tables(&mut self) -> anyhow::Result<Vec<Table>> {
        if self.database.is_none() {
            return Ok(Vec::new());
        }
        let this = &*self;
        let (objects, keys, links) = std::thread::scope(|scope| {
            let keys = scope.spawn(|| this.fetch_keys());
            let links = scope.spawn(|| this.fetch_links());
            (
                this.ask("show terse objects in database"),
                keys.join().ok().and_then(Result::ok),
                links.join().ok().and_then(Result::ok),
            )
        });
        self.early_keys = keys;
        self.early_links = links;
        let mut found: Vec<(String, String)> = objects?
            .picked(&["schema_name", "name"])
            .into_iter()
            .filter_map(|mut row| Some((row[0].take()?, row[1].take()?)))
            .filter(|(schema, _)| schema != "INFORMATION_SCHEMA")
            .collect();
        found.sort_by_key(|(schema, name)| {
            (
                Some(schema) != self.schema.as_ref(),
                schema.clone(),
                name.clone(),
            )
        });
        Ok(found
            .into_iter()
            .map(|(schema, name)| self.table_of(Some(schema), name))
            .collect())
    }

    fn default_schema(&mut self) -> Option<String> {
        self.schema.clone()
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
            .and_then(|filters| filters.condition(&quote_double))
            .map(|condition| condition.render(literal));
        let mut order: Vec<String> = sort
            .map(|sort| {
                format!(
                    "{} {}",
                    quote_double(&sort.column),
                    sort.direction.keyword()
                )
            })
            .into_iter()
            .collect();
        order.extend(
            self.paging_columns(table)
                .iter()
                .filter(|column| sort.is_none_or(|sort| sort.column != **column))
                .map(|column| quote_double(column)),
        );
        let sql = format!(
            "select * from {}{}{} limit {limit} offset {offset}",
            table.quoted(quote_double),
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
        Some(
            self.paging_columns(table)
                .iter()
                .map(|column| quote_double(column))
                .collect::<Vec<_>>()
                .join(", "),
        )
        .filter(|order| !order.is_empty())
    }

    fn count_rows(&mut self, table: &Table, filters: Option<&Filters>) -> anyhow::Result<usize> {
        let condition = filters
            .and_then(|filters| filters.condition(&quote_double))
            .map(|condition| format!(" where {}", condition.render(literal)))
            .unwrap_or_default();
        first_count(self.page(&format!(
            "select count(*) from {}{condition}",
            table.quoted(quote_double)
        ))?)
        .ok_or_else(|| anyhow!("the count came back empty"))
    }

    fn estimated_rows(&mut self, table: &Table) -> Option<Total> {
        self.listed(table).ok()??.0?.parse().ok().map(Total::About)
    }

    fn primary_keys(&mut self) -> anyhow::Result<HashMap<Table, String>> {
        Ok(single_column_keys(self.key_columns()?))
    }

    fn key_columns(&mut self) -> anyhow::Result<HashMap<Table, Vec<String>>> {
        let keys = match self.early_keys.take() {
            Some(keys) => keys,
            None => self.fetch_keys()?,
        };
        self.keys = Some(keys.clone());
        self.orders.clear();
        Ok(keys)
    }

    fn select(&mut self, statement: &str, values: &[String]) -> anyhow::Result<Page> {
        self.page(&fill(statement, values, literal))
    }

    fn columns(&mut self, table: &Table) -> anyhow::Result<Vec<Column>> {
        Ok(self
            .ask(&format!(
                "show columns in table {}",
                table.quoted(quote_double)
            ))?
            .picked(&[
                "column_name",
                "data_type",
                "null?",
                "default",
                "kind",
                "expression",
                "comment",
                "autoincrement",
            ])
            .into_iter()
            .map(|mut row| {
                let present = |value: Option<String>| value.filter(|value| !value.is_empty());
                let (kind, full_type) = typed(row[1].as_deref().unwrap_or_default());
                Column {
                    name: row[0].take().unwrap_or_default(),
                    type_name: kind.to_lowercase(),
                    nullable: row[2].as_deref() != Some("NOT_NULL"),
                    default: present(row[3].take()),
                    auto_increment: present(row[7].take()).is_some(),
                    full_type,
                    collation: None,
                    comment: present(row[6].take()),
                    generation: (row[4].as_deref() == Some("VIRTUAL")).then(|| super::Generation {
                        kind: Some("virtual".to_string()),
                        expression: present(row[5].take()),
                    }),
                }
            })
            .collect())
    }

    fn indexes(&mut self, table: &Table) -> anyhow::Result<Vec<Index>> {
        let quoted = table.quoted(quote_double);
        let mut indexes: Vec<Index> = self
            .table_keys(&format!("show primary keys in table {quoted}"))?
            .into_iter()
            .map(|(name, columns)| Index {
                name,
                columns,
                unique: true,
                primary: true,
            })
            .collect();
        indexes.extend(
            self.table_keys(&format!("show unique keys in table {quoted}"))?
                .into_iter()
                .map(|(name, columns)| Index {
                    name,
                    columns,
                    unique: true,
                    primary: false,
                }),
        );
        indexes.extend(self.listed(table)?.and_then(|row| row.1).map(|key| Index {
            name: "clustering key".to_string(),
            columns: vec![key],
            unique: false,
            primary: false,
        }));
        Ok(indexes)
    }

    fn foreign_keys(&mut self) -> anyhow::Result<HashMap<Table, Vec<ForeignKey>>> {
        match self.early_links.take() {
            Some(links) => Ok(links),
            None => self.fetch_links(),
        }
    }

    fn databases(&mut self) -> anyhow::Result<Vec<String>> {
        Ok(self
            .ask("show terse databases")?
            .picked(&["name"])
            .into_iter()
            .filter_map(|mut row| row[0].take())
            .collect())
    }

    fn use_database(&mut self, name: &str) -> anyhow::Result<()> {
        let was = (self.database.replace(name.to_string()), self.schema.clone());
        if let Err(error) = self.settle_schema() {
            (self.database, self.schema) = was;
            return Err(error);
        }
        Ok(())
    }

    fn run(&mut self, statement: &str) -> anyhow::Result<Outcome> {
        let answer = self.ask(statement)?;
        if let Some(written) = answer.written {
            return Ok(outcome_or_affected(Vec::new(), Vec::new(), written));
        }
        Ok(Outcome {
            kinds: answer.kinds,
            ..outcome_or_affected(answer.headers, answer.rows, 0)
        })
    }

    fn run_reading(&mut self, statement: &str) -> anyhow::Result<Outcome> {
        if let Some(word) = writing_word(statement) {
            bail!(
                "that statement has {word} in it, so tql did not run it; only statements that read run here"
            );
        }
        if !is_read_only(statement) {
            bail!("only statements that read run here");
        }
        self.run(statement)
    }

    fn update(
        &mut self,
        table: &Table,
        column: &str,
        key: &str,
        key_value: &str,
        value: Option<&str>,
    ) -> anyhow::Result<u64> {
        let assigned = self.assigned(table, &vec![(column.to_string(), value.map(String::from))]);
        self.guarded(
            &format!(
                "update {} set {assigned} where {} = {}",
                table.quoted(quote_double),
                quote_double(key),
                literal(key_value)
            ),
            1,
        )
    }

    fn insert(&mut self, table: &Table, values: &Values) -> anyhow::Result<()> {
        if values.is_empty() {
            bail!("give the row at least one value; Snowflake has no row of only defaults");
        }
        let columns: Vec<String> = values
            .iter()
            .map(|(column, _)| quote_double(column))
            .collect();
        let written: Vec<String> = values
            .iter()
            .map(|(column, value)| self.written_value(table, column, value.as_deref()))
            .collect();
        self.ask(&format!(
            "insert into {} ({}) select {}",
            table.quoted(quote_double),
            columns.join(", "),
            written.join(", ")
        ))?;
        Ok(())
    }

    fn delete(&mut self, table: &Table, key: &str, key_values: &[String]) -> anyhow::Result<u64> {
        let listed: Vec<String> = key_values.iter().map(|value| literal(value)).collect();
        self.guarded(
            &format!(
                "delete from {} where {} in ({})",
                table.quoted(quote_double),
                quote_double(key),
                listed.join(", ")
            ),
            key_values.len(),
        )
    }

    fn matches_rows(&self) -> bool {
        true
    }

    fn update_row(&mut self, table: &Table, row: &Values, changes: &Values) -> anyhow::Result<u64> {
        let matched = self.row_matched(table, row)?;
        let assigned = self.assigned(table, changes);
        self.guarded(
            &format!(
                "update {} set {assigned} where {matched}",
                table.quoted(quote_double)
            ),
            1,
        )
    }

    fn delete_row(&mut self, table: &Table, row: &Values) -> anyhow::Result<u64> {
        let matched = self.row_matched(table, row)?;
        self.guarded(
            &format!("delete from {} where {matched}", table.quoted(quote_double)),
            1,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn field(kind: &str, scale: usize) -> Field {
        Field {
            name: "value".to_string(),
            kind: kind.to_string(),
            scale,
        }
    }

    fn reply(status: u16, body: &str) -> Reply {
        Reply {
            status,
            headers: HashMap::new(),
            body: body.to_string(),
            broken: None,
        }
    }

    #[test]
    fn finds_the_account_url_from_whatever_identifier_was_typed() {
        assert_eq!(
            base_url("myorg-myaccount"),
            "https://myorg-myaccount.snowflakecomputing.com"
        );
        assert_eq!(
            base_url("my_org-my_account"),
            "https://my-org-my-account.snowflakecomputing.com"
        );
        assert_eq!(
            base_url("xy12345.us-east-1.snowflakecomputing.com/"),
            "https://xy12345.us-east-1.snowflakecomputing.com"
        );
        assert_eq!(base_url("http://127.0.0.1:9000/"), "http://127.0.0.1:9000");
    }

    #[test]
    fn writes_dates_and_times_the_way_snowflake_shows_them() {
        assert_eq!(shown("18262", &field("date", 0)), "2020-01-01");
        assert_eq!(shown("-1", &field("date", 0)), "1969-12-31");
        assert_eq!(shown("82919.123456789", &field("time", 3)), "23:01:59.123");
        assert_eq!(
            shown("1616173619.500000000", &field("timestamp_ntz", 9)),
            "2021-03-19 17:06:59.500000000"
        );
        assert_eq!(
            shown("-1.500000000", &field("timestamp_ntz", 1)),
            "1969-12-31 23:59:58.5"
        );
        assert_eq!(
            shown("1616173619.000000000", &field("timestamp_ltz", 0)),
            "2021-03-19 17:06:59 +00:00"
        );
        assert_eq!(
            shown("1616173619.000000000 1500", &field("timestamp_tz", 0)),
            "2021-03-19 18:06:59 +01:00"
        );
        assert_eq!(
            shown("1616173619.000000000 1140", &field("timestamp_tz", 0)),
            "2021-03-19 12:06:59 -05:00"
        );
    }

    #[test]
    fn keeps_a_value_it_cannot_read_as_snowflake_sent_it() {
        assert_eq!(shown("not a date", &field("date", 0)), "not a date");
        assert_eq!(shown("12.50", &field("fixed", 2)), "12.50");
    }

    #[test]
    fn writes_semi_structured_values_on_one_line() {
        assert_eq!(
            shown(
                "{\n  \"b\": [\n    1,\n    2\n  ],\n  \"a\": null\n}",
                &field("variant", 0)
            ),
            "{\"b\":[1,2],\"a\":null}"
        );
    }

    #[test]
    fn only_whole_numbers_are_integers() {
        assert_eq!(kind_of(&field("fixed", 0)), Kind::Integer);
        assert_eq!(kind_of(&field("fixed", 2)), Kind::Text);
        assert_eq!(kind_of(&field("real", 0)), Kind::Float);
        assert_eq!(kind_of(&field("boolean", 0)), Kind::Boolean);
    }

    #[test]
    fn reads_the_rows_of_an_answer_with_their_nulls() {
        let body: Value = serde_json::from_str(
            r#"{"resultSetMetaData":{"rowType":[{"name":"ID","type":"fixed","scale":0},{"name":"BORN","type":"date","scale":0}]},
                "data":[["1","18262"],["2",null]]}"#,
        )
        .unwrap();
        let fields = fields(&body["resultSetMetaData"]["rowType"]);
        assert_eq!(
            rows(&body["data"], &fields),
            [
                [Some("1".to_string()), Some("2020-01-01".to_string())],
                [Some("2".to_string()), None],
            ]
        );
    }

    #[test]
    fn reads_a_later_partition_whether_or_not_it_is_wrapped() {
        let fields = [field("text", 0)];
        assert_eq!(
            partition_rows(serde_json::json!([["a"]]), &fields).unwrap(),
            [[Some("a".to_string())]]
        );
        assert_eq!(
            partition_rows(serde_json::json!({"data": [["b"]]}), &fields).unwrap(),
            [[Some("b".to_string())]]
        );
        assert!(partition_rows(serde_json::json!({}), &fields).is_err());
    }

    #[test]
    fn counts_every_row_a_statement_wrote() {
        let body = serde_json::json!({"stats": {"numRowsInserted": 2, "numRowsUpdated": 3, "numRowsDeleted": 0}});
        assert_eq!(written(&body), Some(5));
        assert_eq!(written(&serde_json::json!({})), None);
    }

    #[test]
    fn keeps_snowflakes_words_from_a_failure_on_one_line() {
        assert_eq!(
            said(
                &reply(
                    422,
                    r#"{"code":"002003","message":"SQL compilation error:\nObject 'NOPE' does not exist or not authorized."}"#
                ),
                TOKEN_HINT
            ),
            "SQL compilation error: Object 'NOPE' does not exist or not authorized."
        );
        assert_eq!(
            said(
                &reply(
                    401,
                    r#"{"code":"390303","message":"Invalid OAuth access token. null"}"#
                ),
                TOKEN_HINT
            ),
            "Invalid OAuth access token. Check the access token saved on this connection."
        );
        assert_eq!(
            said(&reply(502, "Bad Gateway"), TOKEN_HINT),
            "Snowflake answered 502: Bad Gateway"
        );
        assert!(
            said(
                &reply(
                    422,
                    r#"{"message":"No active warehouse selected in the current session."}"#
                ),
                TOKEN_HINT
            )
            .ends_with("Set a warehouse on the connection.")
        );
    }

    #[test]
    fn writes_a_column_type_the_way_information_schema_would() {
        let full = |data_type: &str| typed(data_type).1;
        assert_eq!(
            typed(r#"{"type":"FIXED","precision":10,"scale":2,"nullable":true}"#),
            ("NUMBER".to_string(), "NUMBER(10,2)".to_string())
        );
        assert_eq!(full(r#"{"type":"REAL","nullable":true}"#), "FLOAT");
        assert_eq!(
            full(r#"{"type":"TEXT","length":255,"byteLength":1020,"nullable":true,"fixed":false}"#),
            "TEXT(255)"
        );
        assert_eq!(
            full(
                r#"{"type":"BINARY","length":8388608,"byteLength":8388608,"nullable":true,"fixed":true}"#
            ),
            "BINARY(8388608)"
        );
        assert_eq!(
            full(r#"{"type":"TIMESTAMP_TZ","precision":0,"scale":3,"nullable":true}"#),
            "TIMESTAMP_TZ(3)"
        );
        assert_eq!(
            full(r#"{"type":"TIME","precision":0,"scale":0,"nullable":true}"#),
            "TIME(0)"
        );
        assert_eq!(full(r#"{"type":"VARIANT","nullable":true}"#), "VARIANT");
        assert_eq!(full("GEOGRAPHY"), "GEOGRAPHY");
    }

    #[test]
    fn escapes_backslashes_and_quotes_in_literals() {
        assert_eq!(literal("O'Brien\\path"), "'O\\'Brien\\\\path'");
    }
}
