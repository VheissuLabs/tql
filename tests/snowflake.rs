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
                let body = match reply["raw"].as_str() {
                    Some(raw) => raw.to_string(),
                    None => reply.to_string(),
                };
                let _ = write!(
                    stream,
                    "HTTP/1.1 {status} OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
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

#[test]
fn without_a_token_it_says_what_is_missing_before_reaching_snowflake() {
    let server = Server::start(warehouse);

    let failed = database::connect(
        &Connection {
            driver: Driver::Snowflake,
            host: Some(server.url.clone()),
            ..Connection::default()
        },
        None,
    )
    .err()
    .unwrap();

    assert!(failed.to_string().contains("programmatic access token"));
    assert!(server.statements().is_empty());
}

struct Keys {
    directory: std::path::PathBuf,
    public: Vec<u8>,
    fingerprint: String,
}

fn keys() -> &'static Keys {
    static KEYS: std::sync::OnceLock<Keys> = std::sync::OnceLock::new();
    KEYS.get_or_init(|| {
        use base64::Engine;
        use base64::engine::general_purpose::STANDARD;
        use rsa::pkcs1::{EncodeRsaPrivateKey, EncodeRsaPublicKey};
        use rsa::pkcs8::{EncodePrivateKey, EncodePublicKey, LineEnding};
        use sha2::{Digest, Sha256};

        let mut random = rand_core::OsRng;
        let key = rsa::RsaPrivateKey::new(&mut random, 2048).unwrap();
        let directory =
            std::env::temp_dir().join(format!("tql-snowflake-keys-{}", std::process::id()));
        std::fs::create_dir_all(&directory).unwrap();
        for (name, pem) in [
            ("rsa_key.p8", key.to_pkcs8_pem(LineEnding::LF).unwrap()),
            (
                "rsa_key_encrypted.p8",
                key.to_pkcs8_encrypted_pem(&mut random, "correct-horse", LineEnding::LF)
                    .unwrap(),
            ),
            (
                "rsa_key_pkcs1.pem",
                key.to_pkcs1_pem(LineEnding::LF).unwrap(),
            ),
        ] {
            std::fs::write(directory.join(name), pem.as_bytes()).unwrap();
        }
        let public = key.to_public_key();
        Keys {
            directory,
            public: public.to_pkcs1_der().unwrap().as_bytes().to_vec(),
            fingerprint: format!(
                "SHA256:{}",
                STANDARD.encode(Sha256::digest(
                    public.to_public_key_der().unwrap().as_bytes()
                ))
            ),
        }
    })
}

fn keyed(
    server: &Server,
    key: &str,
    passphrase: Option<&str>,
) -> anyhow::Result<Box<dyn Database>> {
    let encrypter = Encrypter::throwaway().unwrap();
    database::connect(
        &Connection {
            name: "snowflake".into(),
            driver: Driver::Snowflake,
            host: Some(server.url.clone()),
            database: Some("ANALYTICS".into()),
            username: Some("karl".into()),
            password: passphrase.map(|passphrase| encrypter.encrypt_string(passphrase).unwrap()),
            authenticator: Some("snowflake_jwt".into()),
            private_key: Some(keys().directory.join(key).display().to_string()),
            warehouse: Some("COMPUTE_WH".into()),
            role: Some("ANALYST".into()),
            ..Connection::default()
        },
        Some(&encrypter),
    )
}

fn verified_claims(jwt: &str) -> Value {
    use base64::Engine;
    use base64::engine::general_purpose::URL_SAFE_NO_PAD;
    use ring::signature::{RSA_PKCS1_2048_8192_SHA256, UnparsedPublicKey};

    let (signed, signature) = jwt.rsplit_once('.').unwrap();
    UnparsedPublicKey::new(&RSA_PKCS1_2048_8192_SHA256, &keys().public)
        .verify(
            signed.as_bytes(),
            &URL_SAFE_NO_PAD.decode(signature).unwrap(),
        )
        .unwrap();
    let claims = signed.split('.').nth(1).unwrap();
    serde_json::from_slice(&URL_SAFE_NO_PAD.decode(claims).unwrap()).unwrap()
}

