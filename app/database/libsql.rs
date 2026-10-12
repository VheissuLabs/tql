use std::collections::HashMap;
use std::time::Duration;

use anyhow::{anyhow, bail};
use serde_json::{Value, json};

use super::http;
use super::lexed::writing_word;
use super::schema::keys_by_table;
use super::sqlite::{
    COLUMNS, CREATED, FOREIGN_KEYS, INDEXES, KEY_COLUMNS, TABLES, columns_of, indexes_of,
};
use super::{
    Column, Database, Filters, ForeignKey, Index, Kind, Outcome, Page, Sort, Table, Values,
    delete_statement, insert_statement, keyed, outcome_or_affected, primary_key, quote_double,
    refused, select_page, shown_delete, shown_insert, shown_update, single_column_keys,
    update_statement,
};
use crate::store::Connection;

const TIMEOUT: Duration = Duration::from_secs(60);

pub struct Libsql {
    pipeline: String,
    token: Option<String>,
}

struct Answer {
    headers: Vec<String>,
    rows: Vec<Vec<Option<String>>>,
    kinds: Vec<Vec<Kind>>,
    affected: usize,
}

impl Libsql {
    pub fn open(connection: &Connection, token: Option<String>) -> anyhow::Result<Self> {
        let url = connection.host.as_deref().unwrap_or_default().trim();
        if url.is_empty() {
            bail!("a libSQL connection needs a libsql:// or https:// URL");
        }
        let base = match url.strip_prefix("libsql://") {
            Some(rest) => format!("https://{rest}"),
            None => url.to_string(),
        };
        let database = Libsql {
            pipeline: format!("{}/v2/pipeline", base.trim_end_matches('/')),
            token: token.filter(|token| !token.trim().is_empty()),
        };
        database.answers(&[("select 1", &[])])?;
        Ok(database)
    }

    fn answers(&self, statements: &[(&str, &[Option<&str>])]) -> anyhow::Result<Vec<Answer>> {
        let mut requests: Vec<Value> = statements
            .iter()
            .map(|(sql, args)| {
                json!({
                    "type": "execute",
                    "stmt": { "sql": sql, "args": args.iter().map(|value| argument(*value)).collect::<Vec<_>>() },
                })
            })
            .collect();
        requests.push(json!({ "type": "close" }));
        let mut headers = vec![("Content-Type", "application/json".to_string())];
        if let Some(token) = &self.token {
            headers.push(("Authorization", format!("Bearer {token}")));
        }
        let reply = http::post(
            &self.pipeline,
            &headers,
            json!({ "requests": requests }).to_string(),
            TIMEOUT,
        )?;
        let results = answered(reply.status, &reply.body)?;
        results
            .into_iter()
            .take(statements.len())
            .map(result)
            .collect()
    }

    fn one(&self, sql: &str, args: &[Option<&str>]) -> anyhow::Result<Answer> {
        self.answers(&[(sql, args)])?
            .pop()
            .ok_or_else(|| anyhow!("the server gave no answer"))
    }

    fn page(&self, sql: &str, values: &[String]) -> anyhow::Result<Page> {
        let args: Vec<Option<&str>> = values.iter().map(|value| Some(value.as_str())).collect();
        let answer = self.one(sql, &args)?;
        Ok(Page {
            statement: sql.to_string(),
            headers: answer.headers,
            rows: answer.rows,
            kinds: answer.kinds,
        })
    }

    fn write(&self, sql: &str, args: &[Option<&str>], shown: String) -> anyhow::Result<u64> {
        self.one(sql, args)
            .map(|answer| answer.affected as u64)
            .map_err(|error| refused(error, &shown))
    }
}

fn argument(value: Option<&str>) -> Value {
    match value {
        Some(text) => json!({ "type": "text", "value": text }),
        None => json!({ "type": "null" }),
    }
}

