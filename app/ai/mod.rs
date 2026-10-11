use anyhow::{Context, anyhow};
use serde_json::{Value, json};

use crate::database::Column;
use crate::settings::Ai;

pub mod providers;
mod wire;

pub use providers::{LOCAL, MODELS, Provider, Wire, chosen, endpoint, key_variable, model};
pub use wire::{Replied, Request, replied, request, send};

pub type Environment = dyn Fn(&str) -> Option<String> + Send + Sync;

pub const SUMMARY_LIMIT: usize = 40;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Answer {
    pub query: String,
    pub explanation: String,
    pub notes: String,
}

pub struct Question {
    pub provider: Provider,
    pub model: String,
    pub timeout: u64,
    pub instructions: String,
    pub question: String,
}

pub fn prepare(
    settings: &Ai,
    environment: &Environment,
    driver: &str,
    schema: &str,
    question: &str,
) -> anyhow::Result<Question> {
    let provider = chosen(settings, environment).ok_or_else(|| anyhow!(missing(settings)))?;
    Ok(Question {
        model: model(settings, &provider.name),
        provider,
        timeout: settings.timeout,
        instructions: instructions(driver, schema),
        question: question.trim().to_string(),
    })
}

pub fn ask(question: &Question) -> anyhow::Result<Answer> {
    let replied = send(question).context("the model could not be reached")?;
    decode(&replied.text, &replied.reasoning)
        .ok_or_else(|| anyhow!("the model did not answer with a query"))
}

pub fn missing(settings: &Ai) -> String {
    let configured = settings.provider.trim();
    if !configured.is_empty() && configured != "auto" {
        return format!(
            "no key for {configured}, set {}, or point [ai] url at a local model",
            key_variable(configured)
        );
    }
    "no model to ask, set an api key such as ANTHROPIC_API_KEY or OPENAI_API_KEY, or point [ai] url at a local endpoint like LM Studio".to_string()
}

pub fn summary(
    tables: &[String],
    focus: Option<&str>,
    mut columns: impl FnMut(&str) -> Vec<Column>,
) -> String {
    let mut ordered: Vec<&String> = Vec::with_capacity(tables.len());
    if let Some(focus) = focus.and_then(|focus| tables.iter().find(|table| *table == focus)) {
        ordered.push(focus);
    }
    ordered.extend(tables.iter().filter(|table| Some(table.as_str()) != focus));

    let mut lines: Vec<String> = ordered
        .iter()
        .take(SUMMARY_LIMIT)
        .map(|table| {
            let described: Vec<String> = columns(table)
                .iter()
                .map(|column| {
                    let kind = [&column.type_name, &column.full_type]
                        .into_iter()
                        .find(|kind| !kind.is_empty())
                        .map_or("?", String::as_str);
                    format!("{} {kind}", column.name)
                })
                .collect();
            format!("{table} ({})", described.join(", "))
        })
        .collect();
    if ordered.len() > SUMMARY_LIMIT {
        lines.push(format!(
            "… and {} more tables",
            ordered.len() - SUMMARY_LIMIT
        ));
    }
    lines.join("\n")
}

pub fn instructions(driver: &str, schema: &str) -> String {
    format!(
        "You write a single SQL query for a {driver} database, for someone
who knows Eloquent but not much SQL.

Rules:
- Return exactly one statement. Never several, never a transaction.
- Only read data: select, with, show, explain. Never insert, update,
  delete, drop, alter, truncate, grant or create. If the question asks
  for a write, explain in the notes what it would take and return a
  select that shows the rows it would affect.
- Use only the tables and columns in the schema below. If the question
  cannot be answered with them, say so in the notes and return an empty
  query rather than inventing a column.
- Add a limit unless the question is an aggregate.
- Quote identifiers the way {driver} does.
- Write it over several lines so it can be read.

The explanation is for someone learning SQL. Say what each clause is
doing in plain words, in two or three sentences. No preamble.

The schema:

{schema}"
    )
}

pub fn schema() -> Value {
    json!({
        "additionalProperties": false,
        "properties": {
            "query": {"type": "string"},
            "explanation": {"type": "string"},
            "notes": {"type": "string"},
        },
        "type": "object",
        "required": ["query", "explanation"],
    })
}

pub fn decode(text: &str, reasoning: &str) -> Option<Answer> {
    let found = structured(text)
        .or_else(|| embedded(reasoning))
        .or_else(|| embedded(text))?;
    let field = |name: &str| match found.get(name) {
        Some(Value::String(text)) => text.trim().to_string(),
        Some(Value::Null) | None => String::new(),
        Some(other) => other.to_string().trim().to_string(),
    };
    Some(Answer {
        query: field("query"),
        explanation: field("explanation"),
        notes: field("notes"),
    })
}

fn with_query(value: Value) -> Option<Value> {
    value
        .as_object()
        .is_some_and(|object| object.get("query").is_some_and(|query| !query.is_null()))
        .then_some(value)
}

fn structured(text: &str) -> Option<Value> {
    let trimmed = text.trim();
    serde_json::from_str(trimmed)
        .ok()
        .and_then(with_query)
        .or_else(|| {
            fenced(trimmed)
                .and_then(|fence| serde_json::from_str(fence).ok())
                .and_then(with_query)
        })
}

