use std::fs;
use std::path::{Path, PathBuf};

use app::database::{self, Database, Exporter};
use app::driver::Driver;
use app::settings::{self, Settings};
use app::store::Connection;
use app::terminal::Fake;
use app::tui::Browser;

const PALETTE: &str = "\x0b";
const RUN: &str = "\x12";

fn folder(name: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!("tql-rust-export-{}-{name}", std::process::id()));
    let _ = fs::remove_dir_all(&path);
    fs::create_dir_all(&path).unwrap();
    path
}

fn exporting_into(exports: &Path) {
    let mut settings = Settings::default();
    settings.export_path = exports.display().to_string();
    settings::install(settings);
}

struct Fixture {
    folder: PathBuf,
    exports: PathBuf,
    path: PathBuf,
    connection: Connection,
}

impl Fixture {
    fn new(name: &str) -> Self {
        let folder = folder(name);
        let exports = folder.join("exports");
        let path = folder.join("export-source.sqlite");
        let writer = rusqlite::Connection::open(&path).unwrap();
        writer
            .execute_batch(
                "create table things (id integer primary key, name text, note text, qty integer, price real)",
            )
            .unwrap();
        let mut statement = writer
            .prepare("insert into things (name, note, qty, price) values (?, ?, ?, ?)")
            .unwrap();
        statement
            .execute(rusqlite::params!["it's quoted", None::<String>, 1, 9.99])
            .unwrap();
        statement
            .execute(rusqlite::params![
                "unicode — é ü 日本",
                "line\nbreak",
                2,
                0.5
            ])
            .unwrap();
        statement
            .execute(rusqlite::params!["back\\slash", "semi;colon", 3, 1000.0])
            .unwrap();
        statement
            .execute(rusqlite::params![None::<String>, "trailing space ", 0, 0.0])
            .unwrap();
        exporting_into(&exports);
        Fixture {
            connection: Connection::sqlite("exporting", path.to_str().unwrap()),
            folder,
            exports,
            path,
        }
    }

    fn database(&self) -> Box<dyn Database> {
        database::connect(&self.connection, None).unwrap()
    }

    fn run(&self, sql: &str) {
        rusqlite::Connection::open(&self.path)
            .unwrap()
            .execute_batch(sql)
            .unwrap();
    }

    fn store(&self) -> PathBuf {
        let store = self.folder.join("tql.sqlite");
        rusqlite::Connection::open(&store)
            .unwrap()
            .execute_batch(
                "create table if not exists settings (key varchar primary key, value text, created_at datetime, updated_at datetime)",
            )
            .unwrap();
        store
    }
}

fn all_rows(path: &Path) -> Vec<Vec<String>> {
    let reader = rusqlite::Connection::open(path).unwrap();
    let mut statement = reader
        .prepare("select id, name, note, qty, price, typeof(price) from things order by id")
        .unwrap();
    statement
        .query_map([], |row| {
            (0..6)
                .map(|index| {
                    row.get::<_, rusqlite::types::Value>(index)
                        .map(|value| format!("{value:?}"))
                })
                .collect()
        })
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap()
}

#[test]
fn exports_a_table_to_re_importable_sql() {
    let fixture = Fixture::new("round-trip");

    let exported = Exporter::new(&fixture.connection, None)
        .table(fixture.database().as_mut(), &"things".into(), None, None)
        .unwrap();

    assert_eq!(exported.rows, 4);
    let sql = fs::read_to_string(&exported.path).unwrap();
    assert!(sql.contains("insert into \"things\" (\"id\", \"name\", \"note\", \"qty\", \"price\") values\n(1, 'it''s quoted', null, 1, 9.99),"), "{sql}");

    let target = fixture.folder.join("imported.sqlite");
    let imported = rusqlite::Connection::open(&target).unwrap();
    imported
        .execute_batch(
            "create table things (id integer primary key, name text, note text, qty integer, price real)",
        )
        .unwrap();
    imported.execute_batch(&sql).unwrap();

    assert_eq!(all_rows(&target), all_rows(&fixture.path));
}

#[test]
fn writes_a_header_naming_the_connection_and_table() {
    let fixture = Fixture::new("header");

    let exported = Exporter::new(&fixture.connection, None)
        .table(fixture.database().as_mut(), &"things".into(), None, None)
        .unwrap();
    let sql = fs::read_to_string(&exported.path).unwrap();

    assert!(sql.starts_with(
        "-- tql export\n-- connection: exporting (sqlite)\n-- table: things\n-- exported: "
    ));
    assert!(sql.contains("+00:00\n\ninsert into"));
}

#[test]
fn exports_an_empty_table_without_insert_statements() {
    let fixture = Fixture::new("empty");
    fixture.run("create table empties (id integer primary key)");

    let exported = Exporter::new(&fixture.connection, None)
        .table(fixture.database().as_mut(), &"empties".into(), None, None)
        .unwrap();

    assert_eq!(exported.rows, 0);
    assert!(
        !fs::read_to_string(&exported.path)
            .unwrap()
            .contains("insert into")
    );
}

