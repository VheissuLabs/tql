use std::fs;
use std::path::PathBuf;

use app::argv;
use app::console::RunResult;
use app::encrypter::Encrypter;
use app::paths;
use app::tableplus::keychain;
use plist::{Dictionary, Value};

struct Scratch {
    root: PathBuf,
}

impl Scratch {
    fn new(label: &str) -> Self {
        let root =
            std::env::temp_dir().join(format!("tql-tableplus-{}-{label}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let root = fs::canonicalize(root).unwrap();
        paths::use_config_directory(root.join("tql"));
        keychain::fake(&[
            ("prod-1_database", "db-secret"),
            ("jump-1_database", "jump-db-secret"),
            ("jump-1_server", "ssh-secret"),
        ]);
        Scratch { root }
    }

    fn folder(&self, connections: Vec<Dictionary>, groups: Vec<Dictionary>) -> String {
        let folder = self.root.join("Data");
        fs::create_dir_all(&folder).unwrap();
        let list = |items: Vec<Dictionary>| {
            Value::Array(items.into_iter().map(Value::Dictionary).collect())
        };
        list(connections)
            .to_file_xml(folder.join("Connections.plist"))
            .unwrap();
        list(groups)
            .to_file_xml(folder.join("ConnectionGroups.plist"))
            .unwrap();
        folder.display().to_string()
    }

    fn stored(&self, name: &str, column: &str) -> Option<String> {
        let store = rusqlite::Connection::open(paths::store()).unwrap();
        let value: Option<String> = store
            .query_row(
                &format!("select cast(\"{column}\" as text) from connections where name = ?1"),
                [name],
                |row| row.get(0),
            )
            .unwrap();
        match column {
            "password" | "ssh_password" => value.map(|payload| {
                Encrypter::from_file(&paths::key())
                    .unwrap()
                    .decrypt_string(&payload)
                    .unwrap()
            }),
            _ => value,
        }
    }

    fn count(&self) -> i64 {
        rusqlite::Connection::open(paths::store())
            .unwrap()
            .query_row("select count(*) from connections", [], |row| row.get(0))
            .unwrap()
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn dictionary(entries: &[(&str, Value)]) -> Dictionary {
    entries
        .iter()
        .map(|(key, value)| (key.to_string(), value.clone()))
        .collect()
}

fn text(value: &str) -> Value {
    Value::String(value.to_string())
}

fn connection(overrides: &[(&str, Value)]) -> Dictionary {
    let mut connection = dictionary(&[
        ("ID", text("id-unique")),
        ("ConnectionName", text("unnamed")),
        ("Driver", text("MySQL")),
        ("DatabaseHost", text("10.0.0.5")),
        ("DatabasePort", text("3306")),
        ("DatabaseName", text("app")),
        ("DatabaseUser", text("forge")),
        ("DatabasePasswordMode", Value::Integer(0.into())),
        ("ServerPasswordMode", Value::Integer(0.into())),
        ("Enviroment", text("")),
        ("GroupID", text("")),
        ("isOverSSH", Value::Boolean(false)),
        ("isUsePrivateKey", Value::Boolean(false)),
        ("ServerAddress", text("")),
        ("ServerPort", text("")),
        ("ServerUser", text("")),
        ("ServerPrivateKeyName", text("")),
        (
            "TlsKeyPaths",
            Value::Array(vec![text(""), text(""), text("")]),
        ),
        ("tLSMode", Value::Integer(0.into())),
    ]);
    for (key, value) in overrides {
        connection.insert(key.to_string(), value.clone());
    }
    connection
}

fn tql(arguments: &[&str]) -> RunResult {
    app::application().run_with(arguments.iter().copied(), "")
}

#[test]
fn brings_tableplus_connections_over_with_their_groups_tags_and_passwords() {
    let scratch = Scratch::new("over");
    let folder = scratch.folder(
        vec![
            connection(&[
                ("ID", text("prod-1")),
                ("ConnectionName", text("NotaryDash Prod")),
                ("Enviroment", text("production")),
                ("GroupID", text("g-1")),
                ("tLSMode", Value::Integer(2.into())),
            ]),
            connection(&[
                ("ID", text("jump-1")),
                ("ConnectionName", text("Behind SSH")),
                ("DatabaseHost", text("127.0.0.1")),
                ("isOverSSH", Value::Boolean(true)),
                ("ServerAddress", text("203.0.113.7")),
                ("ServerPort", text("2222")),
                ("ServerUser", text("forge")),
            ]),
        ],
        vec![dictionary(&[
            ("ID", text("g-1")),
            ("Name", text("NotaryDash")),
            ("GroupID", text("")),
            ("IsExpaned", Value::Boolean(true)),
        ])],
    );

    let result = tql(&["tableplus", &format!("--from={folder}"), "--table"]);

    assert_eq!(result.code, 0, "{result:?}");
    let prod = |column| scratch.stored("NotaryDash Prod", column);
    assert_eq!(prod("group_name").as_deref(), Some("NotaryDash"));
    assert_eq!(prod("tag").as_deref(), Some("production"));
    assert_eq!(prod("ssl_mode").as_deref(), Some("require"));
    assert_eq!(prod("password").as_deref(), Some("db-secret"));
    let ssh = |column| scratch.stored("Behind SSH", column);
    assert_eq!(ssh("ssh_host").as_deref(), Some("203.0.113.7"));
    assert_eq!(ssh("ssh_port").as_deref(), Some("2222"));
    assert_eq!(ssh("ssh_user").as_deref(), Some("forge"));
    assert_eq!(ssh("ssh_password").as_deref(), Some("ssh-secret"));
    assert_eq!(ssh("password").as_deref(), Some("jump-db-secret"));
}

#[test]
fn touches_nothing_and_asks_the_keychain_nothing_on_a_dry_run() {
    let scratch = Scratch::new("dry-run");
    let folder = scratch.folder(
        vec![connection(&[
            ("ID", text("prod-1")),
            ("ConnectionName", text("NotaryDash Prod")),
        ])],
        Vec::new(),
    );

    let result = tql(&[
        "tableplus",
        &format!("--from={folder}"),
        "--dry-run",
        "--table",
    ]);

    assert_eq!(result.code, 0, "{result:?}");
    assert!(result.stdout.contains("1 to bring over"), "{result:?}");
    assert!(
        result
            .stdout
            .contains("  would  NotaryDash Prod  mysql · password from Keychain, after macOS asks")
    );
    assert_eq!(scratch.count(), 0);
    assert!(keychain::asked().is_empty());
}

#[test]
fn leaves_passwords_in_the_keychain_when_asked_to() {
    let scratch = Scratch::new("without");
    let folder = scratch.folder(
        vec![connection(&[
            ("ID", text("prod-1")),
            ("ConnectionName", text("NotaryDash Prod")),
        ])],
        Vec::new(),
    );

    let result = tql(&[
        "tableplus",
        &format!("--from={folder}"),
        "--without-passwords",
        "--table",
    ]);

    assert_eq!(result.code, 0, "{result:?}");
    assert_eq!(scratch.stored("NotaryDash Prod", "password"), None);
    assert!(keychain::asked().is_empty());
    assert!(result.stdout.contains("password left out; add it with e"));
}

#[test]
fn does_not_look_in_the_keychain_for_a_connection_tableplus_asks_about_every_time() {
    let scratch = Scratch::new("asks");
    let folder = scratch.folder(
        vec![connection(&[
            ("ID", text("prod-1")),
            ("ConnectionName", text("Asks")),
            ("DatabasePasswordMode", Value::Integer(1.into())),
        ])],
        Vec::new(),
    );

    let result = tql(&["tableplus", &format!("--from={folder}"), "--table"]);

    assert_eq!(result.code, 0, "{result:?}");
    assert!(keychain::asked().is_empty());
    assert_eq!(scratch.stored("Asks", "password"), None);
    assert!(
        result
            .stdout
            .contains("TablePlus asks for it each time; add it with e")
    );
}

#[test]
fn skips_what_tql_cannot_open_and_what_it_already_has_so_running_it_twice_is_safe() {
    let scratch = Scratch::new("twice");
    let folder = scratch.folder(
        vec![
            connection(&[
                ("ConnectionName", text("Cache")),
                ("Driver", text("MongoDB")),
            ]),
            connection(&[
                ("ID", text("prod-1")),
                ("ConnectionName", text("NotaryDash Prod")),
            ]),
        ],
        Vec::new(),
    );

    assert_eq!(
        tql(&["tableplus", &format!("--from={folder}"), "--table"]).code,
        0
    );
    let again = tql(&["tableplus", &format!("--from={folder}"), "--table"]);

    assert_eq!(again.code, 0, "{again:?}");
    assert!(
        again.stdout.contains("already in tql as NotaryDash Prod"),
        "{again:?}"
    );
    assert!(again.stdout.contains("tql does not speak MongoDB"));
    assert!(again.stdout.contains("Added 0. Run tql to see them."));
    assert_eq!(scratch.count(), 1);
}

#[test]
fn answers_an_agent_in_json_without_a_password_in_it() {
    let scratch = Scratch::new("json");
    let folder = scratch.folder(
        vec![connection(&[
            ("ID", text("prod-1")),
            ("ConnectionName", text("NotaryDash Prod")),
            ("Enviroment", text("production")),
        ])],
        Vec::new(),
    );

    let result = tql(&["tableplus", &format!("--from={folder}"), "--json"]);

    assert_eq!(result.code, 0, "{result:?}");
    let report: serde_json::Value = serde_json::from_str(&result.stdout).unwrap();
    assert_eq!(report["source"], folder);
    assert_eq!(report["dry_run"], false);
    assert_eq!(report["imported"][0]["name"], "NotaryDash Prod");
    assert_eq!(report["imported"][0]["driver"], "mysql");
    assert_eq!(report["imported"][0]["tag"], "production");
    assert_eq!(report["imported"][0]["over_ssh"], false);
    assert_eq!(report["imported"][0]["password"], "from Keychain");
    assert_eq!(report["skipped"], serde_json::json!([]));
    assert!(!result.stdout.contains("db-secret"));
}

#[test]
fn reads_a_sqlite_connection_from_its_path() {
    let scratch = Scratch::new("sqlite");
    let folder = scratch.folder(
        vec![connection(&[
            ("ConnectionName", text("Local file")),
            ("Driver", text("SQLite")),
            ("DatabasePath", text("/tmp/app.sqlite")),
        ])],
        Vec::new(),
    );

    let result = tql(&["tableplus", &format!("--from={folder}"), "--table"]);

    assert_eq!(result.code, 0, "{result:?}");
    assert_eq!(
        scratch.stored("Local file", "driver").as_deref(),
        Some("sqlite")
    );
    assert_eq!(
        scratch.stored("Local file", "database").as_deref(),
        Some("/tmp/app.sqlite")
    );
    assert!(result.stdout.contains("password not needed"));
}

#[test]
fn gives_a_second_connection_with_a_taken_name_a_number() {
    let scratch = Scratch::new("names");
    let folder = scratch.folder(
        vec![
            connection(&[("ConnectionName", text("App"))]),
            connection(&[
                ("ConnectionName", text("App")),
                ("DatabaseHost", text("10.9.9.9")),
            ]),
        ],
        Vec::new(),
    );

    let result = tql(&[
        "tableplus",
        &format!("--from={folder}"),
        "--without-passwords",
        "--table",
    ]);

    assert_eq!(result.code, 0, "{result:?}");
    assert_eq!(
        scratch.stored("App (2)", "host").as_deref(),
        Some("10.9.9.9")
    );
}

#[test]
fn maps_postgres_tls_ports_and_private_keys() {
    let scratch = Scratch::new("postgres");
    let folder = scratch.folder(
        vec![connection(&[
            ("ConnectionName", text("Keyed")),
            ("Driver", text("PostgreSQL")),
            ("DatabasePort", text("")),
            ("tLSMode", Value::Integer(4.into())),
            (
                "TlsKeyPaths",
                Value::Array(vec![text("/k.pem"), text(""), text("/ca.pem")]),
            ),
            ("Enviroment", text("Testing")),
            ("isOverSSH", Value::Boolean(true)),
            ("isUsePrivateKey", Value::Boolean(true)),
            ("ServerAddress", text("bastion")),
            ("ServerPrivateKeyName", text("~/.ssh/deploy")),
        ])],
        Vec::new(),
    );

    let result = tql(&["tableplus", &format!("--from={folder}"), "--table"]);

    assert_eq!(result.code, 0, "{result:?}");
    let keyed = |column| scratch.stored("Keyed", column);
    assert_eq!(keyed("port").as_deref(), Some("5432"));
    assert_eq!(keyed("ssl_mode").as_deref(), Some("verify-ca"));
    assert_eq!(keyed("ssl_key").as_deref(), Some("/k.pem"));
    assert_eq!(keyed("ssl_cert"), None);
    assert_eq!(keyed("ssl_ca").as_deref(), Some("/ca.pem"));
    assert_eq!(keyed("tag").as_deref(), Some("dev"));
    assert_eq!(keyed("ssh_key").as_deref(), Some("~/.ssh/deploy"));
    assert_eq!(keyed("ssh_password"), None);
    assert_eq!(keychain::asked(), vec!["id-unique_database".to_string()]);
}

#[test]
fn says_where_it_looked_when_the_folder_has_no_connections_plist() {
    let scratch = Scratch::new("missing");
    let empty = scratch.root.join("empty");
    fs::create_dir_all(&empty).unwrap();

    let result = tql(&[
        "tableplus",
        &format!("--from={}", empty.display()),
        "--json",
    ]);

    assert_eq!(result.code, 1);
    assert_eq!(
        result.stdout.trim(),
        format!(
            "{{\"error\":\"{} has no Connections.plist; point --from at the TablePlus Data folder.\"}}",
            empty.display()
        )
    );
}

#[test]
fn is_not_mistaken_for_a_file_to_open() {
    let argv = vec!["tql".to_string(), "tableplus".to_string()];

    assert_eq!(argv::rewrite(argv.clone()), argv);
}