pub(crate) fn answered(status: u16, body: &str) -> anyhow::Result<Vec<Value>> {
    if status == 401 || status == 403 {
        bail!("the libSQL server refused the auth token (HTTP {status})");
    }
    let parsed: Value = serde_json::from_str(body)
        .map_err(|_| anyhow!("the libSQL server answered HTTP {status}: {}", body.trim()))?;
    if status >= 400 {
        let said = parsed["message"]
            .as_str()
            .or_else(|| parsed["error"].as_str())
            .unwrap_or(body.trim());
        bail!("the libSQL server answered HTTP {status}: {said}");
    }
    Ok(parsed["results"].as_array().cloned().unwrap_or_default())
}

fn result(value: Value) -> anyhow::Result<Answer> {
    if value["type"] == "error" {
        bail!(
            "{}",
            value["error"]["message"]
                .as_str()
                .unwrap_or("the statement failed")
        );
    }
    let result = &value["response"]["result"];
    let headers: Vec<String> = result["cols"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|column| column["name"].as_str().unwrap_or_default().to_string())
        .collect();
    let mut rows = Vec::new();
    let mut kinds = Vec::new();
    for row in result["rows"].as_array().into_iter().flatten() {
        let cells = row.as_array().cloned().unwrap_or_default();
        kinds.push(cells.iter().map(kind_of).collect());
        rows.push(cells.iter().map(text).collect());
    }
    Ok(Answer {
        headers,
        rows,
        kinds,
        affected: result["affected_row_count"].as_u64().unwrap_or_default() as usize,
    })
}

fn kind_of(cell: &Value) -> Kind {
    match cell["type"].as_str() {
        Some("integer") => Kind::Integer,
        Some("float") => Kind::Float,
        _ => Kind::Text,
    }
}

fn text(cell: &Value) -> Option<String> {
    Some(match cell["type"].as_str() {
        Some("null") | None => return None,
        Some("integer") => cell["value"].as_str().unwrap_or_default().to_string(),
        Some("float") => super::exporter::real_text(cell["value"].as_f64().unwrap_or_default()),
        Some("blob") => {
            let encoded = cell["base64"]
                .as_str()
                .unwrap_or_default()
                .trim_end_matches('=');
            format!("<{} bytes>", encoded.len() * 3 / 4)
        }
        Some(_) => cell["value"].as_str().unwrap_or_default().to_string(),
    })
}

fn numbered(position: usize) -> String {
    format!("?{position}")
}

