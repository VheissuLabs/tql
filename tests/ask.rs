use app::database::Table;
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, Instant};

use app::ai::{self, Answer};
use app::database;
use app::settings::{self, Settings};
use app::store::Connection;
use app::terminal::Fake;
use app::tui::{Browser, Mode};
use serde_json::{Value, json};

fn fixture(name: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!("tql-ask-{}-{name}.sqlite", std::process::id()));
    let _ = std::fs::remove_file(&path);
    rusqlite::Connection::open(&path)
        .unwrap()
        .execute_batch(
            "create table customers (id integer primary key, name text, email text);
             create table invoices (id integer primary key, customer_id integer, total numeric);
             insert into customers (name, email) values ('Karl', 'karl@example.com');",
        )
        .unwrap();
    path
}

fn environment(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> + Send + Sync + 'static {
    let pairs: HashMap<String, String> = pairs
        .iter()
        .map(|(name, value)| (name.to_string(), value.to_string()))
        .collect();
    move |name: &str| pairs.get(name).cloned()
}

fn configured(pairs: &[(&str, Value)]) -> Settings {
    let mut settings = Settings::default();
    for (key, value) in pairs {
        let value: toml::Value = serde_json::from_value(value.clone()).unwrap();
        settings.set(key, &value);
    }
    settings
}

fn asked_with(name: &str, settings: Settings, pairs: &[(&str, &str)]) -> Fake<Browser> {
    settings::install(settings);
    let path = fixture(name);
    let connection = Connection::sqlite("shop", path.to_str().unwrap());
    let database = database::connect(&connection, None).unwrap();
    Fake::sized(
        Browser::new(connection, database)
            .unwrap()
            .with_ai_environment(environment(pairs)),
        120,
        30,
    )
}

fn asked(name: &str) -> Fake<Browser> {
    asked_with(
        name,
        configured(&[("ai.provider", json!("anthropic"))]),
        &[],
    )
}

fn columns_of(path: &std::path::Path) -> impl FnMut(&Table) -> Vec<app::database::Column> {
    let connection = Connection::sqlite("shop", path.to_str().unwrap());
    let mut database = database::connect(&connection, None).unwrap();
    move |table| database.columns(table).unwrap()
}

struct Recorded {
    path: String,
    headers: HashMap<String, String>,
    body: Value,
}

struct Server {
    url: String,
    requests: Receiver<Recorded>,
}

impl Server {
    fn start(status: u16, reply: Value, delay: Duration) -> Server {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let (sender, requests) = mpsc::channel();
        let reply = reply.to_string();
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { return };
                let mut reader = BufReader::new(stream.try_clone().unwrap());
                let mut first = String::new();
                reader.read_line(&mut first).unwrap();
                let path = first
                    .split_whitespace()
                    .nth(1)
                    .unwrap_or_default()
                    .to_string();
                let mut headers = HashMap::new();
                loop {
                    let mut line = String::new();
                    reader.read_line(&mut line).unwrap();
                    let line = line.trim_end();
                    if line.is_empty() {
                        break;
                    }
                    let (name, value) = line.split_once(':').unwrap();
                    headers.insert(name.trim().to_lowercase(), value.trim().to_string());
                }
                let length: usize = headers
                    .get("content-length")
                    .map_or(0, |length| length.parse().unwrap());
                let mut body = vec![0; length];
                reader.read_exact(&mut body).unwrap();
                let _ = sender.send(Recorded {
                    path,
                    headers,
                    body: serde_json::from_slice(&body).unwrap_or(Value::Null),
                });
                std::thread::sleep(delay);
                let _ = write!(
                    stream,
                    "HTTP/1.1 {status} Reply\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{reply}",
                    reply.len()
                );
            }
        });
        Server { url, requests }
    }

    fn replying(reply: Value) -> Server {
        Server::start(200, reply, Duration::ZERO)
    }

    fn request(&self) -> Recorded {
        self.requests.recv_timeout(Duration::from_secs(5)).unwrap()
    }
}

