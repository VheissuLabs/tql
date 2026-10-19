use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::sync::{Arc, Mutex};

use app::database::{self, Database, Filter, Filters, Joiner, Kind, Table};
use app::driver::Driver;
use app::encrypter::Encrypter;
use app::store::Connection;
use serde_json::{Value, json};

#[derive(Debug, Clone)]
struct Request {
    method: String,
    path: String,
    headers: HashMap<String, String>,
    body: Value,
    raw: String,
}

type Handler = dyn Fn(&Request) -> (u16, Value) + Send + Sync;

struct Server {
    url: String,
    requests: Arc<Mutex<Vec<Request>>>,
}

impl Server {
    fn start(handler: impl Fn(&Request) -> (u16, Value) + Send + Sync + 'static) -> Server {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let requests = Arc::new(Mutex::new(Vec::new()));
        let recorded = requests.clone();
        let handler: Arc<Handler> = Arc::new(handler);
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { return };
                let request = read(&mut stream);
                let (status, reply) = handler(&request);
                recorded.lock().unwrap().push(request);
                let body = reply.to_string();
                let location = match reply["location"].as_str() {
                    Some(location) if status == 302 => format!("location: {location}\r\n"),
                    _ => String::new(),
                };
                let _ = write!(
                    stream,
                    "HTTP/1.1 {status} OK\r\n{location}content-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                    body.len()
                );
            }
        });
        Server { url, requests }
    }

    fn statements(&self) -> Vec<String> {
        self.requests
            .lock()
            .unwrap()
            .iter()
            .filter_map(|request| request.body["statement"].as_str().map(String::from))
            .collect()
    }
}

fn read(stream: &mut std::net::TcpStream) -> Request {
    let mut reader = BufReader::new(stream.try_clone().unwrap());
    let mut first = String::new();
    reader.read_line(&mut first).unwrap();
    let mut parts = first.split_whitespace();
    let method = parts.next().unwrap_or_default().to_string();
    let path = parts.next().unwrap_or_default().to_string();
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
    Request {
        method,
        path,
        headers,
        raw: String::from_utf8_lossy(&body).into_owned(),
        body: serde_json::from_slice(&body).unwrap_or_default(),
    }
}

fn answered(columns: &[(&str, &str, u64)], data: Value) -> Value {
    json!({
        "statementHandle": "01b2-handle",
        "resultSetMetaData": {
            "numRows": data.as_array().map_or(0, Vec::len),
            "format": "jsonv2",
            "partitionInfo": [{"rowCount": data.as_array().map_or(0, Vec::len)}],
            "rowType": columns
                .iter()
                .map(|(name, kind, scale)| json!({"name": name, "type": kind, "scale": scale, "nullable": true}))
                .collect::<Vec<_>>(),
        },
        "data": data,
    })
}

fn connect(server: &Server, database: Option<&str>) -> Box<dyn Database> {
    let encrypter = Encrypter::throwaway().unwrap();
    database::connect(
        &Connection {
            name: "snowflake".into(),
            driver: Driver::Snowflake,
            host: Some(server.url.clone()),
            database: database.map(String::from),
            password: Some(encrypter.encrypt_string("pat-secret").unwrap()),
            warehouse: Some("COMPUTE_WH".into()),
            role: Some("ANALYST".into()),
            ..Connection::default()
        },
        Some(&encrypter),
    )
    .unwrap()
}

