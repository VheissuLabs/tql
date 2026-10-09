use std::time::Duration;

use serde_json::{Value, json};

use super::{Question, Wire, schema};

const SUMMARY_LENGTH: usize = 120;

const SCHEMA_INSTRUCTION: &str = "You MUST respond EXCLUSIVELY with a JSON object that strictly adheres to the following schema. Do NOT explain or add other content. Validate your response against this schema:\n";

const CREDIT_PATTERNS: [&str; 6] = [
    "credit balance",
    "insufficient",
    "quota exceeded",
    "exceeded your current quota",
    "billing",
    "usage limit",
];

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Replied {
    pub text: String,
    pub reasoning: String,
}

pub struct Request {
    pub url: String,
    pub headers: Vec<(&'static str, String)>,
    pub body: Value,
}

fn joined(base: &str, path: &str) -> String {
    format!("{}/{path}", base.trim_end_matches('/'))
}

fn reasons(model: &str) -> bool {
    (model.starts_with("gpt-5") && !model.starts_with("gpt-5-chat"))
        || ["gpt-6", "o4-mini", "o3", "o1"]
            .iter()
            .any(|prefix| model.starts_with(prefix))
}

fn bearer(key: &str) -> Vec<(&'static str, String)> {
    if key.is_empty() {
        return Vec::new();
    }
    vec![("Authorization", format!("Bearer {key}"))]
}

fn with_schema(instructions: &str) -> String {
    format!(
        "{instructions}\n\n{SCHEMA_INSTRUCTION}{}",
        crate::agent::pretty(&schema())
    )
}

pub fn request(question: &Question) -> Request {
    let provider = &question.provider;
    let model = question.model.as_str();
    let system = question.instructions.as_str();
    let asked = question.question.as_str();
    match provider.wire {
        Wire::Anthropic => Request {
            url: joined(&provider.url, "messages"),
            headers: vec![
                ("x-api-key", provider.key.clone()),
                ("anthropic-version", "2023-06-01".to_string()),
                ("anthropic-beta", "web-fetch-2025-09-10".to_string()),
            ],
            body: json!({
                "model": model,
                "messages": [{"role": "user", "content": [{"type": "text", "text": asked}]}],
                "max_tokens": 64000,
                "system": system,
                "output_config": {"format": {"type": "json_schema", "schema": schema()}},
            }),
        },
        Wire::Responses => {
            let mut body = json!({
                "model": model,
                "input": [
                    {"role": "system", "content": system},
                    {"role": "user", "content": [{"type": "input_text", "text": asked}]},
                ],
                "text": {"format": {
                    "type": "json_schema",
                    "name": "schema_definition",
                    "schema": schema(),
                    "strict": false,
                }},
            });
            if provider.name != "xai" && reasons(model) {
                body["include"] = json!(["reasoning.encrypted_content"]);
            }
            let headers = if provider.name == "azure" {
                vec![("api-key", provider.key.clone())]
            } else {
                bearer(&provider.key)
            };
            Request {
                url: joined(&provider.url, "responses"),
                headers,
                body,
            }
        }
        Wire::Gemini => Request {
            url: joined(&provider.url, "interactions"),
            headers: vec![("x-goog-api-key", provider.key.clone())],
            body: json!({
                "model": model,
                "input": [{"type": "user_input", "content": [{"type": "text", "text": asked}]}],
                "store": false,
                "system_instruction": system,
                "response_format": {
                    "type": "text",
                    "mime_type": "application/json",
                    "schema": schema(),
                },
            }),
        },
        Wire::Chat => Request {
            url: joined(&provider.url, "chat/completions"),
            headers: bearer(&provider.key),
            body: json!({
                "model": model,
                "messages": [
                    {"role": "system", "content": system},
                    {"role": "user", "content": asked},
                ],
                "response_format": {
                    "type": "json_schema",
                    "json_schema": {
                        "name": "schema_definition",
                        "schema": schema(),
                        "strict": false,
                    },
                },
            }),
        },
        Wire::JsonObject => Request {
            url: joined(&provider.url, "chat/completions"),
            headers: bearer(&provider.key),
            body: json!({
                "model": model,
                "messages": [
                    {"role": "system", "content": with_schema(system)},
                    {"role": "user", "content": asked},
                ],
                "response_format": {"type": "json_object"},
            }),
        },
        Wire::Ollama => Request {
            url: joined(&provider.url, "api/chat"),
            headers: bearer(&provider.key),
            body: json!({
                "model": model,
                "messages": [
                    {"role": "system", "content": with_schema(system)},
                    {"role": "user", "content": asked},
                ],
                "stream": false,
                "format": schema(),
            }),
        },
    }
}

pub fn send(question: &Question) -> Result<Replied, String> {
    let request = request(question);
    let provider = &question.provider;
    let agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(question.timeout.max(1))))
        .http_status_as_error(false)
        .user_agent(concat!("tql/", env!("CARGO_PKG_VERSION")))
        .build()
        .new_agent();
    let mut pending = agent
        .post(&request.url)
        .header("Content-Type", "application/json");
    for (name, value) in &request.headers {
        pending = pending.header(*name, value);
    }
    let unreachable = || format!("Could not connect to AI provider [{}].", provider.name);
    let response = pending
        .send(request.body.to_string())
        .map_err(|_| unreachable())?;
    let status = response.status().as_u16();
    let body = response
        .into_body()
        .read_to_string()
        .map_err(|_| unreachable())?;
    replied(provider, status, &body)
}