fn chat(content: &str) -> Value {
    json!({
        "id": "chatcmpl-1",
        "object": "chat.completion",
        "choices": [{"index": 0, "finish_reason": "stop", "message": {"role": "assistant", "content": content}}],
    })
}

fn answer_text() -> String {
    json!({
        "query": "select count(*)\nfrom customers",
        "explanation": "Counts every row in the customers table.",
    })
    .to_string()
}

fn local(server: &Server) -> Settings {
    configured(&[
        ("ai.provider", json!("auto")),
        ("ai.url", json!(format!("{}/v1", server.url))),
        ("ai.model", json!("qwen2.5-coder-7b")),
    ])
}

fn wait(fake: &mut Fake<Browser>) {
    let started = Instant::now();
    loop {
        fake.settle();
        let waiting = fake
            .app
            .question
            .as_ref()
            .is_some_and(|question| question.is_waiting());
        if !waiting || started.elapsed() > Duration::from_secs(10) {
            return;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn prepared(settings: &Settings, pairs: &[(&str, &str)]) -> Result<ai::Question, String> {
    ai::prepare(&settings.ai, &environment(pairs), "sqlite", "SCHEMA", "Q?")
        .map_err(|error| format!("{error:#}"))
}

#[test]
fn describes_the_schema_without_sending_any_row_data() {
    let path = fixture("summary");
    let tables = vec![Table::bare("customers"), Table::bare("invoices")];

    let summary = ai::summary(&tables, Some(&Table::bare("customers")), columns_of(&path));

    assert!(summary.contains("customers ("));
    assert!(summary.contains("email"));
    assert!(summary.contains("invoices ("));
    assert!(!summary.contains("Karl"));
    assert!(!summary.contains("karl@example.com"));
    assert_eq!(
        summary,
        "customers (id integer, name text, email text)\ninvoices (id integer, customer_id integer, total numeric)"
    );
}

#[test]
fn puts_the_table_you_are_looking_at_first() {
    let path = fixture("focus");
    let tables = vec![Table::bare("customers"), Table::bare("invoices")];

    let summary = ai::summary(&tables, Some(&Table::bare("invoices")), columns_of(&path));

    assert!(summary.starts_with("invoices ("));
}

#[test]
fn sends_forty_tables_and_counts_the_rest() {
    let tables: Vec<Table> = (1..=45).map(|n| Table::bare(format!("t{n}"))).collect();

    let summary = ai::summary(&tables, Some(&Table::bare("t44")), |_| Vec::new());

    let lines: Vec<&str> = summary.lines().collect();
    assert_eq!(lines.len(), 41);
    assert_eq!(lines[0], "t44 ()");
    assert_eq!(lines[40], "… and 5 more tables");
}

#[test]
fn says_so_when_no_provider_is_configured() {
    let settings = configured(&[("ai.provider", json!("anthropic"))]);

    let answer = prepared(&settings, &[("ANTHROPIC_API_KEY", "")])
        .err()
        .unwrap();

    assert!(answer.contains("ANTHROPIC_API_KEY"));
}

#[test]
fn types_a_question_without_running_anything() {
    let mut fake = asked("types");

    fake.press("a");
    assert_eq!(fake.app.question.as_ref().unwrap().editor.buffer(), "");

    fake.press("how many customers");
    assert_eq!(
        fake.app.question.as_ref().unwrap().editor.buffer(),
        "how many customers"
    );
    assert_eq!(fake.app.mode, Mode::Browse);

    fake.press("\x1b");
    assert!(fake.app.question.is_none());
}

#[test]
fn shows_the_question_as_you_type_it() {
    let mut fake = asked("shows");

    fake.press("a").press("count them");

    let screen = fake.screen();
    assert!(screen.contains("ASK"));
    assert!(screen.contains("count them"));
    assert!(screen.contains("↵ asks"));
    assert!(screen.contains("↵ Ask"));
}

#[test]
fn puts_the_answer_in_the_editor_as_comments_above_the_query() {
    let sql = ai::annotate(&Answer {
        query: "select count(*)\nfrom customers".to_string(),
        explanation: "Counts every row in the customers table.".to_string(),
        notes: String::new(),
    });

    assert!(sql.contains("-- Counts every row in the customers table."));
    assert!(sql.contains("select count(*)\nfrom customers"));
    assert!(database::is_read_only(&sql));
    assert_eq!(
        sql,
        "-- Counts every row in the customers table.\n--\nselect count(*)\nfrom customers"
    );
}

#[test]
fn wraps_a_long_explanation_so_it_fits_the_editor() {
    let sql = ai::annotate(&Answer {
        query: "select 1".to_string(),
        explanation: "a long explanation that keeps going ".repeat(8),
        notes: "and a note".to_string(),
    });

    for line in sql.lines() {
        assert!(line.chars().count() <= 80, "{line}");
    }
    assert!(sql.contains("-- and a note"));
}

#[test]
fn picks_whichever_provider_has_a_key() {
    let settings = configured(&[("ai.provider", json!("auto"))]);

    let question = prepared(
        &settings,
        &[
            ("ANTHROPIC_API_KEY", ""),
            ("OPENAI_API_KEY", "sk-test"),
            ("GEMINI_API_KEY", ""),
        ],
    )
    .unwrap();

    assert_eq!(question.provider.name, "openai");
    assert_eq!(question.model, "gpt-5");
}

#[test]
fn honours_a_provider_you_name() {
    let settings = configured(&[("ai.provider", json!("anthropic"))]);

    let question = prepared(
        &settings,
        &[
            ("ANTHROPIC_API_KEY", "sk-ant"),
            ("OPENAI_API_KEY", "sk-openai"),
        ],
    )
    .unwrap();

    assert_eq!(question.provider.name, "anthropic");
    assert_eq!(question.model, "claude-sonnet-5");
}

#[test]
fn lets_you_name_the_model_yourself() {
    let settings = configured(&[("ai.model", json!("claude-opus-5"))]);

    assert_eq!(ai::model(&settings.ai, "anthropic"), "claude-opus-5");
}

#[test]
fn says_which_key_is_missing_for_the_provider_you_named() {
    let settings = configured(&[("ai.provider", json!("groq"))]);

    assert!(
        prepared(&settings, &[])
            .err()
            .unwrap()
            .contains("GROQ_API_KEY")
    );
}

#[test]
fn uses_a_local_endpoint_when_one_is_configured() {
    let settings = configured(&[
        ("ai.provider", json!("auto")),
        ("ai.url", json!("http://localhost:1234/v1")),
        ("ai.model", json!("qwen2.5-coder-7b")),
    ]);

    let question = prepared(&settings, &[("ANTHROPIC_API_KEY", "sk-ant")]).unwrap();

    assert_eq!(question.provider.name, ai::LOCAL);
    assert_eq!(question.provider.url, "http://localhost:1234/v1");
    assert_eq!(question.model, "qwen2.5-coder-7b");
}

#[test]
fn does_not_need_a_key_for_a_local_endpoint() {
    let settings = configured(&[
        ("ai.provider", json!("auto")),
        ("ai.url", json!("http://localhost:1234/v1")),
    ]);

    let question = prepared(&settings, &[]).unwrap();

    assert_eq!(question.provider.name, ai::LOCAL);
    assert!(ai::request(&question).headers.is_empty());
}

#[test]
fn tells_you_what_to_do_when_nothing_is_configured_at_all() {
    let settings = configured(&[("ai.provider", json!("auto"))]);

    let answer = prepared(&settings, &[("ANTHROPIC_API_KEY", "")])
        .err()
        .unwrap();

    assert!(answer.contains("LM Studio"));
    assert!(answer.starts_with("no model to ask"));
}

#[test]
fn does_not_reach_for_ollama_unless_you_point_at_it() {
    let settings = configured(&[("ai.provider", json!("auto"))]);

    assert!(prepared(&settings, &[]).is_err());
    assert_eq!(
        prepared(&settings, &[("OLLAMA_URL", "http://localhost:11434")])
            .unwrap()
            .provider
            .name,
        "ollama"
    );
    assert_eq!(
        prepared(
            &settings,
            &[
                ("OLLAMA_URL", "http://localhost:11434"),
                ("ANTHROPIC_API_KEY", "sk")
            ]
        )
        .unwrap()
        .provider
        .name,
        "anthropic"
    );
}

#[test]
fn never_picks_a_provider_that_cannot_write_sql() {
    let settings = configured(&[("ai.provider", json!("auto"))]);

    for variable in [
        "COHERE_API_KEY",
        "JINA_API_KEY",
        "VOYAGEAI_API_KEY",
        "ELEVENLABS_API_KEY",
    ] {
        assert!(
            prepared(&settings, &[(variable, "key")]).is_err(),
            "{variable}"
        );
    }
}

#[test]
fn sends_the_question_on_enter() {
    let mut fake = asked_with("enter", configured(&[("ai.provider", json!("auto"))]), &[]);

    fake.press("a").press("how many customers").press("\n");

    assert!(fake.app.status().contains("no model to ask"));
    assert!(fake.app.question.is_some());
}

#[test]
fn adds_a_line_on_shift_enter_instead_of_sending() {
    let mut fake = asked("shift-enter");

    fake.press("a")
        .press("first line")
        .press("\x1b[13;2u")
        .press("second line");

    assert_eq!(
        fake.app.question.as_ref().unwrap().editor.buffer(),
        "first line\nsecond line"
    );
    assert_eq!(fake.app.status(), "");
}

#[test]
fn accepts_alt_enter_for_a_new_line_too() {
    for key in ["\x1b\r", "\x1b\n"] {
        let mut fake = asked("alt-enter");

        fake.press("a").press("one").press(key).press("two");

        assert_eq!(
            fake.app.question.as_ref().unwrap().editor.buffer(),
            "one\ntwo"
        );
    }
}

#[test]
fn still_sends_on_ctrl_s() {
    let mut fake = asked_with("ctrl-s", configured(&[("ai.provider", json!("auto"))]), &[]);

    fake.press("a").press("count them").press("\x13");

    assert!(fake.app.status().contains("no model to ask"));
}

#[test]
fn sends_nothing_for_an_empty_question() {
    let mut fake = asked_with("empty", configured(&[("ai.provider", json!("auto"))]), &[]);

    fake.press("a").press("   ").press("\n");

    assert_eq!(fake.app.status(), "");
    assert!(!fake.app.question.as_ref().unwrap().is_waiting());
}

#[test]
fn uses_a_local_endpoint_when_the_named_provider_has_no_key() {
    let settings = configured(&[
        ("ai.provider", json!("anthropic")),
        ("ai.url", json!("http://localhost:1234/v1")),
        ("ai.model", json!("qwen2.5-coder-7b-instruct")),
    ]);

    let question = prepared(&settings, &[("ANTHROPIC_API_KEY", "")]).unwrap();

    assert_eq!(question.provider.name, ai::LOCAL);
}

#[test]
fn says_both_ways_out_when_there_is_no_key_and_no_endpoint() {
    let settings = configured(&[("ai.provider", json!("anthropic"))]);

    let missing = ai::missing(&settings.ai);

    assert!(missing.contains("ANTHROPIC_API_KEY"));
    assert!(missing.contains("[ai] url"));
}

#[test]
fn reads_the_answer_a_local_reasoning_model_left_in_its_thinking() {
    for content in ["", "<think></think>"] {
        let server = Server::replying(json!({
            "id": "chatcmpl-1",
            "object": "chat.completion",
            "model": "qwen3-8b",
            "choices": [{
                "index": 0,
                "finish_reason": "stop",
                "message": {
                    "role": "assistant",
                    "content": content,
                    "reasoning_content": "Thinking. {\"query\": \"select count(*) from customers\", \"explanation\": \"counts them\"}",
                },
            }],
            "usage": {"prompt_tokens": 10, "completion_tokens": 20, "total_tokens": 30},
        }));
        let question = prepared(&local(&server), &[]).unwrap();

        let answer = ai::ask(&question).unwrap();

        assert_eq!(answer.query, "select count(*) from customers");
        assert_eq!(answer.explanation, "counts them");
    }
}

#[test]
fn every_provider_is_asked_the_way_tql_asked_it() {
    let captured: Value = serde_json::from_str(include_str!("fixtures/ai-wire.json")).unwrap();
    let bases: [(&str, &str, &str); 11] = [
        ("anthropic", "ANTHROPIC_URL", "/v1"),
        ("openai", "OPENAI_URL", "/v1"),
        ("azure", "AZURE_OPENAI_URL", ""),
        ("gemini", "GEMINI_URL", "/v1beta/"),
        ("groq", "GROQ_URL", "/openai/v1"),
        ("mistral", "MISTRAL_URL", "/v1"),
        ("deepseek", "DEEPSEEK_URL", "/v1"),
        ("xai", "XAI_URL", "/v1"),
        ("openrouter", "OPENROUTER_URL", "/api/v1"),
        ("ollama", "OLLAMA_URL", ""),
        ("openai-compatible", "OPENAI_COMPATIBLE_URL", "/v1"),
    ];
    for (provider, variable, path) in bases {
        let expected = &captured[provider];
        let server = Server::start(500, json!({"nope": 1}), Duration::ZERO);
        let base = format!("{}{path}", server.url);
        let key_variable = ai::key_variable(provider);
        let settings = configured(&[
            ("ai.provider", json!(provider)),
            ("ai.model", expected["model"].clone()),
        ]);
        let question = prepared(&settings, &[(variable, &base), (&key_variable, "KEY")]).unwrap();

        let answer = ai::ask(&question);
        let request = server.request();

        assert_eq!(request.path, expected["path"], "{provider}");
        assert_eq!(request.body, expected["body"], "{provider}");
        for (name, value) in expected["headers"].as_object().unwrap() {
            assert_eq!(
                request.headers.get(name).map(String::as_str),
                value.as_str(),
                "{provider} {name}"
            );
        }
        assert_eq!(
            answer.err().map(|error| format!("{error:#}")).as_deref(),
            Some(
                "the model could not be reached: HTTP request returned status code 500:\n{\"nope\":1}\n"
            ),
            "{provider}"
        );
    }
}

#[test]
fn reads_each_provider_answer() {
    let text = answer_text();
    let replies: [(&str, &str, &str, Value); 7] = [
        (
            "anthropic",
            "ANTHROPIC_URL",
            "/v1",
            json!({"type": "message", "content": [{"type": "thinking", "thinking": "hmm"}, {"type": "text", "text": text}], "stop_reason": "end_turn"}),
        ),
        (
            "openai",
            "OPENAI_URL",
            "/v1",
            json!({"id": "resp_1", "status": "completed", "error": null, "output": [
                {"type": "reasoning", "summary": []},
                {"type": "message", "role": "assistant", "content": [{"type": "output_text", "text": text}]},
            ]}),
        ),
        (
            "gemini",
            "GEMINI_URL",
            "/v1beta/",
            json!({"status": "completed", "steps": [
                {"type": "thought", "summary": [{"type": "text", "text": "thinking"}]},
                {"type": "model_output", "content": [{"type": "text", "text": text}]},
            ]}),
        ),
        ("groq", "GROQ_URL", "/openai/v1", chat(&text)),
        (
            "mistral",
            "MISTRAL_URL",
            "/v1",
            json!({"choices": [{"message": {"role": "assistant", "content": [
                {"type": "thinking", "thinking": [{"type": "text", "text": "hmm"}]},
                {"type": "text", "text": text},
            ]}}]}),
        ),
        (
            "deepseek",
            "DEEPSEEK_URL",
            "/v1",
            chat(&format!("```json\n{text}\n```")),
        ),
        (
            "ollama",
            "OLLAMA_URL",
            "",
            json!({"model": "llama3.2", "message": {"role": "assistant", "content": text}, "done": true}),
        ),
    ];
    for (provider, variable, path, reply) in replies {
        let server = Server::replying(reply);
        let base = format!("{}{path}", server.url);
        let key_variable = ai::key_variable(provider);
        let settings = configured(&[("ai.provider", json!(provider))]);
        let question = prepared(&settings, &[(variable, &base), (&key_variable, "KEY")]).unwrap();

        let answer = ai::ask(&question).unwrap_or_else(|error| panic!("{provider}: {error}"));

        assert_eq!(
            answer.query, "select count(*)\nfrom customers",
            "{provider}"
        );
        assert_eq!(
            answer.explanation,
            "Counts every row in the customers table."
        );
        assert_eq!(answer.notes, "");
    }
}

#[test]
fn says_what_went_wrong_in_the_providers_words() {
    let cases: [(u16, Value, &str); 4] = [
        (
            429,
            json!({}),
            "Application rate limited by AI provider [tql-local].",
        ),
        (
            402,
            json!({}),
            "AI provider [tql-local] has insufficient credits or quota.",
        ),
        (503, json!({}), "AI provider [tql-local] is overloaded."),
        (
            200,
            json!({"error": {"type": "invalid_request", "message": "no such model"}}),
            "OpenAI-compatible Error: [invalid_request] no such model",
        ),
    ];
    let long = json!({"error": "x".repeat(130)}).to_string();
    let truncated = format!(
        "HTTP request returned status code 404:\n{} (truncated...)\n",
        &long[..120]
    );
    let cases: Vec<(u16, Value, &str)> = cases
        .into_iter()
        .chain([(404, json!({"error": "x".repeat(130)}), truncated.as_str())])
        .collect();
    for (status, reply, said) in cases {
        let server = Server::start(status, reply, Duration::ZERO);
        let question = prepared(&local(&server), &[]).unwrap();

        assert_eq!(
            format!("{:#}", ai::ask(&question).err().unwrap()),
            format!("the model could not be reached: {said}")
        );
    }
}

#[test]
fn says_so_when_the_endpoint_is_not_there() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    drop(listener);
    let settings = configured(&[("ai.url", json!(format!("http://{address}/v1")))]);
    let question = prepared(&settings, &[]).unwrap();

    assert_eq!(
        format!("{:#}", ai::ask(&question).err().unwrap()),
        "the model could not be reached: Could not connect to AI provider [tql-local]."
    );
}

#[test]
fn says_so_when_the_model_answers_with_no_query() {
    let server = Server::replying(chat("I am not sure what you mean."));
    let question = prepared(&local(&server), &[]).unwrap();

    assert_eq!(
        format!("{:#}", ai::ask(&question).err().unwrap()),
        "the model did not answer with a query"
    );
}

#[test]
fn writes_the_answer_into_the_sql_editor_and_runs_nothing() {
    let server = Server::replying(chat(&answer_text()));
    let mut fake = asked_with("answered", local(&server), &[]);

    fake.press("a").press("how many customers").press("\n");
    let request = server.request();
    wait(&mut fake);

    assert_eq!(request.body["messages"][1]["content"], "how many customers");
    let system = request.body["messages"][0]["content"].as_str().unwrap();
    assert!(system.contains("for a sqlite database"));
    assert!(system.contains("customers (id integer, name text, email text)"));
    assert!(!system.contains("Karl"));
    assert!(fake.app.question.is_none());
    assert_eq!(fake.app.mode, Mode::Query);
    assert_eq!(
        fake.app.query.editor.buffer(),
        "-- Counts every row in the customers table.\n--\nselect count(*)\nfrom customers"
    );
    assert_eq!(fake.app.query.editor.cursor(), 0);
    assert_eq!(
        fake.app.status(),
        "ctrl+r runs it · read it first · esc returns"
    );
    assert!(
        !fake
            .app
            .last_statement
            .as_deref()
            .unwrap_or_default()
            .contains("count(*)")
    );
    assert!(!fake.app.query.results);
}

#[test]
fn names_the_vim_run_key_when_the_editor_is_vim() {
    let server = Server::replying(chat(&answer_text()));
    let mut settings = local(&server);
    settings.set("ui.sql_editor", &toml::Value::String("vim".into()));
    let mut fake = asked_with("answered-vim", settings, &[]);

    fake.press("a").press("how many customers").press("\n");
    wait(&mut fake);

    assert_eq!(
        fake.app.status(),
        ":r runs it · read it first · esc returns"
    );
}

#[test]
fn one_undo_brings_back_the_sql_the_answer_replaced() {
    let server = Server::replying(chat(&answer_text()));
    let mut fake = asked_with("answered-undo", local(&server), &[]);
    fake.press("s").press(" -- mine").press("\x1b").press("\t");
    let before = fake.app.query.editor.buffer();

    fake.press("a").press("how many customers").press("\n");
    wait(&mut fake);
    fake.press("\x1a");

    assert!(before.ends_with(" -- mine"));
    assert_eq!(fake.app.query.editor.buffer(), before);
}

#[test]
fn keeps_the_question_open_when_there_is_no_query_for_it() {
    for (notes, said) in [
        ("There is no orders table.", "There is no orders table."),
        ("", "no query for that"),
    ] {
        let server = Server::replying(chat(
            &json!({"query": "", "explanation": "", "notes": notes}).to_string(),
        ));
        let mut fake = asked_with("no-query", local(&server), &[]);

        fake.press("a").press("how many orders").press("\n");
        wait(&mut fake);

        assert_eq!(fake.app.status(), said);
        assert_eq!(
            fake.app.question.as_ref().unwrap().editor.buffer(),
            "how many orders"
        );
        assert_eq!(fake.app.mode, Mode::Browse);
    }
}

#[test]
fn shows_the_failure_on_one_line_and_keeps_the_question() {
    let server = Server::start(500, json!({"error": "boom"}), Duration::ZERO);
    let mut fake = asked_with("failed", local(&server), &[]);

    fake.press("a").press("count them").press("\n");
    wait(&mut fake);

    assert_eq!(
        fake.app.status(),
        "the model could not be reached: HTTP request returned status code 500: {\"error\":\"boom\"}"
    );
    assert!(fake.app.question.is_some());
}

#[test]
fn waits_without_freezing_and_esc_gives_up_on_the_answer() {
    let server = Server::start(200, chat(&answer_text()), Duration::from_millis(400));
    let mut fake = asked_with("cancelled", local(&server), &[]);
    let before = fake.app.query.editor.buffer();

    fake.press("a").press("count them").press("\n");
    server.request();

    assert!(fake.app.question.as_ref().unwrap().is_waiting());
    let screen = fake.screen();
    assert!(screen.contains("asking…"));
    assert!(screen.contains("esc Cancel"));
    fake.press("more");
    assert_eq!(
        fake.app.question.as_ref().unwrap().editor.buffer(),
        "count them"
    );

    fake.press("\x1b");
    std::thread::sleep(Duration::from_millis(600));
    fake.settle();

    assert!(fake.app.question.is_none());
    assert_eq!(fake.app.mode, Mode::Browse);
    assert_eq!(fake.app.query.editor.buffer(), before);
}

#[test]
fn a_paste_into_the_question_keeps_its_lines() {
    let mut fake = asked("paste");

    fake.press("a").paste("how many\r\ncustomers\x07");

    assert_eq!(
        fake.app.question.as_ref().unwrap().editor.buffer(),
        "how many\ncustomers"
    );
}

#[test]
fn the_ai_settings_come_from_the_environment_too() {
    let table: toml::Table = toml::from_str(
        "[ai]\nprovider = \"auto\"\nmodel = \"\"\ntimeout = 60\nurl = \"\"\nkey = \"\"\n",
    )
    .unwrap();
    let variables = environment(&[
        ("TQL_AI_PROVIDER", "groq"),
        ("TQL_AI_MODEL", "llama"),
        ("TQL_AI_TIMEOUT", "5"),
        ("TQL_AI_URL", "http://localhost:1234"),
        ("TQL_AI_KEY", "lm"),
    ]);

    let settings = Settings::from_sources(&table, variables);

    assert_eq!(settings.ai.provider, "groq");
    assert_eq!(settings.ai.model, "llama");
    assert_eq!(settings.ai.timeout, 5);
    assert_eq!(settings.ai.url, "http://localhost:1234");
    assert_eq!(settings.ai.key, "lm");
}