#[test]
fn signs_in_with_a_key_pair_by_sending_a_jwt_signed_with_the_private_key() {
    let server = Server::start(warehouse);
    keyed(&server, "rsa_key.p8", None).unwrap();

    let requests = server.requests.lock().unwrap();
    let first = &requests[0];
    assert_eq!(
        first.headers["x-snowflake-authorization-token-type"],
        "KEYPAIR_JWT"
    );
    let jwt = first.headers["authorization"]
        .strip_prefix("Bearer ")
        .unwrap();
    let claims = verified_claims(jwt);
    let fingerprint = &keys().fingerprint;
    assert_eq!(claims["sub"], "127.KARL");
    assert_eq!(claims["iss"], format!("127.KARL.{}", fingerprint.trim()));
    assert_eq!(
        claims["exp"].as_u64().unwrap() - claims["iat"].as_u64().unwrap(),
        59 * 60
    );
    assert!(
        requests
            .iter()
            .all(|request| request.headers["authorization"] == first.headers["authorization"])
    );
}

#[test]
fn reads_an_encrypted_key_with_its_passphrase_and_a_pkcs1_key_as_the_same_key() {
    let server = Server::start(warehouse);

    keyed(&server, "rsa_key_encrypted.p8", Some("correct-horse")).unwrap();
    keyed(&server, "rsa_key_pkcs1.pem", None).unwrap();
    let wrong = format!(
        "{:#}",
        keyed(&server, "rsa_key_encrypted.p8", Some("wrong"))
            .err()
            .unwrap()
    );
    let missing = format!(
        "{:#}",
        keyed(&server, "rsa_key_encrypted.p8", None).err().unwrap()
    );

    assert!(
        wrong.contains("the passphrase does not decrypt it"),
        "{wrong}"
    );
    assert!(
        missing.contains("save its passphrase as the connection's password"),
        "{missing}"
    );
    let fingerprint = &keys().fingerprint;
    for request in server.requests.lock().unwrap().iter() {
        let jwt = request.headers["authorization"]
            .strip_prefix("Bearer ")
            .unwrap();
        assert_eq!(
            verified_claims(jwt)["iss"],
            format!("127.KARL.{}", fingerprint.trim())
        );
    }
}

#[test]
fn key_pair_and_browser_sign_in_say_what_is_missing_before_reaching_snowflake() {
    let server = Server::start(warehouse);
    let attempt = |authenticator: &str, username: Option<&str>| {
        let failed = database::connect(
            &Connection {
                driver: Driver::Snowflake,
                host: Some(server.url.clone()),
                username: username.map(String::from),
                authenticator: Some(authenticator.into()),
                ..Connection::default()
            },
            None,
        )
        .err()
        .unwrap();
        failed.to_string()
    };

    assert!(attempt("snowflake_jwt", None).contains("needs the user's login name"));
    assert!(attempt("snowflake_jwt", Some("karl")).contains("path to the user's private key"));
    assert!(attempt("externalbrowser", None).contains("needs the user's login name"));
    assert!(server.requests.lock().unwrap().is_empty());
}

fn browser(address: &str) -> anyhow::Result<()> {
    let port = address.split("port=").nth(1).unwrap().to_string();
    std::thread::spawn(move || {
        for target in ["/favicon.ico", "/?token=idp%2Btoken"] {
            let mut stream = std::net::TcpStream::connect(format!("127.0.0.1:{port}")).unwrap();
            write!(stream, "GET {target} HTTP/1.1\r\nhost: localhost\r\n\r\n").unwrap();
            let mut reply = String::new();
            let _ = stream.read_to_string(&mut reply);
        }
    });
    Ok(())
}

fn signing_in(request: &Request) -> Option<(u16, Value)> {
    match request.path.split('?').next().unwrap_or_default() {
        "/session/authenticator-request" => Some((
            200,
            json!({"success": true, "data": {
                "ssoUrl": format!(
                    "https://idp.example/sso?port={}",
                    request.body["data"]["BROWSER_MODE_REDIRECT_PORT"].as_str().unwrap()
                ),
                "proofKey": "proof-key",
            }}),
        )),
        "/session/v1/login-request" => {
            let data = &request.body["data"];
            let accepted = data["TOKEN"] == "idp+token"
                && data["PROOF_KEY"] == "proof-key"
                && data["AUTHENTICATOR"] == "EXTERNALBROWSER";
            Some((
                200,
                match accepted {
                    true => json!({"success": true, "data": {
                        "token": "session-1",
                        "masterToken": "master-1",
                        "validityInSeconds": 3600,
                        "masterValidityInSeconds": 14400,
                        "sessionInfo": {
                            "databaseName": "ANALYTICS",
                            "schemaName": "PUBLIC",
                            "warehouseName": "COMPUTE_WH",
                            "roleName": "ANALYST",
                        },
                    }}),
                    false => {
                        json!({"success": false, "code": "390190", "message": "The SAML response is invalid."})
                    }
                },
            ))
        }
        _ => None,
    }
}

