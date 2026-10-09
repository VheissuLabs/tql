use std::fs;
use std::path::{Path, PathBuf};

use app::console::RunResult;
use app::paths;
use app::store_writer::{Field, StoreWriter};

struct Scratch {
    root: PathBuf,
}

impl Scratch {
    fn new(label: &str) -> Self {
        let root = std::env::temp_dir().join(format!("tql-dump-{}-{label}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        paths::use_config_directory(root.join("tql"));
        StoreWriter::prepare(&paths::store()).unwrap();
        Scratch { root }
    }

    fn path(&self, name: &str) -> PathBuf {
        self.root.join(name)
    }

    fn widgets(&self, name: &str, names: &[&str]) -> PathBuf {
        let path = self.path(&format!("{name}.sqlite"));
        let database = rusqlite::Connection::open(&path).unwrap();
        database
            .execute_batch("create table widgets (id integer primary key, name text)")
            .unwrap();
        for name in names {
            database
                .execute("insert into widgets (name) values (?1)", [name])
                .unwrap();
        }
        path
    }

    fn sqlite(&self, name: &str, path: &Path, extra: &[(&'static str, Field)]) {
        let mut fields = vec![
            ("name", Field::Text(name.into())),
            ("driver", Field::Text("sqlite".into())),
            ("database", Field::Text(path.display().to_string())),
        ];
        fields.extend(extra.iter().cloned());
        StoreWriter::open(&paths::store(), &paths::key())
            .unwrap()
            .create(&fields)
            .unwrap();
    }

    fn server(&self, name: &str, driver: &str) {
        StoreWriter::open(&paths::store(), &paths::key())
            .unwrap()
            .create(&[
                ("name", Field::Text(name.into())),
                ("driver", Field::Text(driver.into())),
                ("host", Field::Text("db.example.test".into())),
                ("port", Field::Number(3306)),
                ("database", Field::Text("app".into())),
            ])
            .unwrap();
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn tql(arguments: &[&str]) -> RunResult {
    tql_with(arguments, "")
}

fn tql_with(arguments: &[&str], stdin: &str) -> RunResult {
    app::application().run_with(arguments.iter().copied(), stdin)
}

fn names(path: &Path) -> Vec<String> {
    let database = rusqlite::Connection::open(path).unwrap();
    let mut statement = database
        .prepare("select name from widgets order by id")
        .unwrap();
    statement
        .query_map([], |row| row.get(0))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap()
}

fn text(path: &Path) -> String {
    path.display().to_string()
}

#[test]
fn dumps_a_sqlite_database_into_a_folder_that_loads_into_another_connection() {
    let scratch = Scratch::new("round-trip");
    scratch.sqlite(
        "source",
        &scratch.widgets("source", &["alpha", "beta"]),
        &[],
    );
    let target = scratch.path("target.sqlite");
    scratch.sqlite("target", &target, &[]);
    let directory = scratch.path("out");

    let dumped = tql(&["dump", "source", &format!("--to={}", text(&directory))]);
    assert_eq!(dumped.code, 0, "{dumped:?}");
    assert!(
        dumped.stdout.contains("  Dumping source (about "),
        "{dumped:?}"
    );
    assert!(dumped.stdout.contains("  Dumped "), "{dumped:?}");
    assert!(
        dumped.stdout.contains("  load it with tql load "),
        "{dumped:?}"
    );

    let manifest = app::dump::read_manifest(&directory).unwrap();
    assert_eq!(manifest["engine"], "sqlite");
    assert_eq!(manifest["connection"], "source");
    assert_eq!(manifest["tool"], "sqlite backup");
    assert_eq!(manifest["database"], "source.sqlite");
    assert_eq!(manifest["tables"], serde_json::Value::Null);
    assert_eq!(manifest["data_only"], false);

    let loaded = tql(&["load", &text(&directory), "target"]);
    assert_eq!(loaded.code, 0, "{loaded:?}");
    assert!(
        loaded
            .stdout
            .contains("  Loading source.sqlite from source into target\n"),
        "{loaded:?}"
    );
    assert_eq!(names(&target), ["alpha", "beta"]);
}

#[test]
fn will_not_load_over_a_database_that_is_already_there_without_drop() {
    let scratch = Scratch::new("no-drop");
    scratch.sqlite("source", &scratch.widgets("source", &["fresh"]), &[]);
    let existing = scratch.widgets("existing", &["kept"]);
    scratch.sqlite("target", &existing, &[]);
    let directory = text(&scratch.path("out"));

    assert_eq!(
        tql(&["dump", "source", &format!("--to={directory}")]).code,
        0
    );
    let refused = tql(&["load", &directory, "target"]);

    assert_eq!(refused.code, 1);
    assert!(
        refused.stdout.contains(&format!(
            "target already has a database at {}; --drop replaces it.",
            text(&existing)
        )),
        "{refused:?}"
    );
    assert_eq!(names(&existing), ["kept"]);

    let replaced = tql(&["load", &directory, "target", "--drop"]);
    assert_eq!(replaced.code, 0, "{replaced:?}");
    assert!(
        replaced
            .stdout
            .contains(", replacing tables it already has")
    );
    assert_eq!(names(&existing), ["fresh"]);
}

#[test]
fn refuses_to_dump_into_a_folder_that_already_has_something_in_it() {
    let scratch = Scratch::new("occupied");
    scratch.sqlite("source", &scratch.widgets("source", &["a"]), &[]);
    let directory = scratch.path("out");
    fs::create_dir_all(&directory).unwrap();
    fs::write(directory.join("something"), "").unwrap();

    let refused = tql(&["dump", "source", &format!("--to={}", text(&directory))]);

    assert_eq!(refused.code, 1);
    assert!(
        refused
            .stdout
            .contains("already has something in it; dump into a new or empty folder.")
    );
}

#[test]
fn will_not_load_a_dump_from_one_engine_into_another() {
    let scratch = Scratch::new("engines");
    scratch.sqlite("source", &scratch.widgets("source", &["a"]), &[]);
    scratch.server("server", "mysql");
    let directory = text(&scratch.path("out"));

    assert_eq!(
        tql(&["dump", "source", &format!("--to={directory}")]).code,
        0
    );
    let refused = tql(&["load", &directory, "server"]);

    assert_eq!(refused.code, 1);
    assert!(
        refused.stdout.contains(
            "That dump is from SQLite and server is MySQL; tql does not move between engines yet."
        ),
        "{refused:?}"
    );
}

#[test]
fn will_not_load_into_a_read_only_connection() {
    let scratch = Scratch::new("read-only");
    scratch.sqlite("source", &scratch.widgets("source", &["a"]), &[]);
    scratch.sqlite(
        "locked",
        &scratch.widgets("locked", &["b"]),
        &[("read_only", Field::Flag(true))],
    );
    let directory = text(&scratch.path("out"));

    assert_eq!(
        tql(&["dump", "source", &format!("--to={directory}")]).code,
        0
    );
    let refused = tql(&["load", &directory, "locked", "--drop"]);

    assert_eq!(refused.code, 1);
    assert!(
        refused
            .stdout
            .contains("locked is read only, so nothing can be loaded into it.")
    );
}

#[test]
fn will_not_load_into_production_unless_it_is_confirmed() {
    let scratch = Scratch::new("production");
    scratch.sqlite("source", &scratch.widgets("source", &["fresh"]), &[]);
    let production = scratch.widgets("prod", &["live"]);
    scratch.sqlite(
        "prod",
        &production,
        &[("tag", Field::Text("production".into()))],
    );
    let directory = text(&scratch.path("out"));

    assert_eq!(
        tql(&["dump", "source", &format!("--to={directory}")]).code,
        0
    );

    let unasked = tql_with(&["load", &directory, "prod", "--drop"], "yes\n");
    assert_eq!(unasked.code, 1, "{unasked:?}");
    assert_eq!(unasked.stdout, "");

    let scripted = tql(&["load", &directory, "prod", "--drop", "-n"]);
    assert_eq!(scripted.code, 1);
    assert!(
        scripted
            .stdout
            .contains("prod is tagged production; --force loads into it anyway.")
    );
    assert_eq!(names(&production), ["live"]);

    let forced = tql(&["load", &directory, "prod", "--drop", "--force"]);
    assert_eq!(forced.code, 0, "{forced:?}");
    assert_eq!(names(&production), ["fresh"]);
}

#[test]
fn replays_a_tql_export_into_tables_that_exist_all_or_nothing() {
    let scratch = Scratch::new("replay");
    scratch.sqlite(
        "source",
        &scratch.widgets("source", &["it's; tricky", "plain"]),
        &[],
    );
    let target = scratch.widgets("target", &[]);
    scratch.sqlite("target", &target, &[]);
    let file = text(&scratch.path("widgets.sql"));

    let exported = tql(&["export", "source", "widgets", &format!("--sql={file}")]);
    assert_eq!(exported.code, 0, "{exported:?}");

    let imported = tql(&["import", &file, "target"]);
    assert_eq!(imported.code, 0, "{imported:?}");
    assert!(imported.stdout.contains(", in one transaction"));
    assert!(
        imported.stdout.contains("  Loaded 1 statements in "),
        "{imported:?}"
    );
    assert_eq!(names(&target), ["it's; tricky", "plain"]);

    let again = tql(&["load", &file, "target"]);
    assert_eq!(again.code, 1);
    assert!(
        again.stdout.contains(
            "Statement 1 failed, so nothing was loaded: UNIQUE constraint failed: widgets.id"
        ),
        "{again:?}"
    );
    assert_eq!(names(&target), ["it's; tricky", "plain"]);
}

#[test]
fn will_not_drop_tables_for_an_exported_file() {
    let scratch = Scratch::new("replay-drop");
    let target = scratch.widgets("target", &[]);
    scratch.sqlite("target", &target, &[]);
    let file = scratch.path("rows.sql");
    fs::write(&file, "insert into widgets (name) values ('x');").unwrap();

    let refused = tql(&["load", &text(&file), "target", "--drop"]);

    assert_eq!(refused.code, 1);
    assert!(refused.stdout.contains(
        "--drop is for dump folders; a .sql file from tql export goes into tables that exist."
    ));
}

#[test]
fn says_how_to_install_the_tool_when_it_is_missing() {
    let scratch = Scratch::new("missing-tool");
    scratch.server("server", "mysql");
    let path = std::env::var_os("PATH");

    unsafe { std::env::set_var("PATH", "/nonexistent") };
    let refused = tql(&["dump", "server"]);
    unsafe { std::env::set_var("PATH", path.unwrap_or_default()) };

    assert_eq!(refused.code, 1);
    assert!(
        refused
            .stdout
            .contains("mydumper is not installed, and tql uses it for MySQL.")
    );
    assert!(
        refused.stdout.contains("  brew install mydumper")
            || refused.stdout.contains("  install mydumper")
    );
}

#[test]
fn names_the_engines_it_does_not_dump() {
    let scratch = Scratch::new("unsupported");
    scratch.server("mssql", "sqlsrv");

    let refused = tql(&["dump", "mssql"]);

    assert_eq!(refused.code, 1);
    assert!(refused.stdout.contains(
        "SQL Server is not supported yet; tql dumps and loads MySQL, Postgres and SQLite."
    ));
}

#[test]
fn asks_for_a_database_on_a_server_connection_that_names_none() {
    let _scratch = Scratch::new("server-only");
    StoreWriter::open(&paths::store(), &paths::key())
        .unwrap()
        .create(&[
            ("name", Field::Text("bare".into())),
            ("driver", Field::Text("mysql".into())),
            ("host", Field::Text("db.example.test".into())),
        ])
        .unwrap();

    let refused = tql(&["dump", "bare"]);

    assert_eq!(refused.code, 1);
    assert!(
        refused
            .stdout
            .contains("[bare] is a server, not a database.")
    );
    assert!(refused.stdout.contains("  name one with --database=."));
}

#[test]
fn names_the_known_connections_when_one_is_missing() {
    let scratch = Scratch::new("unknown");
    scratch.sqlite("zeta", &scratch.widgets("zeta", &[]), &[]);
    scratch.sqlite("alpha", &scratch.widgets("alpha", &[]), &[]);

    let refused = tql(&["dump", "ghost"]);

    assert_eq!(refused.code, 1);
    assert_eq!(
        refused.stdout,
        "No connection named [ghost].\n  known: alpha, zeta\n"
    );
}

#[test]
fn a_sqlite_dump_takes_no_tables_and_shows_what_it_would_do() {
    let scratch = Scratch::new("sqlite-dry");
    let source = scratch.widgets("source", &["a"]);
    scratch.sqlite("source", &source, &[]);
    let directory = text(&scratch.path("out"));

    let refused = tql(&["dump", "source", "widgets", &format!("--to={directory}")]);
    assert_eq!(refused.code, 1);
    assert!(
        refused
            .stdout
            .contains("A SQLite dump is the whole file; it takes no tables and no --data-only.")
    );

    let dry = tql(&["dump", "source", &format!("--to={directory}"), "--dry-run"]);
    assert_eq!(dry.code, 0, "{dry:?}");
    assert!(
        dry.stdout
            .ends_with(&format!("  a SQLite backup of {}\n", text(&source)))
    );
    assert!(!scratch.path("out").exists());
}

#[test]
fn dumps_into_a_dated_folder_beside_the_exports_when_none_is_named() {
    let scratch = Scratch::new("default-folder");
    scratch.sqlite("source", &scratch.widgets("My Shop", &["a"]), &[]);

    let dumped = tql(&["dump", "source"]);
    assert_eq!(dumped.code, 0, "{dumped:?}");

    let exports = paths::config_directory().join("exports");
    let made: Vec<String> = fs::read_dir(&exports)
        .unwrap()
        .flatten()
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(made.len(), 1);
    assert!(made[0].starts_with("my-shop-"), "{made:?}");
    assert_eq!(made[0].len(), "my-shop-".len() + 15);
}

#[test]
fn says_when_there_is_nothing_to_load() {
    let scratch = Scratch::new("nothing");
    let missing = scratch.path("missing");

    let refused = tql(&["load", &text(&missing), "anything"]);

    assert_eq!(refused.code, 1);
    assert_eq!(
        refused.stdout,
        format!("There is nothing at {}.\n", text(&missing))
    );
}

#[test]
fn does_not_take_an_unknown_folder_for_a_dump() {
    let scratch = Scratch::new("not-a-dump");
    scratch.sqlite("target", &scratch.widgets("target", &[]), &[]);
    let folder = scratch.path("folder");
    fs::create_dir_all(&folder).unwrap();

    let refused = tql(&["load", &text(&folder), "target"]);

    assert_eq!(refused.code, 1);
    assert!(
        refused
            .stdout
            .contains("does not look like a dump from tql dump, mydumper or pg_dump.")
    );
}

#[test]
fn makes_one_database_a_copy_of_another_in_one_step_and_cleans_up_after_itself() {
    let scratch = Scratch::new("sync");
    scratch.sqlite("prod", &scratch.widgets("prod", &["live", "data"]), &[]);
    let local = scratch.widgets("local", &["stale"]);
    scratch.sqlite("local", &local, &[]);
    let via = scratch.path("via");

    let synced = tql(&["sync", "prod", "local", &format!("--via={}", text(&via))]);

    assert_eq!(synced.code, 0, "{synced:?}");
    assert!(
        synced.stdout.contains("  Syncing prod into local (about "),
        "{synced:?}"
    );
    assert!(synced.stdout.contains("  Synced in "));
    assert_eq!(names(&local), ["live", "data"]);
    assert!(!via.exists());
}

#[test]
fn keeps_the_dump_when_asked_to() {
    let scratch = Scratch::new("sync-keep");
    scratch.sqlite("prod", &scratch.widgets("prod", &["live"]), &[]);
    scratch.sqlite("local", &scratch.widgets("local", &[]), &[]);
    let via = scratch.path("via");

    let synced = tql(&[
        "sync",
        "prod",
        "local",
        &format!("--via={}", text(&via)),
        "--keep",
    ]);

    assert_eq!(synced.code, 0, "{synced:?}");
    assert!(via.join("tql.json").is_file());
    assert!(synced.stdout.contains("  the dump is kept in "));
}

#[test]
fn will_not_copy_a_database_onto_itself() {
    let scratch = Scratch::new("sync-itself");
    let path = scratch.widgets("only", &["only"]);
    scratch.sqlite("one", &path, &[]);
    scratch.sqlite("same file", &path, &[]);

    let refused = tql(&["sync", "one", "same file"]);

    assert_eq!(refused.code, 1);
    assert!(refused.stdout.contains("That would copy one onto itself."));
    assert_eq!(names(&path), ["only"]);
}

#[test]
fn will_not_sync_into_production_from_a_script_without_force() {
    let scratch = Scratch::new("sync-production");
    scratch.sqlite("local", &scratch.widgets("local", &["dev"]), &[]);
    let production = scratch.widgets("prod", &["live"]);
    scratch.sqlite(
        "prod",
        &production,
        &[("tag", Field::Text("production".into()))],
    );

    let refused = tql(&["sync", "local", "prod", "--no-interaction"]);

    assert_eq!(refused.code, 1);
    assert!(refused.stdout.contains("tagged production"));
    assert_eq!(names(&production), ["live"]);
}

#[test]
fn will_not_sync_between_engines() {
    let scratch = Scratch::new("sync-engines");
    scratch.sqlite("lite", &scratch.widgets("lite", &[]), &[]);
    scratch.server("server", "mysql");

    let refused = tql(&["sync", "lite", "server"]);

    assert_eq!(refused.code, 1);
    assert!(
        refused
            .stdout
            .contains("lite is SQLite and server is MySQL; tql does not move between engines yet.")
    );
}

#[test]
fn will_not_sync_into_a_read_only_connection() {
    let scratch = Scratch::new("sync-read-only");
    scratch.sqlite("prod", &scratch.widgets("prod", &["live"]), &[]);
    scratch.sqlite(
        "locked",
        &scratch.widgets("locked", &[]),
        &[("read_only", Field::Flag(true))],
    );

    let refused = tql(&["sync", "prod", "locked"]);

    assert_eq!(refused.code, 1);
    assert!(refused.stdout.contains("read only"));
}

#[test]
fn quiet_keeps_everything_but_errors_to_itself() {
    let scratch = Scratch::new("quiet");
    scratch.sqlite("source", &scratch.widgets("source", &["a"]), &[]);
    let directory = text(&scratch.path("out"));

    let quiet = tql(&["dump", "source", &format!("--to={directory}"), "-q"]);
    assert_eq!(quiet.code, 0, "{quiet:?}");
    assert_eq!(quiet.stdout, "");

    let failed = tql(&["dump", "source", &format!("--to={directory}"), "--quiet"]);
    assert_eq!(failed.code, 1);
    assert!(failed.stdout.contains("already has something in it"));

    let silent = tql(&["dump", "source", &format!("--to={directory}"), "--silent"]);
    assert_eq!(silent.code, 1);
    assert_eq!(silent.stdout, "");
}

#[test]
fn is_not_mistaken_for_a_file_to_open() {
    for command in ["dump", "load", "import", "sync"] {
        let argv: Vec<String> = ["tql", command, "x", "y"].map(String::from).to_vec();
        assert_eq!(app::argv::rewrite(argv.clone()), argv);
    }
}