fn warehouse(request: &Request) -> (u16, Value) {
    let statement = request.body["statement"].as_str().unwrap_or_default();
    if statement == "select current_schema()" {
        return (
            200,
            answered(&[("CURRENT_SCHEMA()", "text", 0)], json!([["PUBLIC"]])),
        );
    }
    if statement == "show terse objects in database" {
        return (
            200,
            answered(
                &[
                    ("created_on", "timestamp_ltz", 3),
                    ("name", "text", 0),
                    ("kind", "text", 0),
                    ("database_name", "text", 0),
                    ("schema_name", "text", 0),
                ],
                json!([
                    ["0.000", "TABLES", "VIEW", "ANALYTICS", "INFORMATION_SCHEMA"],
                    [
                        "1760000000.000",
                        "RAW_EVENTS",
                        "TABLE",
                        "ANALYTICS",
                        "STAGING"
                    ],
                    ["1760000000.000", "ORDERS", "TABLE", "ANALYTICS", "PUBLIC"]
                ]),
            ),
        );
    }
    if statement.starts_with("show primary keys in table") {
        return (
            200,
            answered(
                &[
                    ("schema_name", "text", 0),
                    ("table_name", "text", 0),
                    ("column_name", "text", 0),
                    ("key_sequence", "fixed", 0),
                    ("constraint_name", "text", 0),
                ],
                json!([["PUBLIC", "ORDERS", "ID", "1", "ORDERS_PK"]]),
            ),
        );
    }
    if statement.starts_with("select * from \"ORDERS\"") {
        return (
            200,
            answered(
                &[
                    ("ID", "fixed", 0),
                    ("TOTAL", "fixed", 2),
                    ("PLACED", "date", 0),
                ],
                json!([["1", "12.50", "18262"]]),
            ),
        );
    }
    (
        422,
        json!({"code": "002003", "message": format!("SQL compilation error:\nunexpected '{statement}'")}),
    )
}

#[test]
fn signs_in_with_the_token_and_sends_the_warehouse_and_role_with_every_statement() {
    let server = Server::start(warehouse);
    connect(&server, Some("ANALYTICS"));

    let requests = server.requests.lock().unwrap();
    let first = &requests[0];
    assert_eq!(first.method, "POST");
    assert_eq!(first.path, "/api/v2/statements");
    assert_eq!(first.headers["authorization"], "Bearer pat-secret");
    assert_eq!(first.body["database"], "ANALYTICS");
    assert_eq!(first.body["warehouse"], "COMPUTE_WH");
    assert_eq!(first.body["role"], "ANALYST");
}

#[test]
fn a_database_chosen_without_a_schema_settles_on_public_and_says_so_with_every_statement() {
    let server = Server::start(|request| match request.body["statement"].as_str() {
        Some("select current_schema()") if request.body["schema"].is_null() => (
            200,
            answered(&[("CURRENT_SCHEMA()", "text", 0)], json!([[null]])),
        ),
        Some("show terse schemas like 'PUBLIC'") => (
            200,
            answered(
                &[("created_on", "timestamp_ltz", 3), ("name", "text", 0)],
                json!([["1760000000.000", "PUBLIC"]]),
            ),
        ),
        _ => (200, answered(&[("N", "fixed", 0)], json!([["1"]]))),
    });
    let mut database = connect(&server, Some("ANALYTICS"));

    assert_eq!(database.default_schema().as_deref(), Some("PUBLIC"));
    database.run("select 1").unwrap();
    let requests = server.requests.lock().unwrap();
    assert_eq!(requests.last().unwrap().body["schema"], "PUBLIC");
}

#[test]
fn lists_tables_in_the_current_schema_bare_and_the_rest_by_schema() {
    let server = Server::start(warehouse);
    let mut database = connect(&server, Some("ANALYTICS"));

    assert_eq!(database.default_schema().as_deref(), Some("PUBLIC"));
    assert_eq!(
        database.tables().unwrap(),
        [
            Table::bare("ORDERS"),
            Table::in_schema("STAGING", "RAW_EVENTS")
        ]
    );
}

#[test]
fn pages_rows_in_primary_key_order_with_filters_written_as_literals() {
    let server = Server::start(warehouse);
    let mut database = connect(&server, Some("ANALYTICS"));
    let filters = Filters::new(vec![Filter::new("STATUS", "is", "it's done")], Joiner::And);

    let page = database
        .filtered_rows(&Table::bare("ORDERS"), 50, 100, None, Some(&filters))
        .unwrap();

    assert_eq!(
        page.statement,
        "select * from \"ORDERS\" where \"STATUS\" = 'it\\'s done' order by \"ID\" limit 50 offset 100"
    );
    assert_eq!(page.headers, ["ID", "TOTAL", "PLACED"]);
    assert_eq!(
        page.rows,
        [[
            Some("1".to_string()),
            Some("12.50".to_string()),
            Some("2020-01-01".to_string())
        ]]
    );
    assert_eq!(page.kinds[0], [Kind::Integer, Kind::Text, Kind::Text]);
}