#[test]
fn names_exports_after_the_database_table_and_time() {
    let fixture = Fixture::new("naming");

    let exported = Exporter::new(&fixture.connection, None)
        .table(fixture.database().as_mut(), &"things".into(), None, None)
        .unwrap();

    let name = exported
        .path
        .file_name()
        .unwrap()
        .to_string_lossy()
        .into_owned();
    assert_eq!(exported.path.parent().unwrap(), fixture.exports);
    assert!(name.starts_with("export-source-things-20"), "{name}");
    assert!(name.ends_with(".sql"));
    assert_eq!(name.len(), "export-source-things-YYYYMMDD-HHMMSS.sql".len());
}

#[test]
fn names_exports_after_the_database_in_use_on_a_server() {
    let exports = folder("server-naming");
    exporting_into(&exports);
    let mut connection = Connection {
        name: "MySql Dev".into(),
        driver: Driver::Mysql,
        database: Some("karlm_v2".into()),
        ..Connection::default()
    };

    let whole = Exporter::new(&connection, None).filename(None).unwrap();
    assert!(
        whole
            .file_name()
            .unwrap()
            .to_string_lossy()
            .starts_with("karlm-v2-20")
    );

    connection.database = Some("other_db".into());
    let table = Exporter::new(&connection, None)
        .filename(Some(&"users".into()))
        .unwrap();
    assert!(
        table
            .file_name()
            .unwrap()
            .to_string_lossy()
            .starts_with("other-db-users-")
    );

    connection.database = Some(String::new());
    let unnamed = Exporter::new(&connection, None).filename(None).unwrap();
    assert!(
        unnamed
            .file_name()
            .unwrap()
            .to_string_lossy()
            .starts_with("mysql-dev-20")
    );
}

#[test]
fn exports_every_table_into_one_file() {
    let fixture = Fixture::new("every-table");
    fixture.run(
        "create table others (id integer primary key, label text); insert into others (label) values ('one'), ('two');",
    );

    let exported = Exporter::new(&fixture.connection, None)
        .tables(
            fixture.database().as_mut(),
            &["things".into(), "others".into()],
            None,
            None,
        )
        .unwrap();
    let sql = fs::read_to_string(&exported.path).unwrap();

    assert_eq!(exported.rows, 6);
    assert!(sql.contains("-- table: things, others\n"));
    assert!(sql.contains("\n-- things\ninsert into \"things\""));
    assert!(sql.contains(";\n\n-- others\ninsert into \"others\""));
    assert_eq!(sql.matches("-- tql export").count(), 1);
    assert!(
        exported
            .path
            .file_name()
            .unwrap()
            .to_string_lossy()
            .starts_with("export-source-20")
    );
}

#[test]
fn honours_a_row_limit() {
    let fixture = Fixture::new("limit");

    let exported = Exporter::new(&fixture.connection, None)
        .table(fixture.database().as_mut(), &"things".into(), None, Some(2))
        .unwrap();

    assert_eq!(exported.rows, 2);
    assert!(
        fs::read_to_string(&exported.path)
            .unwrap()
            .contains("(2, 'unicode — é ü 日本', 'line\nbreak', 2, 0.5);")
    );
}

#[test]
fn pages_through_a_table_in_the_order_of_its_whole_primary_key() {
    let fixture = Fixture::new("composite");
    fixture.run(
        "create table pairs (a integer, b integer, label text, primary key (a, b));
         insert into pairs values (2, 1, 'third'), (1, 2, 'second'), (1, 1, 'first');",
    );

    let exported = Exporter::new(&fixture.connection, None)
        .table(fixture.database().as_mut(), &"pairs".into(), None, None)
        .unwrap();
    let sql = fs::read_to_string(&exported.path).unwrap();

    assert!(
        sql.contains("(1, 1, 'first'),\n(1, 2, 'second'),\n(2, 1, 'third');"),
        "{sql}"
    );
}

#[test]
fn splits_big_tables_into_statements_of_a_hundred_rows() {
    let fixture = Fixture::new("batches");
    fixture.run(
        "create table many (id integer primary key, label text);
         with recursive counter(at) as (select 1 union all select at + 1 from counter where at < 1234)
         insert into many select at, 'row ' || at from counter;",
    );

    let exported = Exporter::new(&fixture.connection, None)
        .table(fixture.database().as_mut(), &"many".into(), None, None)
        .unwrap();
    let sql = fs::read_to_string(&exported.path).unwrap();

    assert_eq!(exported.rows, 1234);
    assert_eq!(sql.matches("insert into").count(), 13);
    assert!(sql.contains("(1234, 'row 1234');\n\n"));
}