fn classic(request: &Request) -> (u16, Value) {
    let statement = request.body["sqlText"].as_str().unwrap_or_default();
    let place = json!({
        "finalDatabaseName": "ANALYTICS",
        "finalSchemaName": "PUBLIC",
        "finalWarehouseName": "COMPUTE_WH",
        "finalRoleName": "ANALYST",
    });
    let mut data = match statement.starts_with("use ") {
        true => json!({
            "rowtype": [{"name": "status", "type": "text", "scale": 0}],
            "rowset": [["Statement executed successfully."]],
        }),
        false => {
            let (status, answer) = warehouse(&Request {
                body: json!({ "statement": statement }),
                ..request.clone()
            });
            if status != 200 {
                return (
                    200,
                    json!({"success": false, "code": answer["code"], "message": answer["message"], "data": {}}),
                );
            }
            json!({
                "rowtype": answer["resultSetMetaData"]["rowType"],
                "rowset": answer["data"],
                "statementTypeId": 4096,
            })
        }
    };
    data.as_object_mut()
        .unwrap()
        .extend(place.as_object().unwrap().clone());
    (200, json!({"success": true, "data": data}))
}

fn through_the_browser(server: &Server) -> Connection {
    Connection {
        name: "snowflake".into(),
        driver: Driver::Snowflake,
        host: Some(server.url.clone()),
        database: Some("ANALYTICS".into()),
        username: Some("karl@acme.com".into()),
        authenticator: Some("externalbrowser".into()),
        warehouse: Some("COMPUTE_WH".into()),
        role: Some("ANALYST".into()),
        ..Connection::default()
    }
}

fn paths_of(requests: &[Request], prefix: &str) -> usize {
    requests
        .iter()
        .filter(|request| request.path.starts_with(prefix))
        .count()
}

#[test]
fn signs_in_through_the_browser_and_runs_statements_over_the_session() {
    database::snowflake::open_with(browser);
    let server = Server::start(|request| signing_in(request).unwrap_or_else(|| classic(request)));

    let mut database = database::connect(&through_the_browser(&server), None).unwrap();

    assert_eq!(database.default_schema().as_deref(), Some("PUBLIC"));
    assert!(!database.tables().unwrap().is_empty());
    let requests = server.requests.lock().unwrap();
    let login = requests
        .iter()
        .find(|request| request.path.starts_with("/session/v1/login-request"))
        .unwrap();
    for wanted in [
        "roleName=ANALYST",
        "warehouse=COMPUTE_WH",
        "databaseName=ANALYTICS",
    ] {
        assert!(login.path.contains(wanted), "{wanted} in {}", login.path);
    }
    assert_eq!(login.body["data"]["LOGIN_NAME"], "karl@acme.com");
    assert_eq!(login.body["data"]["CLIENT_APP_ID"], "tql");
    let queries: Vec<&Request> = requests
        .iter()
        .filter(|request| request.path.starts_with("/queries/v1/query-request"))
        .collect();
    assert!(!queries.is_empty());
    for query in &queries {
        assert_eq!(
            query.headers["authorization"],
            "Snowflake Token=\"session-1\""
        );
        assert!(
            !query.body["sqlText"].as_str().unwrap().starts_with("use "),
            "the session was already in place"
        );
    }
    assert_eq!(paths_of(&requests, "/api/v2"), 0);
}

#[test]
fn a_sign_in_snowflake_refuses_says_why() {
    database::snowflake::open_with(browser);
    let server = Server::start(|request| match request.path.as_str() {
        path if path.starts_with("/session/v1/login-request") => (
            200,
            json!({"success": false, "code": "390190", "message": "The SAML response is invalid."}),
        ),
        _ => signing_in(request).unwrap_or_else(|| classic(request)),
    });

    let failed = database::connect(&through_the_browser(&server), None)
        .err()
        .unwrap();

    assert_eq!(
        failed.to_string(),
        "Snowflake refused the sign-in: The SAML response is invalid."
    );
}