#[test]
fn waits_for_a_long_statement_and_gathers_every_partition_of_its_answer() {
    let server = Server::start(|request| {
        if request.method == "POST" && request.body["statement"] != "select n from big" {
            return (200, answered(&[("name", "text", 0)], json!([["PUBLIC"]])));
        }
        if request.method == "POST" {
            return (
                202,
                json!({"code": "333334", "statementHandle": "01b2-handle", "statementStatusUrl": "/api/v2/statements/01b2-handle"}),
            );
        }
        match request.path.as_str() {
            "/api/v2/statements/01b2-handle" => {
                let mut first = answered(&[("N", "fixed", 0)], json!([["1"], ["2"]]));
                first["resultSetMetaData"]["partitionInfo"] =
                    json!([{"rowCount": 2}, {"rowCount": 1}, {"rowCount": 1}]);
                (200, first)
            }
            "/api/v2/statements/01b2-handle?partition=1" => (200, json!({"data": [["3"]]})),
            "/api/v2/statements/01b2-handle?partition=2" => (200, json!([["4"]])),
            other => (404, json!({"message": format!("no {other}")})),
        }
    });
    let mut database = connect(&server, Some("ANALYTICS"));

    let outcome = database.run("select n from big").unwrap();

    assert_eq!(
        outcome.rows,
        [["1"], ["2"], ["3"], ["4"]].map(|row| row.map(|value| Some(value.to_string())))
    );
    let fetched: Vec<String> = server
        .requests
        .lock()
        .unwrap()
        .iter()
        .filter(|request| request.method == "GET")
        .map(|request| {
            assert_eq!(request.headers["authorization"], "Bearer pat-secret");
            request.path.clone()
        })
        .collect();
    assert_eq!(
        fetched,
        [
            "/api/v2/statements/01b2-handle",
            "/api/v2/statements/01b2-handle?partition=1",
            "/api/v2/statements/01b2-handle?partition=2"
        ]
    );
}

#[test]
fn a_statement_that_changes_rows_reports_how_many() {
    let server = Server::start(|request| match request.body["statement"].as_str() {
        Some("select current_schema()") => {
            (200, answered(&[("S", "text", 0)], json!([["PUBLIC"]])))
        }
        _ => {
            let mut changed = answered(&[("number of rows updated", "fixed", 0)], json!([["3"]]));
            changed["stats"] = json!({"numRowsUpdated": 3});
            (200, changed)
        }
    });
    let mut database = connect(&server, Some("ANALYTICS"));

    let outcome = database.run("update orders set total = 0").unwrap();

    assert_eq!(outcome.affected, Some(3));
    assert!(outcome.headers.is_empty());
}

#[test]
fn says_what_snowflake_said_when_a_statement_fails() {
    let server = Server::start(warehouse);
    let mut database = connect(&server, Some("ANALYTICS"));

    let failed = database.run("select * from nope").err().unwrap();

    assert_eq!(
        failed.to_string(),
        "SQL compilation error: unexpected 'select * from nope'"
    );
}

#[test]
fn a_read_only_connection_never_sends_a_write() {
    let server = Server::start(warehouse);
    let mut database = connect(&server, Some("ANALYTICS"));
    let sent = server.statements().len();

    for writing in [
        "delete from orders",
        "with gone as (select 1) delete from orders",
        "select 1; drop table orders",
    ] {
        assert!(database.run_reading(writing).is_err(), "{writing}");
    }
    assert_eq!(server.statements().len(), sent);
}