#[test]
fn writes_binary_values_as_hex() {
    let fixture = Fixture::new("binary");
    fixture.run(
        "create table blobs (id integer primary key, payload blob); insert into blobs values (1, x'00ff41');",
    );

    let exported = Exporter::new(&fixture.connection, None)
        .table(fixture.database().as_mut(), &"blobs".into(), None, None)
        .unwrap();

    assert!(
        fs::read_to_string(&exported.path)
            .unwrap()
            .contains("(1, X'00ff41');")
    );
}

#[test]
fn offers_the_folder_the_last_export_went_to() {
    let fixture = Fixture::new("remember");
    let store = fixture.store();
    settings::install(Settings::default());
    let by_hand = fixture.folder.join("by-hand");
    fs::create_dir_all(&by_hand).unwrap();
    let exporter = Exporter::new(&fixture.connection, Some(&store));

    exporter
        .table(
            fixture.database().as_mut(),
            &"things".into(),
            Some(by_hand.join("by-hand.sql")),
            None,
        )
        .unwrap();

    assert_eq!(exporter.directory().unwrap(), by_hand);
    assert_eq!(exporter.filename(None).unwrap().parent().unwrap(), by_hand);
}

#[test]
fn forgets_a_remembered_folder_that_has_gone_away() {
    let fixture = Fixture::new("gone");
    let store = fixture.store();
    let gone = fixture.folder.join("gone");
    rusqlite::Connection::open(&store)
        .unwrap()
        .execute(
            "insert into settings (key, value) values ('export_directory', ?1)",
            [gone.display().to_string()],
        )
        .unwrap();

    let directory = Exporter::new(&fixture.connection, Some(&store))
        .directory()
        .unwrap();

    assert_eq!(directory, fixture.exports);
}

#[test]
fn lets_the_config_say_where_exports_go() {
    let fixture = Fixture::new("configured");
    let store = fixture.store();
    rusqlite::Connection::open(&store)
        .unwrap()
        .execute(
            "insert into settings (key, value) values ('export_directory', ?1)",
            [fixture.folder.display().to_string()],
        )
        .unwrap();
    let configured = fixture.folder.join("configured");
    exporting_into(&configured);

    let directory = Exporter::new(&fixture.connection, Some(&store))
        .directory()
        .unwrap();

    assert_eq!(directory, configured);
    assert!(configured.is_dir());
}

fn browsing(fixture: &Fixture) -> Fake<Browser> {
    let browser = Browser::new(fixture.connection.clone(), fixture.database()).unwrap();
    exporting_into(&fixture.exports);
    Fake::sized(browser, 200, 30)
}

fn only_export(fixture: &Fixture) -> String {
    let files: Vec<_> = fs::read_dir(&fixture.exports)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .collect();
    assert_eq!(files.len(), 1, "{files:?}");
    fs::read_to_string(&files[0]).unwrap()
}

#[test]
fn exports_the_open_table_from_the_browser() {
    let fixture = Fixture::new("browser");
    let mut fake = browsing(&fixture);

    fake.press(":export\n");

    let status = fake.app.status().to_string();
    assert!(status.starts_with("exported 4 rows ("), "{status}");
    assert!(
        status.contains(&format!(
            "to {}/export-source-things-",
            fixture.exports.display()
        )),
        "{status}"
    );
    assert!(only_export(&fixture).contains("insert into \"things\""));
}

#[test]
fn export_sql_is_the_same_command() {
    let fixture = Fixture::new("browser-sql");
    let mut fake = browsing(&fixture);

    fake.press(":export sql\n");

    assert!(fake.app.status().starts_with("exported 4 rows"));
}

#[test]
fn exports_query_results_as_results() {
    let fixture = Fixture::new("browser-results");
    let mut fake = browsing(&fixture);

    fake.press("s");
    for _ in 0..fake.app.query.editor.buffer().chars().count() {
        fake.press("\x7f");
    }
    fake.press("select name, qty from things where qty > 1");
    fake.press(RUN);
    fake.press(PALETTE);
    fake.press("export this table\n");

    assert!(
        fake.app.status().starts_with("exported 2 rows"),
        "{}",
        fake.app.status()
    );
    let sql = only_export(&fixture);
    assert!(sql.contains("-- table: results\n"), "{sql}");
    assert!(sql.contains(
        "insert into \"results\" (\"name\", \"qty\") values\n('unicode — é ü 日本', '2'),\n('back\\slash', '3');"
    ));
}

#[test]
fn exports_from_the_palette() {
    let fixture = Fixture::new("browser-palette");
    let mut fake = browsing(&fixture);

    fake.press(PALETTE);
    fake.press("export this table");
    fake.press("\n");

    assert!(fake.app.status().starts_with("exported 4 rows"));
}

#[test]
fn says_when_the_export_fails() {
    let fixture = Fixture::new("browser-fails");
    let mut fake = browsing(&fixture);
    fs::write(fixture.folder.join("blocked"), "").unwrap();
    exporting_into(&fixture.folder.join("blocked"));

    fake.press(":export\n");

    assert!(
        fake.app.status().starts_with("export failed: "),
        "{}",
        fake.app.status()
    );
}
