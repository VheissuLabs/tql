use super::Table;
use std::collections::HashMap;
use std::sync::{Arc, LazyLock};
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

use super::schema::keys_by_schema;
use super::{
    Column, Database, Filters, ForeignKey, Generation, Index, Outcome, Page, Scalar, Scalars, Sort,
    Values, all_text, delete_statement, insert_statement, keyed, outcome_or_affected, refused,
    shown_delete, shown_insert, shown_update, single_column_keys, split_list, update_statement,
};
use crate::store::Connection;
use crate::tunnel::{Route, Tunnel};

const MASTER: &str = "master";
const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);

type Wire = Client<Compat<TcpStream>>;

pub struct SqlServer {
    runtime: Runtime,
    client: Wire,
    connection: Connection,
    password: Option<String>,
    default: Option<String>,
    route: Route,
    _tunnel: Option<Arc<Tunnel>>,
}

const SCHEMA_OR_DEFAULT: &str = "case when schema_name(t.schema_id) = schema_name() then null else schema_name(t.schema_id) end";

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

mod client;
mod driver;
mod sql;
mod values;

use client::*;
use sql::question;
pub use sql::{numbered_parameters, page, quote_bracket};
use values::*;

#[cfg(test)]
mod tests;