#[test]
fn waits_for_a_long_statement_and_gathers_the_chunks_snowflake_stored_it_in() {
    database::snowflake::open_with(browser);
    let address = Arc::new(std::sync::OnceLock::<String>::new());
    let known = address.clone();
    let server = Server::start(move |request| {
        let url = known.get().cloned().unwrap_or_default();
        match request.path.as_str() {
            "/chunks/0" => (200, json!({"raw": "[\"3\"],\n[\"4\"]"})),
            "/chunks/1" => (200, json!({"raw": "[\"5\"]"})),
            "/queries/qid/result" => (
                200,
                json!({"success": true, "data": {
                    "rowtype": [{"name": "N", "type": "fixed", "scale": 0}],
                    "rowset": [["1"], ["2"]],
                    "chunks": [
                        {"url": format!("{url}/chunks/0"), "rowCount": 2},
                        {"url": format!("{url}/chunks/1"), "rowCount": 1},
                    ],
                    "chunkHeaders": {"x-amz-server-side-encryption-customer-key": "chunk-key"},
                }}),
            ),
            _ if request.body["sqlText"] == "select N from BIG" => (
                200,
                json!({"success": true, "code": "333334", "data": {"queryId": "qid", "getResultUrl": "/queries/qid/result"}}),
            ),
            _ => signing_in(request).unwrap_or_else(|| classic(request)),
        }
    });
    address.set(server.url.clone()).unwrap();
    let mut database = database::connect(&through_the_browser(&server), None).unwrap();

    let outcome = database.run("select N from BIG").unwrap();

    assert_eq!(
        outcome.rows,
        [["1"], ["2"], ["3"], ["4"], ["5"]].map(|row| row.map(|value| Some(value.to_string())))
    );
    assert_eq!(outcome.kinds[4], [Kind::Integer]);
    let requests = server.requests.lock().unwrap();
    let poll = requests
        .iter()
        .find(|request| request.path == "/queries/qid/result")
        .unwrap();
    assert_eq!(poll.method, "GET");
    assert_eq!(
        poll.headers["authorization"],
        "Snowflake Token=\"session-1\""
    );
    for chunk in requests
        .iter()
        .filter(|request| request.path.starts_with("/chunks/"))
    {
        assert_eq!(
            chunk.headers["x-amz-server-side-encryption-customer-key"],
            "chunk-key"
        );
        assert!(!chunk.headers.contains_key("authorization"));
    }
}

