mod tools;

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use serde_json::{Map, Value, json};

use crate::agent::Agent;

pub use tools::Tool;

const PROTOCOLS: [&str; 2] = ["2025-11-25", "2025-06-18"];
const VERSIONED: &str = "2026-07-28";
const VERSION_KEY: &str = "io.modelcontextprotocol/protocolVersion";
const CAPABILITIES_KEY: &str = "io.modelcontextprotocol/clientCapabilities";
const PER_PAGE: usize = 15;
const MOST_PER_PAGE: usize = 50;

pub const INSTRUCTIONS: &str = "tql exposes the same database connections the user browses in the tql terminal interface. Start by listing connections, then list or describe tables before querying. Queries are read-only; writes must happen in the tql interface, and the write SQL tool puts a statement in the SQL editor of the tql the user has open, for them to read and run. The read screen tool shows what the user is looking at there, when they allow it.";

pub struct Server {
    agent: Agent,
    version: String,
}

struct Failure {
    code: i64,
    message: String,
    data: Option<Value>,
}

impl Failure {
    fn new(code: i64, message: impl Into<String>) -> Self {
        Failure {
            code,
            message: message.into(),
            data: None,
        }
    }
}

impl Server {
    pub fn new(agent: Agent, version: &str) -> Self {
        Server {
            agent,
            version: version.trim_start_matches('v').to_string(),
        }
    }

    pub fn handle(&self, line: &str) -> Option<String> {
        let reply = match serde_json::from_str::<Value>(line) {
            Ok(message) => self.reply(&message)?,
            Err(_) => json!({
                "jsonrpc": "2.0",
                "error": {
                    "code": -32700,
                    "message": "Parse error: Invalid JSON was received by the server.",
                },
            }),
        };
        Some(reply.to_string())
    }

    fn reply(&self, message: &Value) -> Option<Value> {
        let id = message.get("id").filter(|id| !id.is_null())?.clone();
        let method = message["method"].as_str().unwrap_or_default();
        let params = message.get("params").cloned().unwrap_or(json!({}));
        let answered = versioned(&params).and_then(|()| self.answer(method, &params));
        Some(match answered {
            Ok(result) => json!({ "jsonrpc": "2.0", "id": id, "result": result }),
            Err(failure) => {
                let mut error = json!({ "code": failure.code, "message": failure.message });
                if let Some(data) = failure.data {
                    error["data"] = data;
                }
                json!({ "jsonrpc": "2.0", "id": id, "error": error })
            }
        })
    }

    fn answer(&self, method: &str, params: &Value) -> Result<Value, Failure> {
        match method {
            "initialize" => {
                let asked = params["protocolVersion"].as_str().unwrap_or_default();
                let agreed = PROTOCOLS
                    .into_iter()
                    .find(|known| *known == asked)
                    .unwrap_or(PROTOCOLS[0]);
                Ok(self.complete(
                    json!({
                        "protocolVersion": agreed,
                        "capabilities": capabilities(),
                        "serverInfo": self.info(),
                        "instructions": INSTRUCTIONS,
                    }),
                    false,
                ))
            }
            "ping" => Ok(self.complete(json!({}), false)),
            "server/discover" => Ok(self.complete(
                json!({
                    "supportedVersions": [VERSIONED],
                    "capabilities": capabilities(),
                    "instructions": INSTRUCTIONS,
                }),
                true,
            )),
            "tools/list" => {
                let listed: Vec<Value> = Tool::ALL.iter().map(Tool::listed).collect();
                let (page, next) = paged(listed, params)?;
                let mut result = json!({ "tools": page });
                if let Some(next) = next {
                    result["nextCursor"] = Value::String(next);
                }
                Ok(self.complete(result, true))
            }
            "tools/call" => self.call(params),
            "resources/list" => Ok(self.complete(json!({ "resources": [] }), true)),
            "resources/templates/list" => {
                Ok(self.complete(json!({ "resourceTemplates": [] }), true))
            }
            "prompts/list" => Ok(self.complete(json!({ "prompts": [] }), true)),
            "resources/read" => Err(Failure::new(
                -32002,
                format!(
                    "Resource [{}] not found.",
                    params["uri"].as_str().unwrap_or_default()
                ),
            )),
            "prompts/get" => Err(Failure::new(
                -32602,
                format!(
                    "Prompt [{}] not found.",
                    params["name"].as_str().unwrap_or_default()
                ),
            )),
            "completion/complete" => Ok(self.complete(
                json!({ "completion": { "values": [], "total": 0, "hasMore": false } }),
                false,
            )),
            other => Err(Failure::new(
                -32601,
                format!("The method [{other}] was not found."),
            )),
        }
    }