pub fn replied(provider: &super::Provider, status: u16, body: &str) -> Result<Replied, String> {
    let name = &provider.name;
    let data: Value = serde_json::from_str(body).unwrap_or(Value::Null);
    if status >= 400 {
        let overloaded: &[u16] = match provider.name.as_str() {
            "anthropic" => &[529, 502, 503, 504, 520, 522, 524],
            "groq" => &[498, 502, 503],
            _ => &[502, 503, 504, 520, 522, 524],
        };
        let message = data["error"]["message"]
            .as_str()
            .unwrap_or_default()
            .to_lowercase();
        return Err(match status {
            429 => format!("Application rate limited by AI provider [{name}]."),
            402 => format!("AI provider [{name}] has insufficient credits or quota."),
            status if overloaded.contains(&status) => {
                format!("AI provider [{name}] is overloaded.")
            }
            _ if provider.name == "anthropic"
                && CREDIT_PATTERNS
                    .iter()
                    .any(|pattern| message.contains(pattern)) =>
            {
                format!("AI provider [{name}] has insufficient credits or quota.")
            }
            _ => failed(status, body),
        });
    }
    if let Some(error) = refused(provider, &data) {
        return Err(error);
    }
    Ok(match provider.wire {
        Wire::Anthropic => Replied {
            text: blocks(&data["content"], "text", "text"),
            reasoning: blocks(&data["content"], "thinking", "thinking"),
        },
        Wire::Responses => {
            let output = data["output"].as_array().cloned().unwrap_or_default();
            let message = output
                .iter()
                .rev()
                .find(|item| item["type"] == "message")
                .or_else(|| (provider.name != "xai").then(|| output.last()).flatten());
            Replied {
                text: message
                    .and_then(|item| item["content"][0]["text"].as_str())
                    .unwrap_or_default()
                    .to_string(),
                reasoning: output
                    .iter()
                    .filter(|item| item["type"] == "reasoning")
                    .flat_map(|item| [texts(&item["summary"]), texts(&item["content"])])
                    .filter(|text| !text.is_empty())
                    .collect::<Vec<_>>()
                    .join("\n"),
            }
        }
        Wire::Gemini => {
            let steps = data["steps"].as_array().cloned().unwrap_or_default();
            let of = |kind: &str| {
                steps
                    .iter()
                    .filter(|step| step["type"] == kind)
                    .map(|step| {
                        if step["summary"].is_array() {
                            texts(&step["summary"])
                        } else {
                            texts(&step["content"])
                        }
                    })
                    .collect::<Vec<_>>()
            };
            Replied {
                text: of("model_output").concat(),
                reasoning: of("thought").join("\n"),
            }
        }
        Wire::Chat | Wire::JsonObject => {
            let message = &data["choices"][0]["message"];
            let content = &message["content"];
            let text = match content {
                Value::Array(chunks) => chunks
                    .iter()
                    .filter(|chunk| chunk["type"] != "thinking")
                    .filter_map(|chunk| chunk["text"].as_str())
                    .collect(),
                other => other.as_str().unwrap_or_default().to_string(),
            };
            let thinking = match content {
                Value::Array(chunks) => chunks
                    .iter()
                    .filter(|chunk| chunk["type"] == "thinking")
                    .map(|chunk| texts(&chunk["thinking"]))
                    .collect::<Vec<_>>()
                    .join("\n"),
                _ => String::new(),
            };
            let reasoning = [&message["reasoning_content"], &message["reasoning"]]
                .into_iter()
                .filter_map(Value::as_str)
                .find(|reasoning| !reasoning.is_empty())
                .map_or(thinking, str::to_string);
            Replied { text, reasoning }
        }
        Wire::Ollama => Replied {
            text: data["message"]["content"]
                .as_str()
                .unwrap_or_default()
                .to_string(),
            reasoning: data["message"]["thinking"]
                .as_str()
                .unwrap_or_default()
                .to_string(),
        },
    })
}

