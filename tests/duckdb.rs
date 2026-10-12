use std::path::PathBuf;

use app::database::{self, Database, Table};
use app::driver::Driver;
use app::store::Connection;

struct Scratch {
    path: PathBuf,
}

impl Scratch {
    fn new(label: &str) -> Scratch {
        let path =
            std::env::temp_dir().join(format!("tql-duckdb-{}-{label}.duckdb", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let made = duckdb::Connection::open(&path).unwrap();
        made.execute_batch(
            "create table people (id integer primary key, name varchar, tags varchar[], \
             meta struct(a integer, b varchar), born date, seen timestamp, amount decimal(18, 2), \
             big hugeint, flag boolean, owner integer);
             insert into people values
               (1, 'Ada', ['a', 'b'], {'a': 1, 'b': 'x'}, '2026-10-12', '2026-10-12 09:14:00',
                12.5, 170141183460469231731687303715884105727, true, null),
               (2, 'Grace', [], null, null, null, null, null, false, 1);
             create schema ev;
             create table ev.people (id integer primary key, name varchar);
             insert into ev.people values (1, 'Hopper'), (2, 'Lovelace');
             create table ev.notes (id integer primary key, person integer references ev.people (id));",
        )
        .unwrap();
        drop(made);
        Scratch { path }
    }

    fn database(&self) -> Box<dyn Database> {
        database::connect(
            &Connection {
                driver: Driver::Duckdb,
                database: Some(self.path.display().to_string()),
                ..Connection::sqlite("duck", "")
            },
            None,
        )
        .unwrap()
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
        let _ = std::fs::remove_file(self.path.with_extension("duckdb.wal"));
    }
}

fn cell(page: &database::Page, row: usize, column: &str) -> Option<String> {
    let at = page.headers.iter().position(|header| header == column)?;
    page.rows[row][at].clone()
}

#[test]
fn lists_tables_under_their_schemas_with_main_as_the_default() {
    let scratch = Scratch::new("tables");
    let mut database = scratch.database();

    let tables = database.tables().unwrap();
    assert!(tables.contains(&Table::bare("people")), "{tables:?}");
    assert!(tables.contains(&Table::in_schema("ev", "people")));
    assert!(tables.contains(&Table::in_schema("ev", "notes")));
    assert_eq!(database.default_schema().as_deref(), Some("main"));
}

#[test]
fn shows_every_duckdb_type_readably() {
    let scratch = Scratch::new("types");
    let mut database = scratch.database();

    let page = database.rows(&"people".into(), 10, 0, None).unwrap();
    assert_eq!(cell(&page, 0, "tags").as_deref(), Some(r#"["a","b"]"#));
    assert_eq!(
        cell(&page, 0, "meta").as_deref(),
        Some(r#"{"a":1,"b":"x"}"#)
    );
    assert_eq!(cell(&page, 0, "born").as_deref(), Some("2026-10-12"));
    assert_eq!(
        cell(&page, 0, "seen").as_deref(),
        Some("2026-10-12 09:14:00")
    );
    assert_eq!(cell(&page, 0, "amount").as_deref(), Some("12.50"));
    assert_eq!(
        cell(&page, 0, "big").as_deref(),
        Some("170141183460469231731687303715884105727")
    );
    assert_eq!(cell(&page, 0, "flag").as_deref(), Some("true"));
    assert_eq!(cell(&page, 1, "meta"), None);
    assert_eq!(cell(&page, 1, "tags").as_deref(), Some("[]"));
}

#[test]
fn knows_keys_columns_and_links() {
    let scratch = Scratch::new("keys");
    let mut database = scratch.database();

    assert_eq!(
        database.key_columns().unwrap()[&Table::bare("people")],
        ["id"]
    );
    assert_eq!(
        database.key_columns().unwrap()[&Table::in_schema("ev", "people")],
        ["id"]
    );
    let columns = database.columns(&"people".into()).unwrap();
    assert_eq!(columns.len(), 10);
    assert_eq!(columns[0].name, "id");
    assert!(
        database
            .indexes(&"people".into())
            .unwrap()
            .iter()
            .any(|index| index.primary && index.columns == ["id"])
    );
    let links = database.foreign_keys().unwrap();
    let notes = &links[&Table::in_schema("ev", "notes")][0];
    assert_eq!(notes.columns, ["person"]);
    assert_eq!(notes.table, Table::in_schema("ev", "people"));
}

#[test]
fn edits_adds_and_deletes_by_primary_key_in_the_right_schema() {
    let scratch = Scratch::new("edits");
    let mut database = scratch.database();
    let theirs = Table::in_schema("ev", "people");

    assert_eq!(
        database
            .update(&theirs, "name", "id", "1", Some("Grace"))
            .unwrap(),
        1
    );
    database
        .insert(
            &theirs,
            &vec![
                ("id".to_string(), Some("3".to_string())),
                ("name".to_string(), Some("Mary".to_string())),
            ],
        )
        .unwrap();
    assert_eq!(
        database.delete(&theirs, "id", &["2".to_string()]).unwrap(),
        1
    );

    let page = database.rows(&theirs, 10, 0, None).unwrap();
    let names: Vec<_> = page.rows.iter().map(|row| row[1].clone()).collect();
    assert_eq!(names, [Some("Grace".to_string()), Some("Mary".to_string())]);
    let ours = database.rows(&"people".into(), 10, 0, None).unwrap();
    assert_eq!(cell(&ours, 0, "name").as_deref(), Some("Ada"));
}

#[test]
fn refuses_a_write_hidden_in_a_read() {
    let scratch = Scratch::new("reading");
    let mut database = scratch.database();

    let refused = database
        .run_reading("with gone as (delete from ev.people returning *) select * from gone")
        .unwrap_err();
    assert!(refused.to_string().contains("delete"), "{refused}");
    assert_eq!(
        database
            .rows(&Table::in_schema("ev", "people"), 10, 0, None)
            .unwrap()
            .rows
            .len(),
        2
    );

    let read = database
        .run_reading("select count(*) as n from people")
        .unwrap();
    assert_eq!(read.rows, [[Some("2".to_string())]]);
}

#[test]
fn runs_a_statement_and_says_how_many_rows_it_changed() {
    let scratch = Scratch::new("run");
    let mut database = scratch.database();

    let changed = database
        .run("update ev.people set name = upper(name)")
        .unwrap();
    assert_eq!(changed.affected, Some(2));
    let selected = database
        .run("select name from ev.people order by id")
        .unwrap();
    assert_eq!(selected.headers, ["name"]);
    assert_eq!(selected.rows[0][0].as_deref(), Some("HOPPER"));
}

#[test]
fn says_when_there_is_no_file() {
    let failed = database::connect(
        &Connection {
            driver: Driver::Duckdb,
            database: Some("/nowhere/at/all.duckdb".to_string()),
            ..Connection::sqlite("duck", "")
        },
        None,
    )
    .err()
    .unwrap();
    assert!(failed.to_string().contains("no DuckDB file"), "{failed}");
}