#[test]
fn refuses_to_change_rows_from_the_grid() {
    let server = Server::start(warehouse);
    let mut database = connect(&server, Some("ANALYTICS"));

    let refused = database
        .update(&Table::bare("ORDERS"), "TOTAL", "ID", "1", Some("0"))
        .err()
        .unwrap();

    assert_eq!(refused.to_string(), database::snowflake::READ_ONLY);
}

fn query(text: &str) -> HashMap<String, String> {
    let query = text.split_once('?').map_or(text, |(_, query)| query);
    query
        .split('&')
        .filter_map(|pair| pair.split_once('='))
        .map(|(key, value)| (key.to_string(), decoded(value)))
        .collect()
}

fn decoded(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::new();
    let mut at = 0;
    while at < bytes.len() {
        match bytes[at] {
            b'%' => {
                out.push(u8::from_str_radix(&text[at + 1..at + 3], 16).unwrap());
                at += 3;
            }
            b'+' => {
                out.push(b' ');
                at += 1;
            }
            byte => {
                out.push(byte);
                at += 1;
            }
        }
    }
    String::from_utf8(out).unwrap()
}

fn signs_in_with(
    authorized: impl Fn(&HashMap<String, String>) -> String + Send + Sync + 'static,
    tokens: impl Fn(&HashMap<String, String>) -> (u16, Value) + Send + Sync + 'static,
) -> Server {
    unsafe { std::env::set_var("BROWSER", "curl -sL") };
    Server::start(move |request| {
        if request.path.starts_with("/oauth/authorize") {
            let asked = query(&request.path);
            let location = format!("{}?{}", asked["redirect_uri"], authorized(&asked));
            return (302, json!({ "location": location }));
        }
        if request.path == "/oauth/token-request" {
            return tokens(&query(&request.raw));
        }
        if request.headers["authorization"] == "Bearer expired" {
            return (
                401,
                json!({"code": "390318", "message": "OAuth access token expired."}),
            );
        }
        warehouse(request)
    })
}

fn browser_connection(server: &Server) -> Connection {
    Connection {
        name: "snowflake".into(),
        driver: Driver::Snowflake,
        host: Some(server.url.clone()),
        role: Some("analyst".into()),
        ..Connection::default()
    }
}

fn signed_in(asked: &HashMap<String, String>) -> String {
    format!("code=the-code&state={}", asked["state"])
}

#[test]
fn without_a_token_it_signs_in_through_the_browser_and_queries_with_that_sign_in() {
    let server = signs_in_with(signed_in, |_| {
        (
            200,
            json!({"access_token": "signed-in", "refresh_token": "again", "expires_in": 600, "token_type": "Bearer"}),
        )
    });

    let mut database = database::connect(&browser_connection(&server), None).unwrap();
    database.tables().unwrap();

    let requests = server.requests.lock().unwrap().clone();
    let authorize = query(&requests[0].path);
    assert_eq!(authorize["client_id"], "LOCAL_APPLICATION");
    assert_eq!(authorize["response_type"], "code");
    assert_eq!(authorize["code_challenge_method"], "S256");
    assert_eq!(
        authorize["scope"],
        "refresh_token session:role-encoded:ANALYST"
    );
    assert!(authorize["redirect_uri"].starts_with("http://127.0.0.1:"));

    let exchange = query(&requests[1].raw);
    assert_eq!(exchange["grant_type"], "authorization_code");
    assert_eq!(exchange["code"], "the-code");
    assert_eq!(exchange["redirect_uri"], authorize["redirect_uri"]);
    use base64::Engine;
    use sha2::Digest;
    let challenge = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .encode(sha2::Sha256::digest(exchange["code_verifier"].as_bytes()));
    assert_eq!(challenge, authorize["code_challenge"]);

    let statement = requests
        .iter()
        .find(|request| request.body["statement"].is_string())
        .unwrap();
    assert_eq!(statement.headers["authorization"], "Bearer signed-in");
    assert_eq!(
        statement.headers["x-snowflake-authorization-token-type"],
        "OAUTH"
    );
}