fn fenced(text: &str) -> Option<&str> {
    let start = text.find("```json")? + "```json".len();
    let rest = &text[start..];
    let end = rest.find("```")?;
    Some(rest[..end].trim())
}

fn embedded(text: &str) -> Option<Value> {
    text.match_indices('{').find_map(|(start, _)| {
        serde_json::Deserializer::from_str(&text[start..])
            .into_iter::<Value>()
            .next()?
            .ok()
            .and_then(with_query)
    })
}

pub fn annotate(answer: &Answer) -> String {
    let mut lines: Vec<String> = Vec::new();
    for text in [&answer.explanation, &answer.notes] {
        if text.is_empty() {
            continue;
        }
        for line in wordwrap(text, 76, false).split('\n') {
            lines.push(format!("-- {line}"));
        }
        lines.push("--".to_string());
    }
    let joined = lines.join("\n");
    let separator = if lines.is_empty() { "" } else { "\n" };
    format!("{joined}{separator}{}", trimmed(&answer.query))
}

pub fn trimmed(query: &str) -> String {
    let query = query.trim();
    let Some(body) = query.strip_suffix("*/") else {
        return query.to_string();
    };
    match body.rfind("/*") {
        Some(start) => query[..start].trim_end().to_string(),
        None => query.to_string(),
    }
}

pub fn pieces(line: &[char], width: usize, cut: bool) -> Vec<(usize, usize)> {
    let width = width.max(1);
    let mut pieces = Vec::new();
    let (mut last_start, mut last_space) = (0, 0);
    for (current, character) in line.iter().enumerate() {
        if *character == ' ' {
            if current - last_start >= width {
                pieces.push((last_start, current));
                last_start = current + 1;
            }
            last_space = current;
        } else if current - last_start >= width && cut && last_start >= last_space {
            pieces.push((last_start, current));
            last_start = current;
            last_space = current;
        } else if current - last_start >= width && last_start < last_space {
            pieces.push((last_start, last_space));
            last_start = last_space + 1;
            last_space = last_start;
        }
    }
    pieces.push((last_start, line.len()));
    pieces
}

pub fn wordwrap(text: &str, width: usize, cut: bool) -> String {
    text.split('\n')
        .map(|line| {
            let characters: Vec<char> = line.chars().collect();
            pieces(&characters, width, cut)
                .into_iter()
                .map(|(start, end)| characters[start..end].iter().collect::<String>())
                .collect::<Vec<_>>()
                .join("\n")
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wraps_the_way_php_wordwrap_does() {
        assert_eq!(
            wordwrap("The quick brown fox", 10, true),
            "The quick\nbrown fox"
        );
        assert_eq!(
            wordwrap("A very long woooooooooooord.", 8, true),
            "A very\nlong\nwooooooo\nooooord."
        );
        assert_eq!(
            wordwrap("A very long woooooooooooord.", 8, false),
            "A very\nlong\nwoooooooooooord."
        );
        assert_eq!(
            wordwrap("short\nlines stay", 76, false),
            "short\nlines stay"
        );
        assert_eq!(wordwrap("", 10, true), "");
        assert_eq!(
            wordwrap("aaaa bbbb cccc dd", 4, true),
            "aaaa\nbbbb\ncccc\ndd"
        );
        assert_eq!(wordwrap("a  b   c", 1, true), "a\n \nb\n \nc");
    }

    #[test]
    fn strips_only_the_comment_the_model_signed_off_with() {
        assert_eq!(
            trimmed("select 1;\n/* Explain: it selects one. */"),
            "select 1;"
        );
        assert_eq!(trimmed("select 1;"), "select 1;");
        assert_eq!(
            trimmed("select /* the count */ count(*) from t;"),
            "select /* the count */ count(*) from t;"
        );
        assert_eq!(
            trimmed("select /* the count */ count(*) from t;\n/* done */"),
            "select /* the count */ count(*) from t;"
        );
    }

    #[test]
    fn finds_the_answer_in_a_fence_in_the_thinking_or_in_prose() {
        let answer = Answer {
            query: "select 1".to_string(),
            explanation: "one".to_string(),
            notes: String::new(),
        };
        assert_eq!(
            decode(r#"{"query": " select 1 ", "explanation": "one"}"#, ""),
            Some(answer.clone())
        );
        assert_eq!(
            decode(
                "```json\n{\"query\": \"select 1\", \"explanation\": \"one\"}\n```",
                ""
            ),
            Some(answer.clone())
        );
        assert_eq!(
            decode(
                "",
                r#"Let me think. {"query": "select 1", "explanation": "one"} and that is it."#
            ),
            Some(answer.clone())
        );
        assert_eq!(
            decode(
                "<think></think>",
                r#"{"query": "select 1", "explanation": "one"}"#
            ),
            Some(answer)
        );
        assert_eq!(decode("no json here", ""), None);
        assert_eq!(decode(r#"{"explanation": "no query"}"#, ""), None);
    }

    #[test]
    fn finds_the_answer_after_braces_in_the_reasoning() {
        let reasoning = r#"The user wants {orders}. {"query": "select 1", "explanation": "one"}"#;
        assert_eq!(decode("", reasoning).unwrap().query, "select 1");
    }
}