impl Database for Libsql {
    fn tables(&mut self) -> anyhow::Result<Vec<Table>> {
        Ok(self
            .one(TABLES, &[])?
            .rows
            .into_iter()
            .filter_map(|mut row| row[0].take())
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
        let condition = filters.and_then(|filters| filters.condition(&quote_double));
        let page = |shown: Option<String>| {
            select_page(
                &table.quoted(quote_double),
                quote_double,
                limit,
                offset,
                sort,
                shown.as_deref(),
            )
        };
        let sql = page(condition.as_ref().map(|condition| condition.placeheld()));
        let bindings = condition
            .as_ref()
            .map(|condition| condition.bindings())
            .unwrap_or_default();
        let mut found = self.page(&sql, &bindings)?;
        found.statement = page(condition.as_ref().map(|condition| condition.readable()));
        Ok(found)
    }

    fn run(&mut self, statement: &str) -> anyhow::Result<Outcome> {
        let answer = self.one(statement, &[])?;
        Ok(Outcome {
            kinds: answer.kinds,
            ..outcome_or_affected(answer.headers, answer.rows, answer.affected)
        })
    }

    fn run_reading(&mut self, statement: &str) -> anyhow::Result<Outcome> {
        if let Some(word) = writing_word(statement) {
            bail!(
                "that statement has {word} in it, so tql did not run it; only statements that read run here"
            );
        }
        let mut answers = self
            .answers(&[("begin", &[]), (statement, &[]), ("rollback", &[])])?
            .into_iter();
        let answer = answers
            .nth(1)
            .ok_or_else(|| anyhow!("the server gave no answer"))?;
        Ok(Outcome {
            kinds: answer.kinds,
            ..outcome_or_affected(answer.headers, answer.rows, answer.affected)
        })
    }

    fn primary_keys(&mut self) -> anyhow::Result<HashMap<Table, String>> {
        Ok(single_column_keys(self.key_columns()?))
    }

    fn key_columns(&mut self) -> anyhow::Result<HashMap<Table, Vec<String>>> {
        Ok(keyed(
            self.one(KEY_COLUMNS, &[])?
                .rows
                .into_iter()
                .filter_map(|mut row| Some((Table::bare(row[0].take()?), row[1].take()?)))
                .collect(),
        ))
    }

    fn select(&mut self, statement: &str, values: &[String]) -> anyhow::Result<Page> {
        self.page(statement, values)
    }

    fn columns(&mut self, table: &Table) -> anyhow::Result<Vec<Column>> {
        let name = Some(table.name.as_str());
        let mut answers = self
            .answers(&[(COLUMNS, &[name]), (CREATED, &[name])])?
            .into_iter();
        let rows = answers.next().map(|answer| answer.rows).unwrap_or_default();
        let created = answers
            .next()
            .and_then(|answer| answer.rows.into_iter().next())
            .and_then(|mut row| row[0].take())
            .unwrap_or_default();
        Ok(columns_of(rows, &created))
    }

    fn indexes(&mut self, table: &Table) -> anyhow::Result<Vec<Index>> {
        Ok(indexes_of(
            self.one(INDEXES, &[Some(table.name.as_str())])?.rows,
        ))
    }

    fn foreign_keys(&mut self) -> anyhow::Result<HashMap<Table, Vec<ForeignKey>>> {
        let mut keys = keys_by_table(self.one(FOREIGN_KEYS, &[])?.rows);
        for key in keys.values_mut().flatten() {
            if key.references.is_empty() {
                key.references = primary_key(&self.indexes(&key.table)?)
                    .into_iter()
                    .collect();
            }
        }
        Ok(keys)
    }

    fn highest(&mut self, table: &Table, column: &str) -> anyhow::Result<Option<String>> {
        Ok(self
            .one(
                &format!(
                    "select max({}) from {}",
                    quote_double(column),
                    table.quoted(quote_double)
                ),
                &[],
            )?
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
        let sql = update_statement(quote_double, numbered, table, column, key);
        self.write(
            &sql,
            &[value, Some(key_value)],
            shown_update(quote_double, table, column, key, value, key_value),
        )
    }

    fn insert(&mut self, table: &Table, values: &Values) -> anyhow::Result<()> {
        let sql = insert_statement(quote_double, numbered, table, values, "default values");
        let args: Vec<Option<&str>> = values.iter().map(|(_, value)| value.as_deref()).collect();
        self.write(
            &sql,
            &args,
            shown_insert(quote_double, table, values, "default values"),
        )
        .map(|_| ())
    }

    fn delete(&mut self, table: &Table, key: &str, key_values: &[String]) -> anyhow::Result<u64> {
        let sql = delete_statement(quote_double, numbered, table, key, key_values.len());
        let args: Vec<Option<&str>> = key_values
            .iter()
            .map(|value| Some(value.as_str()))
            .collect();
        self.write(
            &sql,
            &args,
            shown_delete(quote_double, table, key, key_values),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn says_the_token_was_refused_rather_than_failing_to_read_json() {
        let refused = answered(401, "").err().unwrap();
        assert_eq!(
            refused.to_string(),
            "the libSQL server refused the auth token (HTTP 401)"
        );
    }

    #[test]
    fn says_what_the_server_said_for_any_other_failure() {
        let failed = answered(500, r#"{"message":"disk full"}"#).err().unwrap();
        assert_eq!(
            failed.to_string(),
            "the libSQL server answered HTTP 500: disk full"
        );
        let plain = answered(502, "Bad Gateway").err().unwrap();
        assert_eq!(
            plain.to_string(),
            "the libSQL server answered HTTP 502: Bad Gateway"
        );
    }
}
