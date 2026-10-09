use serde_json::Value;

use super::answers;
use crate::console::prelude::*;

pub struct Query;

const NO_STATEMENT: &str =
    "Give the statement as an argument, or pipe it in: echo \"select 1\" | tql query <connection>";

impl Command for Query {
    fn signature(&self) -> &str {
        "query {connection : The connection, by name} {sql? : The statement; - or nothing reads it from stdin} {--database= : Another database on the same server} {--limit=200 : The most rows to return} {--json : JSON, which is what you get when the output is piped} {--table : A table, which is what you get in a terminal}"
    }

    fn description(&self) -> &str {
        "Run a read-only query, for a script or an agent"
    }

    fn execute(&self, cmd: &mut Cmd) -> Result {
        let Some(statement) = statement(cmd)? else {
            return answers::refuse(cmd, NO_STATEMENT);
        };
        let limit = cmd
            .option("limit")
            .and_then(|limit| limit.trim().parse::<i64>().ok())
            .unwrap_or(0)
            .max(0) as usize;
        let ran = answers::agent().query(
            cmd.argument("connection"),
            &statement,
            "cli",
            cmd.option("database"),
            limit,
        );
        answers::answer(cmd, ran, draw)
    }
}

fn statement(cmd: &mut Cmd) -> Result<Option<String>> {
    if let Some(given) = cmd.argument("sql").filter(|given| *given != "-") {
        return Ok(Some(given.to_string()));
    }
    if cmd.has_terminal() {
        return Ok(None);
    }
    let piped = cmd.read_all()?.trim().to_string();
    Ok(Some(piped).filter(|piped| !piped.is_empty()))
}

fn draw(cmd: &mut Cmd, answer: &Value) {
    let results = answer["results"].as_array().cloned().unwrap_or_default();
    if let Some(Value::Object(first)) = results.first() {
        let headers: Vec<String> = first.keys().cloned().collect();
        let rows: Vec<Vec<String>> = results
            .iter()
            .filter_map(Value::as_object)
            .map(|row| {
                row.values()
                    .map(|value| match value {
                        Value::Null => "NULL".to_string(),
                        value => answers::text(value),
                    })
                    .collect()
            })
            .collect();
        cmd.table(&headers, rows);
    }

    let total = answer["rows"].as_u64().unwrap_or_default();
    let noun = match total {
        1 => "row",
        _ => "rows",
    };
    let showing = match answer["truncated"].as_bool() {
        Some(true) => format!(" · showing the first {}", results.len()),
        _ => String::new(),
    };
    cmd.line(format!(
        "{total} {noun} · {}ms{showing}",
        answer["duration_ms"]
    ));
}
