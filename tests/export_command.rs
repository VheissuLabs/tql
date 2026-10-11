use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard, Once};

static SANDBOX: Once = Once::new();
static ONE_AT_A_TIME: Mutex<()> = Mutex::new(());

fn sandbox_root() -> PathBuf {
    std::env::temp_dir().join(format!("tql-rust-export-command-{}", std::process::id()))
}

struct Sandbox {
    _turn: MutexGuard<'static, ()>,
    folder: PathBuf,
    source: PathBuf,
}

impl Sandbox {
    fn new(name: &str) -> Self {
        SANDBOX.call_once(|| {
            let root = sandbox_root();
            let _ = fs::remove_dir_all(&root);
            fs::create_dir_all(root.join("tql")).unwrap();
            unsafe { std::env::set_var("XDG_CONFIG_HOME", &root) };
        });
        let turn = ONE_AT_A_TIME
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        assert!(
            app::paths::store().starts_with(sandbox_root()),
            "the store must be in the sandbox"
        );

        let folder = sandbox_root().join(name);
        let _ = fs::remove_dir_all(&folder);
        fs::create_dir_all(&folder).unwrap();
        let source = folder.join("things.sqlite");
        rusqlite::Connection::open(&source)
            .unwrap()
            .execute_batch(
                "create table things (id integer primary key, name text);
                 insert into things (name) values ('first'), ('second');
                 create table others (id integer primary key, label text);
                 insert into others (label) values ('one');",
            )
            .unwrap();

        let store = app::paths::store();
        let _ = fs::remove_file(&store);
        fs::remove_dir_all(sandbox_root().join("tql/exports")).ok();
        rusqlite::Connection::open(&store)
            .unwrap()
            .execute_batch(
                "create table connections (id integer primary key, name text, driver text, host text, port integer,
                    database text, username text, password text, read_only integer, tag text, last_used_at text);
                 create table settings (key varchar primary key, value text, created_at datetime, updated_at datetime);",
            )
            .unwrap();

        Sandbox {
            _turn: turn,
            folder,
            source,
        }
    }

    fn with_connections(self, rows: &[(&str, &str, &str)]) -> Self {
        let store = rusqlite::Connection::open(app::paths::store()).unwrap();
        for (name, driver, database) in rows {
            let database = if *database == "SOURCE" {
                self.source.display().to_string()
            } else {
                database.to_string()
            };
            store
                .execute(
                    "insert into connections (name, driver, host, port, database, username) values (?1, ?2, '127.0.0.1', 1, ?3, 'root')",
                    (name, driver, database),
                )
                .unwrap();
        }
        self
    }

    fn with_things(self) -> Self {
        self.with_connections(&[("things", "sqlite", "SOURCE")])
    }

    fn out(&self, name: &str) -> PathBuf {
        self.folder.join(name)
    }
}

fn run(arguments: &[&str], stdin: &str) -> app::console::RunResult {
    app::application().run_with(arguments.iter().copied(), stdin)
}

fn read(path: &Path) -> String {
    fs::read_to_string(path).unwrap()
}

#[test]
fn exports_a_named_table_to_a_named_file() {
    let sandbox = Sandbox::new("named").with_things();
    let out = sandbox.out("things.sql");

    let result = run(
        &[
            "export",
            "things",
            "things",
            &format!("--sql={}", out.display()),
        ],
        "",
    );

    assert_eq!(result.code, 0, "{}{}", result.stdout, result.stderr);
    assert!(read(&out).contains(
        "insert into \"things\" (\"id\", \"name\") values\n(1, 'first'),\n(2, 'second');"
    ));
    assert!(
        result.stdout.contains("  2 rows from things ("),
        "{}",
        result.stdout
    );
    assert!(result.stdout.contains(&out.display().to_string()));
}

#[test]
fn exports_every_table_when_none_is_named() {
    let sandbox = Sandbox::new("every").with_things();
    let out = sandbox.out("all.sql");

    let result = run(
        &["export", "things", &format!("--sql={}", out.display())],
        "",
    );

    assert_eq!(result.code, 0, "{}", result.stdout);
    let sql = read(&out);
    assert!(sql.contains("-- table: others, things\n"), "{sql}");
    assert!(sql.contains("-- others\n") && sql.contains("-- things\n"));
    assert!(result.stdout.contains("  3 rows from 2 tables ("));
}

#[test]
fn names_the_file_itself_without_sql() {
    let _sandbox = Sandbox::new("auto-named").with_things();

    let result = run(&["export", "things", "things"], "");

    assert_eq!(result.code, 0, "{}", result.stdout);
    let exports = sandbox_root().join("tql/exports");
    let written: Vec<_> = fs::read_dir(&exports).unwrap().collect();
    assert_eq!(written.len(), 1);
    let name = written[0].as_ref().unwrap().file_name();
    assert!(name.to_string_lossy().starts_with("things-things-20"));
}