#[test]
fn an_expired_sign_in_is_renewed_with_its_refresh_token_and_the_statement_sent_again() {
    let server = signs_in_with(signed_in, |asked| match asked["grant_type"].as_str() {
        "authorization_code" => (
            200,
            json!({"access_token": "expired", "refresh_token": "again", "expires_in": 600}),
        ),
        _ => {
            assert_eq!(asked["refresh_token"], "again");
            (200, json!({"access_token": "renewed", "expires_in": 600}))
        }
    });

    let mut database = database::connect(&browser_connection(&server), None).unwrap();
    database.tables().unwrap();

    let requests = server.requests.lock().unwrap().clone();
    assert_eq!(
        requests
            .iter()
            .filter(|request| request.path.starts_with("/oauth/authorize"))
            .count(),
        1
    );
    assert!(requests.iter().any(
        |request| request.headers.get("authorization").map(String::as_str)
            == Some("Bearer renewed")
    ));
}

#[test]
fn a_refused_sign_in_says_why_and_sends_no_statement() {
    let server = signs_in_with(
        |asked| {
            format!(
                "error=access_denied&error_description=The+user+said+no&state={}",
                asked["state"]
            )
        },
        |_| panic!("no code to exchange"),
    );

    let failed = database::connect(&browser_connection(&server), None)
        .err()
        .unwrap();

    assert_eq!(
        failed.to_string(),
        "the Snowflake sign-in failed: The user said no"
    );
    assert!(server.statements().is_empty());
}

const LIVE: &str = "needs a Snowflake account seeded by tests/fixtures/snowflake/seed.sh: set TQL_TEST_SNOWFLAKE and TQL_TEST_SNOWFLAKE_TOKEN";

fn live() -> Box<dyn Database> {
    let variable = |name: &str| std::env::var(name).ok().filter(|value| !value.is_empty());
    let encrypter = Encrypter::throwaway().unwrap();
    database::connect(
        &Connection {
            name: "snowflake".into(),
            driver: Driver::Snowflake,
            host: Some(variable("TQL_TEST_SNOWFLAKE").expect(LIVE)),
            database: Some("TQL_TEST".into()),
            password: Some(
                encrypter
                    .encrypt_string(&variable("TQL_TEST_SNOWFLAKE_TOKEN").expect(LIVE))
                    .unwrap(),
            ),
            warehouse: Some(
                variable("TQL_TEST_SNOWFLAKE_WAREHOUSE").unwrap_or("COMPUTE_WH".into()),
            ),
            role: Some(variable("TQL_TEST_SNOWFLAKE_ROLE").unwrap_or("SYSADMIN".into())),
            ..Connection::default()
        },
        Some(&encrypter),
    )
    .unwrap()
}

fn cell(page: &app::database::Page, row: usize, column: &str) -> Option<String> {
    let at = page
        .headers
        .iter()
        .position(|header| header == column)
        .unwrap();
    page.rows[row][at].clone()
}

#[test]
#[ignore = "needs a Snowflake account seeded by tests/fixtures/snowflake/seed.sh: set TQL_TEST_SNOWFLAKE and TQL_TEST_SNOWFLAKE_TOKEN"]
fn live_lists_the_seeded_tables_and_views_with_other_schemas_qualified() {
    let mut database = live();

    assert_eq!(database.default_schema().as_deref(), Some("PUBLIC"));
    let tables = database.tables().unwrap();
    for expected in [
        Table::bare("ACTIVE_CUSTOMERS"),
        Table::bare("CUSTOMERS"),
        Table::bare("EVENTS"),
        Table::bare("ORDERS"),
        Table::bare("READINGS"),
        Table::in_schema("STAGING", "Mixed Case"),
        Table::in_schema("STAGING", "RAW_EVENTS"),
    ] {
        assert!(tables.contains(&expected), "{expected} in {tables:?}");
    }
    assert!(
        database
            .databases()
            .unwrap()
            .contains(&"TQL_TEST".to_string())
    );
}