    fn call(&self, params: &Value) -> Result<Value, Failure> {
        let Some(name) = params["name"].as_str() else {
            return Err(Failure::new(-32602, "Missing [name] parameter."));
        };
        let Some(tool) = Tool::named(name) else {
            return Err(Failure::new(-32602, format!("Tool [{name}] not found.")));
        };
        let (text, failed) = match tool.call(&self.agent, &params["arguments"]) {
            Ok(text) => (text, false),
            Err(error) => (format!("{error:#}"), true),
        };
        Ok(self.complete(
            json!({ "content": [{ "type": "text", "text": text }], "isError": failed }),
            false,
        ))
    }

    fn info(&self) -> Value {
        json!({ "name": "tql", "version": self.version })
    }

    fn complete(&self, fields: Value, listing: bool) -> Value {
        let mut result = Map::new();
        result.insert("resultType".into(), json!("complete"));
        if listing {
            result.insert("ttlMs".into(), json!(0));
            result.insert("cacheScope".into(), json!("private"));
        }
        if let Value::Object(fields) = fields {
            result.extend(fields);
        }
        result.insert(
            "_meta".into(),
            json!({ "io.modelcontextprotocol/serverInfo": self.info() }),
        );
        Value::Object(result)
    }
}

fn capabilities() -> Value {
    json!({
        "tools": { "listChanged": false },
        "resources": { "listChanged": false },
        "prompts": { "listChanged": false },
    })
}

fn versioned(params: &Value) -> Result<(), Failure> {
    let meta = &params["_meta"];
    let version = meta.get(VERSION_KEY);
    let capabilities = meta.get(CAPABILITIES_KEY);
    if version.is_none() && capabilities.is_none() {
        return Ok(());
    }
    for (key, given) in [(VERSION_KEY, version), (CAPABILITIES_KEY, capabilities)] {
        if given.is_none() {
            return Err(Failure::new(
                -32602,
                format!(
                    "Invalid params: The request [_meta] is missing the required [{key}] member."
                ),
            ));
        }
    }
    let requested = version.cloned().unwrap_or_default();
    if requested.as_str() == Some(VERSIONED) {
        return Ok(());
    }
    Err(Failure {
        data: Some(json!({ "supported": [VERSIONED], "requested": requested })),
        ..Failure::new(-32022, "Unsupported protocol version")
    })
}

fn paged(items: Vec<Value>, params: &Value) -> Result<(Vec<Value>, Option<String>), Failure> {
    let offset = match params["cursor"].as_str() {
        None => 0,
        Some(cursor) => STANDARD
            .decode(cursor)
            .ok()
            .and_then(|decoded| serde_json::from_slice::<Value>(&decoded).ok())
            .and_then(|decoded| decoded["offset"].as_u64())
            .ok_or_else(|| Failure::new(-32602, "Invalid params: The cursor is not valid."))?
            as usize,
    };
    let per_page = params["per_page"]
        .as_u64()
        .map(|asked| (asked as usize).clamp(1, MOST_PER_PAGE))
        .unwrap_or(PER_PAGE);
    let total = items.len();
    let page: Vec<Value> = items.into_iter().skip(offset).take(per_page).collect();
    let next = (offset + per_page < total)
        .then(|| STANDARD.encode(json!({ "offset": offset + per_page }).to_string()));
    Ok((page, next))
}