#[test]
fn renews_an_expired_session_with_the_master_token_and_keeps_it_for_the_next_run() {
    use app::paths;
    use app::store::Store;
    use app::store_writer::{Field, StoreWriter};

    database::snowflake::open_with(browser);
    let directory = std::env::temp_dir().join(format!("tql-snowflake-sso-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&directory);
    paths::use_config_directory(directory.clone());
    let server = Server::start(|request| {
        let session = request
            .headers
            .get("authorization")
            .cloned()
            .unwrap_or_default();
        match request.path.split('?').next().unwrap_or_default() {
            "/session/token-request" => {
                let renewed = session == "Snowflake Token=\"master-1\""
                    && request.body["oldSessionToken"] == "session-1"
                    && request.body["requestType"] == "RENEW";
                (
                    200,
                    match renewed {
                        true => json!({"success": true, "data": {
                            "sessionToken": "session-2",
                            "validityInSecondsST": 3600,
                            "masterToken": "master-1",
                            "validityInSecondsMT": 14000,
                        }}),
                        false => {
                            json!({"success": false, "code": "390114", "message": "Authentication token has expired."})
                        }
                    },
                )
            }
            "/queries/v1/query-request" if session == "Snowflake Token=\"session-1\"" => (
                200,
                json!({"success": false, "code": "390112", "message": "Your session has expired. Please login again.", "data": {}}),
            ),
            _ => signing_in(request).unwrap_or_else(|| classic(request)),
        }
    });
    StoreWriter::prepare(&paths::store()).unwrap();
    Encrypter::ensure(&paths::key()).unwrap();
    let saved = through_the_browser(&server);
    let text = |value: &Option<String>| Field::Text(value.clone().unwrap());
    let id = StoreWriter::open(&paths::store(), &paths::key())
        .unwrap()
        .create(&[
            ("name", Field::Text(saved.name.clone())),
            ("driver", Field::Text("snowflake".into())),
            ("host", text(&saved.host)),
            ("database", text(&saved.database)),
            ("username", text(&saved.username)),
            ("authenticator", text(&saved.authenticator)),
            ("warehouse", text(&saved.warehouse)),
            ("role", text(&saved.role)),
        ])
        .unwrap();
    let stored = || {
        Store::open(&paths::store())
            .unwrap()
            .connection(id)
            .unwrap()
            .unwrap()
    };
    let encrypter = Encrypter::from_file(&paths::key()).unwrap();

    database::connect(&stored(), Some(&encrypter)).unwrap();
    let cached = encrypter
        .decrypt_string(stored().session.as_ref().unwrap())
        .unwrap();
    let mut reopened = database::connect(&stored(), Some(&encrypter)).unwrap();
    reopened.run("select current_schema()").unwrap();

    let requests = server.requests.lock().unwrap();
    assert!(cached.contains("\"session-2\""), "{cached}");
    assert_eq!(paths_of(&requests, "/session/authenticator-request"), 1);
    assert_eq!(paths_of(&requests, "/session/token-request"), 1);
    let sent: Vec<&str> = requests
        .iter()
        .filter(|request| {
            request.headers.get("authorization").map(String::as_str)
                == Some("Snowflake Token=\"session-2\"")
        })
        .filter_map(|request| request.body["sqlText"].as_str())
        .collect();
    assert!(sent.contains(&"use role ANALYST"), "{sent:?}");
    assert!(
        !sent
            .iter()
            .any(|statement| statement.starts_with("use warehouse")
                || statement.starts_with("use database")),
        "Snowflake said the warehouse and database were already in place: {sent:?}"
    );
    drop(requests);
    let _ = std::fs::remove_dir_all(&directory);
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

fn variable(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|value| !value.is_empty())
}

#[test]
#[ignore = "needs a Snowflake user with an RSA public key: set TQL_TEST_SNOWFLAKE, TQL_TEST_SNOWFLAKE_USER and TQL_TEST_SNOWFLAKE_KEY"]
fn live_signs_in_with_a_key_pair() {
    const NEEDS: &str =
        "set TQL_TEST_SNOWFLAKE, TQL_TEST_SNOWFLAKE_USER and TQL_TEST_SNOWFLAKE_KEY";
    let encrypter = Encrypter::throwaway().unwrap();
    let mut database = database::connect(
        &Connection {
            name: "snowflake".into(),
            driver: Driver::Snowflake,
            host: Some(variable("TQL_TEST_SNOWFLAKE").expect(NEEDS)),
            username: Some(variable("TQL_TEST_SNOWFLAKE_USER").expect(NEEDS)),
            authenticator: Some("snowflake_jwt".into()),
            private_key: Some(variable("TQL_TEST_SNOWFLAKE_KEY").expect(NEEDS)),
            password: variable("TQL_TEST_SNOWFLAKE_KEY_PASSPHRASE")
                .map(|passphrase| encrypter.encrypt_string(&passphrase).unwrap()),
            warehouse: variable("TQL_TEST_SNOWFLAKE_WAREHOUSE"),
            role: variable("TQL_TEST_SNOWFLAKE_ROLE"),
            ..Connection::default()
        },
        Some(&encrypter),
    )
    .unwrap();

    let outcome = database.run("select current_user()").unwrap();
    assert!(
        outcome.rows[0][0]
            .as_deref()
            .is_some_and(|user| !user.is_empty())
    );
    assert!(!database.databases().unwrap().is_empty());
}

#[test]
#[ignore = "opens a browser to sign in through the account's identity provider: set TQL_TEST_SNOWFLAKE and TQL_TEST_SNOWFLAKE_SSO_USER"]
fn live_signs_in_through_the_browser_and_reads_an_answer_stored_in_chunks() {
    const NEEDS: &str = "set TQL_TEST_SNOWFLAKE and TQL_TEST_SNOWFLAKE_SSO_USER";
    let mut database = database::connect(
        &Connection {
            name: "snowflake".into(),
            driver: Driver::Snowflake,
            host: Some(variable("TQL_TEST_SNOWFLAKE").expect(NEEDS)),
            username: Some(variable("TQL_TEST_SNOWFLAKE_SSO_USER").expect(NEEDS)),
            authenticator: Some("externalbrowser".into()),
            warehouse: Some(
                variable("TQL_TEST_SNOWFLAKE_WAREHOUSE").unwrap_or("COMPUTE_WH".into()),
            ),
            role: variable("TQL_TEST_SNOWFLAKE_ROLE"),
            ..Connection::default()
        },
        None,
    )
    .unwrap();

    let outcome = database
        .run("select seq4() as N, randstr(40, random()) as FILLER from table(generator(rowcount => 300000)) order by N")
        .unwrap();
    assert_eq!(outcome.rows.len(), 300_000);
    assert_eq!(outcome.rows[299_999][0].as_deref(), Some("299999"));
    assert_eq!(outcome.kinds[0][0], Kind::Integer);

    let failed = database.run("select * from NOPE_NOT_HERE").err().unwrap();
    assert!(failed.to_string().contains("does not exist"), "{failed}");

    let databases = database.databases().unwrap();
    database.use_database(&databases[0]).unwrap();
    database.tables().unwrap();
}