#[test]
#[ignore = "needs a Snowflake account seeded by tests/fixtures/snowflake/seed.sh: set TQL_TEST_SNOWFLAKE and TQL_TEST_SNOWFLAKE_TOKEN"]
fn live_describes_columns_keys_and_the_clustering_key() {
    let mut database = live();
    let orders = Table::bare("ORDERS");

    let columns = database.columns(&orders).unwrap();
    let typed: Vec<(&str, &str, bool)> = columns
        .iter()
        .map(|column| {
            (
                column.name.as_str(),
                column.full_type.as_str(),
                column.nullable,
            )
        })
        .collect();
    assert_eq!(typed[0], ("ID", "NUMBER(38,0)", false));
    assert_eq!(typed[2], ("TOTAL", "NUMBER(10,2)", true));
    assert_eq!(typed[5], ("PLACED_AT", "TIMESTAMP_TZ(3)", true));
    assert!(
        database.columns(&Table::bare("CUSTOMERS")).unwrap()[0].auto_increment,
        "CUSTOMERS.ID is an autoincrement"
    );

    let customers = database.columns(&Table::bare("CUSTOMERS")).unwrap();
    assert_eq!(customers[3].default.as_deref(), Some("TRUE"));
    assert_eq!(customers[1].full_type, "TEXT(100)");
    assert_eq!(
        database
            .columns(&Table::bare("ACTIVE_CUSTOMERS"))
            .unwrap()
            .iter()
            .map(|column| column.name.as_str())
            .collect::<Vec<_>>(),
        ["ID", "NAME", "EMAIL"]
    );
    assert_eq!(
        database.estimated_rows(&Table::bare("READINGS")),
        Some(app::database::Total::About(60_000))
    );

    let keys = database.key_columns().unwrap();
    assert_eq!(keys[&orders], ["ID"]);
    assert_eq!(keys[&Table::bare("CUSTOMERS")], ["ID"]);

    let links = database.foreign_keys().unwrap();
    assert_eq!(links[&orders].len(), 1);
    assert_eq!(links[&orders][0].columns, ["CUSTOMER_ID"]);
    assert_eq!(links[&orders][0].table, Table::bare("CUSTOMERS"));
    assert_eq!(links[&orders][0].references, ["ID"]);

    let indexes = database.indexes(&orders).unwrap();
    assert!(
        indexes
            .iter()
            .any(|index| index.primary && index.columns == ["ID"])
    );
    assert!(indexes.iter().any(|index| index.name == "clustering key"));
    assert!(
        database
            .indexes(&Table::bare("CUSTOMERS"))
            .unwrap()
            .iter()
            .any(|index| !index.primary && index.unique && index.columns == ["EMAIL"])
    );
}

#[test]
#[ignore = "needs a Snowflake account seeded by tests/fixtures/snowflake/seed.sh: set TQL_TEST_SNOWFLAKE and TQL_TEST_SNOWFLAKE_TOKEN"]
fn live_shows_every_type_the_way_snowflake_does() {
    let mut database = live();

    let orders = database.rows(&Table::bare("ORDERS"), 10, 0, None).unwrap();
    assert_eq!(cell(&orders, 0, "TOTAL").as_deref(), Some("12.50"));
    assert_eq!(cell(&orders, 0, "PLACED_ON").as_deref(), Some("2026-01-06"));
    assert_eq!(
        cell(&orders, 0, "PLACED_AT").as_deref(),
        Some("2026-01-06 10:00:00.250 +01:00")
    );
    assert_eq!(cell(&orders, 0, "PICKUP").as_deref(), Some("17:30:00"));
    assert!(cell(&orders, 0, "SEEN_AT").unwrap().ends_with(" +00:00"));
    assert_eq!(cell(&orders, 1, "NOTE"), None);
    assert_eq!(cell(&orders, 3, "PLACED_ON").as_deref(), Some("1969-12-31"));
    assert_eq!(
        cell(&orders, 3, "NOTE").as_deref(),
        Some("refund\non two lines")
    );
    assert_eq!(orders.kinds[0][0], Kind::Integer);
    assert_eq!(orders.kinds[0][3], Kind::Float);

    let events = database.rows(&Table::bare("EVENTS"), 10, 0, None).unwrap();
    let payloads: Vec<Option<String>> = (0..3).map(|row| cell(&events, row, "PAYLOAD")).collect();
    assert!(payloads.contains(&Some(
        "{\"kind\":\"signup\",\"plan\":{\"name\":\"pro\",\"seats\":3}}".to_string()
    )));
    assert!(payloads.contains(&None));
    let checksums: Vec<Option<String>> = (0..3).map(|row| cell(&events, row, "CHECKSUM")).collect();
    assert!(checksums.contains(&Some("CAFE".to_string())));

    let customers = database
        .rows(&Table::bare("CUSTOMERS"), 10, 0, None)
        .unwrap();
    assert_eq!(
        cell(&customers, 3, "CREATED_AT").as_deref(),
        Some("1969-12-31 23:59:58.500000000")
    );
    assert_eq!(customers.kinds[0][3], Kind::Boolean);
}