#[test]
fn honours_a_row_limit() {
    let sandbox = Sandbox::new("limit").with_things();
    let out = sandbox.out("limited.sql");

    let result = run(
        &[
            "export",
            "things",
            "things",
            "--limit=1",
            &format!("--sql={}", out.display()),
        ],
        "",
    );

    assert_eq!(result.code, 0);
    assert!(result.stdout.contains("  1 rows from things"));
}

#[test]
fn lists_the_tables() {
    let _sandbox = Sandbox::new("list").with_things();

    let result = run(&["export", "things", "--list"], "");

    assert_eq!(result.code, 0);
    assert_eq!(result.stdout, "  others\n  things\n");
}

#[test]
fn fails_clearly_for_an_unknown_connection() {
    let _sandbox = Sandbox::new("unknown-connection").with_things();

    let result = run(&["export", "definitely-not-a-connection"], "");

    assert_eq!(result.code, 1);
    assert!(
        result
            .stdout
            .contains("No connection named [definitely-not-a-connection].")
    );
    assert!(result.stdout.contains("  known: things"));
}

#[test]
fn fails_clearly_for_an_unknown_table() {
    let _sandbox = Sandbox::new("unknown-table").with_things();

    let result = run(&["export", "things", "nope"], "");

    assert_eq!(result.code, 1);
    assert!(result.stdout.contains("No table named [nope] in [things]."));
    assert!(result.stdout.contains("  known: others, things"));
}

#[test]
fn asks_which_connection_table_and_file_when_given_nothing() {
    let sandbox = Sandbox::new("asks").with_things();
    let out = sandbox.out("asked.sql");

    let result = run(&["export"], &format!("0\nthings\n{}\n", out.display()));

    assert_eq!(result.code, 0, "{}", result.stdout);
    assert!(result.stdout.contains("Export from"));
    assert!(result.stdout.contains("every table (2)"));
    assert!(result.stdout.contains("Save it where?"));
    assert!(read(&out).contains("insert into \"things\""));
}

#[test]
fn takes_every_table_when_the_first_option_is_chosen() {
    let sandbox = Sandbox::new("asks-every").with_things();
    let out = sandbox.out("asked-every.sql");

    let result = run(&["export"], &format!("0\n0\n{}\n", out.display()));

    assert_eq!(result.code, 0, "{}", result.stdout);
    assert!(read(&out).contains("-- table: others, things\n"));
}

#[test]
fn names_the_file_itself_when_the_answer_is_a_folder() {
    let sandbox = Sandbox::new("into-folder").with_things();
    let folder = sandbox.out("into");
    fs::create_dir_all(&folder).unwrap();

    let result = run(&["export"], &format!("0\nthings\n{}\n", folder.display()));

    assert_eq!(result.code, 0, "{}", result.stdout);
    let written: Vec<_> = fs::read_dir(&folder)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .collect();
    assert_eq!(written.len(), 1);
    assert!(read(&written[0]).contains("insert into \"things\""));
}

#[test]
fn asks_again_when_the_folder_is_not_there() {
    let sandbox = Sandbox::new("no-folder").with_things();
    let out = sandbox.out("second-try.sql");

    let result = run(
        &["export"],
        &format!(
            "0\nthings\n{}\n{}\n",
            sandbox.out("missing/file.sql").display(),
            out.display()
        ),
    );

    assert_eq!(result.code, 0, "{}", result.stdout);
    assert!(result.stdout.contains("There is no folder ["));
    assert!(out.is_file());
}

#[test]
fn does_not_ask_where_when_sql_says_so() {
    let sandbox = Sandbox::new("told").with_things();
    let out = sandbox.out("told.sql");

    let result = run(
        &["export", &format!("--sql={}", out.display())],
        "0\nthings\n",
    );

    assert_eq!(result.code, 0, "{}", result.stdout);
    assert!(!result.stdout.contains("Save it where?"));
    assert!(out.is_file());
}

#[test]
fn says_which_connections_there_are_when_it_cannot_ask() {
    let _sandbox = Sandbox::new("cannot-ask").with_things();

    let result = run(&["export", "--no-interaction"], "");

    assert_eq!(result.code, 1);
    assert!(result.stdout.contains("Which connection?"));
    assert!(result.stdout.contains("  known: things"));
}

#[test]
fn has_nothing_to_export_before_a_connection_is_saved() {
    let _sandbox = Sandbox::new("no-connections");

    let result = run(&["export", "--no-interaction"], "");

    assert_eq!(result.code, 1);
    assert!(result.stdout.contains("No saved connections"));
}

#[test]
fn will_not_export_from_a_server_without_a_database() {
    let _sandbox = Sandbox::new("server").with_connections(&[("server", "mysql", "")]);

    let result = run(&["export", "server", "--no-interaction"], "");

    assert_eq!(result.code, 1);
    assert!(result.stdout.contains("is a server, not a database"));
}

#[test]
fn has_no_database_to_choose_on_sqlite() {
    let _sandbox = Sandbox::new("sqlite-database").with_things();

    let result = run(
        &["export", "things", "--database=nope", "--no-interaction"],
        "",
    );

    assert_eq!(result.code, 1);
    assert!(result.stdout.contains("one file"));
}