fn failed(status: u16, body: &str) -> String {
    let message = format!("HTTP request returned status code {status}");
    if body.is_empty() {
        return message;
    }
    let mut summary: String = body.chars().take(SUMMARY_LENGTH).collect();
    if body.chars().count() > SUMMARY_LENGTH {
        summary.push_str(" (truncated...)");
    }
    format!("{message}:\n{summary}\n")
}

fn refused(provider: &super::Provider, data: &Value) -> Option<String> {
    let label = provider.label();
    let typed = |error: &Value, fallback: &str| {
        format!(
            "{label} Error: [{}] {}",
            error["type"]
                .as_str()
                .or_else(|| error["code"].as_str())
                .unwrap_or("unknown"),
            error["message"]
                .as_str()
                .map_or_else(|| fallback.to_string(), str::to_string)
        )
    };
    let unknown = format!("Unknown {label} error.");
    if !data.is_object() {
        return Some(match provider.wire {
            Wire::Ollama => format!("{label} Error: {unknown}"),
            _ => format!("{label} Error: [unknown] {unknown}"),
        });
    }
    match provider.wire {
        Wire::Anthropic => (data["type"] == "error").then(|| typed(&data["error"], &unknown)),
        Wire::Ollama => data
            .get("error")
            .filter(|error| !error.is_null())
            .map(|error| {
                format!(
                    "{label} Error: {}",
                    error
                        .as_str()
                        .map_or_else(|| error.to_string(), str::to_string)
                )
            }),
        Wire::Gemini => {
            if let Some(error) = data.get("error").filter(|error| !error.is_null()) {
                return Some(format!(
                    "{label} Error: [{}] {}",
                    error["code"]
                        .as_str()
                        .map_or_else(|| error["code"].to_string(), str::to_string),
                    error["message"].as_str().unwrap_or("Unknown Gemini error.")
                ));
            }
            ["failed", "cancelled"]
                .contains(&data["status"].as_str().unwrap_or_default())
                .then(|| {
                    format!(
                        "{label} Error: [{}] {}",
                        data["errors"][0]["code"]
                            .as_str()
                            .or_else(|| data["status"].as_str())
                            .unwrap_or_default(),
                        data["errors"][0]["message"]
                            .as_str()
                            .unwrap_or("The Gemini interaction did not complete.")
                    )
                })
        }
        _ => data
            .get("error")
            .filter(|error| !error.is_null())
            .map(|error| typed(error, &unknown))
            .or_else(|| (data["status"] == "failed").then(|| typed(&data["error"], &unknown))),
    }
}

fn blocks(content: &Value, kind: &str, field: &str) -> String {
    content
        .as_array()
        .map(|blocks| {
            blocks
                .iter()
                .filter(|block| block["type"] == kind)
                .filter_map(|block| block[field].as_str())
                .collect::<Vec<_>>()
                .join(if kind == "text" { "" } else { "\n" })
        })
        .unwrap_or_default()
}

fn texts(items: &Value) -> String {
    match items {
        Value::Array(items) => items
            .iter()
            .filter_map(|item| item["text"].as_str())
            .collect(),
        Value::String(text) => text.clone(),
        _ => String::new(),
    }
}
