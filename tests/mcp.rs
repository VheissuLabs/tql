use std::fs;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::{Duration, Instant};

use app::agent::Agent;
use app::database;
use app::live::{self, Live};
use app::mcp::Server;
use app::paths;
use app::settings::{self, Settings};
use app::store::Connection;
use app::terminal::Fake;
use app::tui::{Browser, Mode};
use serde_json::{Value, json};

struct Scratch {
    root: PathBuf,
}

impl Scratch {
    fn new(label: &str) -> Self {
        let root = std::env::temp_dir().join(format!("tql-mcp-{}-{label}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        paths::use_config_directory(root.join("tql"));
        Scratch { root }
    }

    fn shop(label: &str) -> Self {
        let scratch = Scratch::new(label);
        let shop = scratch.shop_file();
        rusqlite::Connection::open(&shop)
            .unwrap()
            .execute_batch(
                "create table users (id integer primary key, name text not null);
                 insert into users (name) values ('Ann'), ('Bob');",
            )
            .unwrap();
        app::application().run_with(["open", &shop.display().to_string(), "--name=shop"], "");
        scratch
    }

    fn shop_file(&self) -> PathBuf {
        self.root.join("shop.sqlite")
    }

    fn live(&self) -> PathBuf {
        self.root.join("live")
    }

    fn server(&self) -> Server {
        Server::new(Agent::new(paths::store(), paths::key()), "v9.8.7")
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn ask(server: &Server, method: &str, params: Value) -> Value {
    let request = json!({ "jsonrpc": "2.0", "id": 7, "method": method, "params": params });
    serde_json::from_str(&server.handle(&request.to_string()).unwrap()).unwrap()
}

fn call(server: &Server, tool: &str, arguments: Value) -> (String, bool) {
    let reply = ask(
        server,
        "tools/call",
        json!({ "name": tool, "arguments": arguments }),
    );
    let result = &reply["result"];
    (
        result["content"][0]["text"].as_str().unwrap().to_string(),
        result["isError"].as_bool().unwrap(),
    )
}

#[test]
fn introduces_itself_without_the_v_of_its_version() {
    let scratch = Scratch::new("initialize");

    let reply = ask(
        &scratch.server(),
        "initialize",
        json!({ "protocolVersion": "2025-06-18", "capabilities": {}, "clientInfo": { "name": "t", "version": "1" } }),
    );

    let result = &reply["result"];
    assert_eq!(result["resultType"], "complete");
    assert_eq!(result["protocolVersion"], "2025-06-18");
    assert_eq!(
        result["serverInfo"],
        json!({ "name": "tql", "version": "9.8.7" })
    );
    assert_eq!(
        result["_meta"]["io.modelcontextprotocol/serverInfo"]["version"],
        "9.8.7"
    );
    let keys: Vec<&String> = result.as_object().unwrap().keys().collect();
    assert_eq!(
        keys,
        [
            "resultType",
            "protocolVersion",
            "capabilities",
            "serverInfo",
            "instructions",
            "_meta"
        ]
    );
}

#[test]
fn offers_its_newest_protocol_to_a_client_it_does_not_know() {
    let scratch = Scratch::new("old-protocol");

    let reply = ask(
        &scratch.server(),
        "initialize",
        json!({ "protocolVersion": "2024-11-05" }),
    );

    assert_eq!(reply["result"]["protocolVersion"], "2025-11-25");
}

#[test]
fn lists_the_tools_by_the_names_tql_has_always_used() {
    let scratch = Scratch::new("tools");

    let reply = ask(&scratch.server(), "tools/list", json!({}));

    let names: Vec<&str> = reply["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|tool| tool["name"].as_str().unwrap())
        .collect();
    assert_eq!(
        names,
        [
            "list-connections-tool",
            "list-tables-tool",
            "describe-table-tool",
            "run-query-tool",
            "write-sql-tool"
        ]
    );
    assert_eq!(reply["result"]["ttlMs"], 0);
    assert_eq!(
        reply["result"]["tools"][4]["inputSchema"]["required"],
        json!(["connection", "sql"])
    );
}

#[test]
fn the_tools_give_the_same_answers_as_the_commands() {
    let scratch = Scratch::shop("same-answers");
    let server = scratch.server();
    let command = |arguments: &[&str]| {
        app::application()
            .run_with(arguments.iter().copied(), "")
            .stdout
            .trim_end()
            .to_string()
    };

    assert_eq!(
        call(&server, "list-tables-tool", json!({ "connection": "shop" })),
        (command(&["tables", "shop"]), false)
    );
    assert_eq!(
        call(
            &server,
            "describe-table-tool",
            json!({ "connection": "shop", "table": "users" })
        ),
        (command(&["describe", "shop", "users"]), false)
    );
    let (listed, failed) = call(&server, "list-connections-tool", json!({}));
    assert!(!failed);
    assert_eq!(listed, command(&["connections"]));
    let (ran, failed) = call(
        &server,
        "run-query-tool",
        json!({ "connection": "shop", "query": "select name from users" }),
    );
    assert!(!failed);
    let ran: Value = serde_json::from_str(&ran).unwrap();
    assert_eq!(
        ran["results"],
        json!([{ "name": "Ann" }, { "name": "Bob" }])
    );
}

#[test]
fn says_so_when_there_are_no_connections() {
    let scratch = Scratch::new("none");

    assert_eq!(
        call(&scratch.server(), "list-connections-tool", json!({})),
        (
            "No connections are configured in tql yet.".to_string(),
            false
        )
    );
}

#[test]
fn a_refusal_is_an_error_the_agent_can_read() {
    let scratch = Scratch::shop("refusals");
    let server = scratch.server();

    let (refused, failed) = call(
        &server,
        "run-query-tool",
        json!({ "connection": "shop", "query": "delete from users" }),
    );
    assert!(failed);
    assert!(refused.starts_with("Only read-only statements are allowed here"));

    assert_eq!(
        call(&server, "list-tables-tool", json!({})),
        (
            "There is no tql connection named []. Known connections: shop".to_string(),
            true
        )
    );
}

#[test]
fn records_queries_as_from_the_agent() {
    let scratch = Scratch::shop("history");

    call(
        &scratch.server(),
        "run-query-tool",
        json!({ "connection": "shop", "query": "select 1" }),
    );

    let store = rusqlite::Connection::open(paths::store()).unwrap();
    let source: String = store
        .query_row("select source from query_executions", [], |row| row.get(0))
        .unwrap();
    assert_eq!(source, "mcp");
}

#[test]
fn answers_mistakes_in_json_rpc_terms() {
    let scratch = Scratch::new("mistakes");
    let server = scratch.server();

    assert_eq!(
        ask(&server, "tools/call", json!({ "name": "bogus" }))["error"],
        json!({ "code": -32602, "message": "Tool [bogus] not found." })
    );
    assert_eq!(
        ask(&server, "tools/call", json!({}))["error"],
        json!({ "code": -32602, "message": "Missing [name] parameter." })
    );
    assert_eq!(
        ask(&server, "nope/nope", json!({}))["error"],
        json!({ "code": -32601, "message": "The method [nope/nope] was not found." })
    );
    assert_eq!(
        server.handle("not json").unwrap(),
        r#"{"jsonrpc":"2.0","error":{"code":-32700,"message":"Parse error: Invalid JSON was received by the server."}}"#
    );
    assert_eq!(
        server.handle(r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#),
        None
    );
}

#[test]
fn a_versioned_request_must_ask_for_the_version_it_supports() {
    let scratch = Scratch::new("versioned");
    let server = scratch.server();
    let meta = |version: &str| {
        json!({ "_meta": {
            "io.modelcontextprotocol/protocolVersion": version,
            "io.modelcontextprotocol/clientCapabilities": {},
        } })
    };

    assert!(ask(&server, "tools/list", meta("2026-07-28"))["result"].is_object());
    assert_eq!(
        ask(&server, "tools/list", meta("2025-06-18"))["error"],
        json!({
            "code": -32022,
            "message": "Unsupported protocol version",
            "data": { "supported": ["2026-07-28"], "requested": "2025-06-18" },
        })
    );
    assert_eq!(
        ask(
            &server,
            "tools/list",
            json!({ "_meta": { "io.modelcontextprotocol/protocolVersion": "2026-07-28" } })
        )["error"]["message"],
        "Invalid params: The request [_meta] is missing the required [io.modelcontextprotocol/clientCapabilities] member."
    );
}

#[test]
fn the_command_answers_one_line_at_a_time_until_stdin_ends() {
    let _scratch = Scratch::new("stdio");

    let served = app::application().run_with(
        ["mcp:start", "tql"],
        "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"ping\"}\n\n{\"jsonrpc\":\"2.0\",\"method\":\"notifications/initialized\"}\n{\"jsonrpc\":\"2.0\",\"id\":2,\"method\":\"prompts/list\"}\n",
    );

    assert_eq!(served.code, 0);
    assert_eq!(served.stderr, "");
    let lines: Vec<Value> = served
        .stdout
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(lines.len(), 2);
    assert_eq!(lines[0]["id"], 1);
    assert_eq!(lines[1]["result"]["prompts"], json!([]));
}

#[test]
fn only_the_tql_server_starts() {
    let _scratch = Scratch::new("handle");

    let refused = app::application().run_with(["mcp:start", "foo"], "");

    assert_eq!(refused.code, 1);
    assert_eq!(
        refused.stderr,
        "There is no MCP server named [foo]. tql has one, named [tql].\n"
    );
    assert!(
        app::application()
            .run_with(["list"], "")
            .stdout
            .contains("mcp:start")
    );
}

fn browser(scratch: &Scratch, live: Option<Live>) -> Fake<Browser> {
    settings::install(Settings::default());
    let connection = Connection::sqlite("shop", scratch.shop_file().to_str().unwrap());
    let database = database::connect(&connection, None).unwrap();
    Fake::sized(
        Browser::new(connection, database).unwrap().with_live(live),
        120,
        30,
    )
}

fn deliver_while(fake: &mut Fake<Browser>, directory: &Path, sql: &str) -> String {
    let directory = directory.to_path_buf();
    let sql = sql.to_string();
    let delivering = thread::spawn(move || live::deliver(&directory, "shop", &sql));
    let started = Instant::now();
    while !delivering.is_finished() && started.elapsed() < Duration::from_secs(5) {
        fake.settle();
        thread::sleep(Duration::from_millis(10));
    }
    delivering.join().unwrap().unwrap()
}

#[test]
fn the_agents_sql_takes_the_place_of_the_editors_and_one_undo_brings_it_back() {
    let scratch = Scratch::shop("write-sql");
    let mut fake = browser(&scratch, Live::register(&scratch.live(), "shop"));
    fake.settle().press("s").press("select 1");
    let before = fake.app.query.editor.buffer();
    fake.press("\x1b");

    let said = deliver_while(
        &mut fake,
        &scratch.live(),
        "update users\r\nset name = 'Al'",
    );

    assert!(
        said.starts_with("The SQL is in the SQL editor of the tql open on [shop]."),
        "{said}"
    );
    assert_eq!(
        fake.app.query.editor.buffer(),
        "update users\nset name = 'Al'"
    );
    assert_eq!(fake.app.mode, Mode::Query);
    assert!(
        fake.screen()
            .contains("SQL from your agent · nothing has run")
    );
    let names = database::connect(
        &Connection::sqlite("shop", scratch.shop_file().to_str().unwrap()),
        None,
    )
    .unwrap()
    .run("select name from users where name = 'Al'")
    .unwrap();
    assert!(names.rows.is_empty());

    fake.press("\x1a");
    assert_eq!(fake.app.query.editor.buffer(), before);
}

#[test]
fn the_agents_sql_waits_in_the_editor_while_a_form_is_open() {
    let scratch = Scratch::shop("write-sql-busy");
    let mut fake = browser(&scratch, Live::register(&scratch.live(), "shop"));
    fake.settle().press("/");

    deliver_while(&mut fake, &scratch.live(), "select 2");

    assert!(fake.app.filtering);
    assert_ne!(fake.app.mode, Mode::Query);
    assert_eq!(fake.app.query.editor.buffer(), "select 2");
}

#[test]
fn the_most_recently_used_tql_on_the_connection_gets_the_sql() {
    let scratch = Scratch::shop("write-sql-recent");
    let mut older = browser(&scratch, Live::register(&scratch.live(), "shop"));
    let mut newer = browser(&scratch, Live::register(&scratch.live(), "shop"));
    older.settle();
    newer.settle();
    thread::sleep(Duration::from_millis(1100));
    newer.press("j");
    let before = older.app.query.editor.buffer();

    deliver_while(&mut newer, &scratch.live(), "select 3");

    assert_eq!(newer.app.query.editor.buffer(), "select 3");
    assert_eq!(older.settle().app.query.editor.buffer(), before);
}

#[test]
fn says_which_connections_are_open_when_the_named_one_is_not() {
    let scratch = Scratch::shop("write-sql-elsewhere");

    assert_eq!(
        live::deliver(&scratch.live(), "shop", "select 1"),
        Err("tql is not open. Ask the user to open [shop] in tql, then try again.".to_string())
    );

    let _open = Live::register(&scratch.live(), "warehouse");
    assert_eq!(
        live::deliver(&scratch.live(), "shop", "select 1"),
        Err("No tql is open on [shop]. tql is open on: warehouse".to_string())
    );
    assert_eq!(
        live::deliver(&scratch.live(), "warehouse", "  "),
        Err("Give the SQL to put in the editor.".to_string())
    );
}

#[test]
fn a_tql_that_closed_leaves_nothing_behind() {
    let scratch = Scratch::shop("write-sql-closed");

    drop(Live::register(&scratch.live(), "shop"));
    fs::write(scratch.live().join("1-1.json"), r#"{"connection":"shop"}"#).unwrap();

    assert!(live::deliver(&scratch.live(), "shop", "select 1").is_err());
    assert_eq!(fs::read_dir(scratch.live()).unwrap().count(), 0);
}

#[test]
fn a_tql_that_is_busy_gets_the_sql_when_it_is_free() {
    let scratch = Scratch::shop("write-sql-later");
    let mut fake = browser(&scratch, Live::register(&scratch.live(), "shop"));

    let said = live::deliver(&scratch.live(), "shop", "select 4").unwrap();
    assert!(said.starts_with("tql on [shop] is busy"), "{said}");

    fake.settle();
    assert_eq!(fake.app.query.editor.buffer(), "select 4");
}

#[test]
fn in_vim_the_agents_sql_lands_in_normal_mode_and_u_brings_back_what_was_there() {
    let scratch = Scratch::shop("write-sql-vim");
    let mut fake = browser(&scratch, Live::register(&scratch.live(), "shop"));
    let mut vim = Settings::default();
    vim.set("ui.sql_editor", &toml::Value::String("vim".into()));
    settings::install(vim);
    fake.settle();
    let before = fake.app.query.editor.buffer();

    deliver_while(&mut fake, &scratch.live(), "select 5");

    assert_eq!(fake.app.query.editor.buffer(), "select 5");
    assert_eq!(fake.app.query.vim.mode, app::tui::vim::VimMode::Normal);
    assert!(fake.app.query.editor.cursor() < "select 5".len());
    fake.press("u");
    assert_eq!(fake.app.query.editor.buffer(), before);
}