#[test]
#[ignore = "needs a Snowflake account seeded by tests/fixtures/snowflake/seed.sh: set TQL_TEST_SNOWFLAKE and TQL_TEST_SNOWFLAKE_TOKEN"]
fn live_filters_on_values_with_quotes_and_backslashes() {
    let mut database = live();
    let customers = Table::bare("CUSTOMERS");

    for name in ["O'Brien", "Back\\slash"] {
        let filters = Filters::new(vec![Filter::new("NAME", "is", name)], Joiner::And);
        let page = database
            .filtered_rows(&customers, 10, 0, None, Some(&filters))
            .unwrap();
        assert_eq!(page.rows.len(), 1, "{name}");
        assert_eq!(cell(&page, 0, "NAME").as_deref(), Some(name));
        assert_eq!(database.count_rows(&customers, Some(&filters)).unwrap(), 1);
    }

    let mixed = database
        .rows(&Table::in_schema("STAGING", "Mixed Case"), 10, 0, None)
        .unwrap();
    assert_eq!(mixed.headers, ["Quoted Column", "lower"]);
}

#[test]
#[ignore = "needs a Snowflake account seeded by tests/fixtures/snowflake/seed.sh: set TQL_TEST_SNOWFLAKE and TQL_TEST_SNOWFLAKE_TOKEN"]
fn live_reads_an_answer_spread_over_many_partitions() {
    let mut database = live();
    let readings = Table::bare("READINGS");

    assert_eq!(database.count_rows(&readings, None).unwrap(), 60_000);
    let outcome = database.run("select ID from READINGS order by ID").unwrap();
    assert_eq!(outcome.rows.len(), 60_000);
    assert_eq!(outcome.rows[59_999][0].as_deref(), Some("60000"));

    let page = database.rows(&readings, 100, 59_950, None).unwrap();
    assert_eq!(page.rows.len(), 50);
    assert_eq!(cell(&page, 0, "ID").as_deref(), Some("59951"));
}

#[test]
#[ignore = "needs a Snowflake account seeded by tests/fixtures/snowflake/seed.sh: set TQL_TEST_SNOWFLAKE and TQL_TEST_SNOWFLAKE_TOKEN"]
fn live_says_what_snowflake_said_and_reads_back_a_write() {
    let mut database = live();

    let failed = database.run("select * from NOPE").err().unwrap();
    assert!(failed.to_string().contains("does not exist"), "{failed}");

    database
        .run("create or replace table STAGING.SCRATCH (N number)")
        .unwrap();
    let inserted = database
        .run("insert into STAGING.SCRATCH values (1), (2)")
        .unwrap();
    assert_eq!(inserted.affected, Some(2));
    let deleted = database.run("delete from STAGING.SCRATCH").unwrap();
    assert_eq!(deleted.affected, Some(2));
    database.run("drop table STAGING.SCRATCH").unwrap();
}
