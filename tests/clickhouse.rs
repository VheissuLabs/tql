use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};

use app::database::{
    self, Column, Database, Filter, Filters, ForeignKey, Index, Joiner, Kind, Page, Sort, Table,
};
use app::driver::Driver;
use app::encrypter::Encrypter;
use app::store::Connection;
use app::terminal::Fake;
use app::tui::Browser;

fn server(database: &str) -> Box<dyn Database> {
    let place = std::env::var("TQL_TEST_CLICKHOUSE").expect("TQL_TEST_CLICKHOUSE is host:port");
    let (host, port) = place.rsplit_once(':').expect("host:port");
    let encrypter = Encrypter::throwaway().unwrap();
    database::connect(
        &Connection {
            name: "clickhouse".into(),
            driver: Driver::Clickhouse,
            host: Some(host.into()),
            port: Some(port.parse().unwrap()),
            database: Some(database.into()),
            username: Some("tql".into()),
            password: Some(encrypter.encrypt_string("tql").unwrap()),
            ..Connection::sqlite("clickhouse", "")
        },
        Some(&encrypter),
    )
    .unwrap()
}

fn scratch() -> (Box<dyn Database>, String) {
    static MADE: AtomicUsize = AtomicUsize::new(0);
    let name = format!(
        "tql_{}_{}",
        std::process::id(),
        MADE.fetch_add(1, Ordering::Relaxed)
    );
    let mut database = server("default");
    database.run(&format!("create database {name}")).unwrap();
    database.use_database(&name).unwrap();
    database
        .run(
            "create table events (id UInt64, name String, tags Array(String), \
             counts Map(String, UInt8), maybe Nullable(Int32), kind LowCardinality(String), \
             huge Int256, price Decimal(18, 4), at DateTime64(3), ok Bool) \
             engine = MergeTree order by id",
        )
        .unwrap();
    database
        .run(
            "insert into events values \
             (1, 'Ada', ['a', 'b'], {'x': 1}, null, 'web', \
              57896044618658097711785492504343953926634992332820282019728792003956564819967, \
              12.3456, '2026-10-12 09:14:00.123', true), \
             (2, 'O''Brien\\\\path', [], {}, 7, 'app', -1, 0.5, '2026-10-12 10:00:00.000', false)",
        )
        .unwrap();
    database
        .run("create view recent as select id, name from events")
        .unwrap();
    (database, name)
}

fn drop(database: &mut Box<dyn Database>, name: &str) {
    database.run(&format!("drop database {name}")).unwrap();
}

fn cell(page: &Page, row: usize, column: &str) -> Option<String> {
    let at = page.headers.iter().position(|header| header == column)?;
    page.rows[row][at].clone()
}

#[test]
#[ignore = "needs a ClickHouse server: set TQL_TEST_CLICKHOUSE=host:port (user tql, password tql)"]
fn lists_databases_tables_and_views() {
    let (mut database, name) = scratch();

    let databases = database.databases().unwrap();
    assert!(databases.contains(&name), "{databases:?}");
    assert!(!databases.contains(&"system".to_string()));
    let tables = database.tables().unwrap();
    assert!(tables.contains(&Table::bare("events")), "{tables:?}");
    assert!(tables.contains(&Table::bare("recent")));

    drop(&mut database, &name);
}

#[test]
#[ignore = "needs a ClickHouse server: set TQL_TEST_CLICKHOUSE=host:port (user tql, password tql)"]
fn shows_every_clickhouse_type_exactly() {
    let (mut database, name) = scratch();

    let page = database.rows(&"events".into(), 10, 0, None).unwrap();
    assert_eq!(cell(&page, 0, "id").as_deref(), Some("1"));
    assert_eq!(cell(&page, 0, "tags").as_deref(), Some(r#"["a","b"]"#));
    assert_eq!(cell(&page, 0, "counts").as_deref(), Some(r#"{"x":1}"#));
    assert_eq!(cell(&page, 0, "maybe"), None);
    assert_eq!(cell(&page, 1, "maybe").as_deref(), Some("7"));
    assert_eq!(cell(&page, 0, "kind").as_deref(), Some("web"));
    assert_eq!(
        cell(&page, 0, "huge").as_deref(),
        Some("57896044618658097711785492504343953926634992332820282019728792003956564819967")
    );
    assert_eq!(cell(&page, 0, "price").as_deref(), Some("12.3456"));
    assert_eq!(
        cell(&page, 0, "at").as_deref(),
        Some("2026-10-12 09:14:00.123")
    );
    assert_eq!(cell(&page, 0, "ok").as_deref(), Some("true"));
    assert_eq!(cell(&page, 1, "name").as_deref(), Some("O'Brien\\path"));
    assert_eq!(page.kinds[0][0], Kind::Integer);

    let columns = database.columns(&"events".into()).unwrap();
    assert_eq!(columns.len(), 10);
    assert!(columns[4].nullable);
    assert_eq!(
        database.key_columns().unwrap()[&Table::bare("events")],
        ["id"]
    );

    drop(&mut database, &name);
}

#[test]
#[ignore = "needs a ClickHouse server: set TQL_TEST_CLICKHOUSE=host:port (user tql, password tql)"]
fn filters_and_sorts_with_values_that_need_escaping() {
    let (mut database, name) = scratch();

    let filters = Filters::new(
        vec![Filter::new("name", "is", "O'Brien\\path")],
        Joiner::And,
    );
    let page = database
        .filtered_rows(&"events".into(), 10, 0, None, Some(&filters))
        .unwrap();
    assert_eq!(page.rows.len(), 1, "{}", page.statement);
    assert_eq!(cell(&page, 0, "id").as_deref(), Some("2"));

    let sorted = database
        .rows(
            &"events".into(),
            10,
            0,
            Some(&Sort {
                column: "id".to_string(),
                direction: database::Direction::Desc,
            }),
        )
        .unwrap();
    assert_eq!(cell(&sorted, 0, "id").as_deref(), Some("2"));

    drop(&mut database, &name);
}

#[test]
#[ignore = "needs a ClickHouse server: set TQL_TEST_CLICKHOUSE=host:port (user tql, password tql)"]
fn reading_runs_refuse_a_write() {
    let (mut database, name) = scratch();

    assert!(
        database
            .run_reading("insert into events (id, name) values (3, 'x')")
            .is_err()
    );
    let counted = database
        .run_reading("select count() as n from events")
        .unwrap();
    assert_eq!(counted.rows, [[Some("2".to_string())]]);

    drop(&mut database, &name);
}

struct Columnar;

impl Database for Columnar {
    fn tables(&mut self) -> anyhow::Result<Vec<Table>> {
        Ok(vec![Table::bare("events")])
    }

    fn filtered_rows(
        &mut self,
        _: &Table,
        _: usize,
        _: usize,
        _: Option<&Sort>,
        _: Option<&Filters>,
    ) -> anyhow::Result<Page> {
        Ok(Page {
            headers: vec!["id".to_string(), "name".to_string()],
            rows: vec![vec![Some("1".to_string()), Some("Ada".to_string())]],
            ..Page::default()
        })
    }

    fn primary_keys(&mut self) -> anyhow::Result<HashMap<Table, String>> {
        Ok(HashMap::from([(Table::bare("events"), "id".to_string())]))
    }

    fn select(&mut self, _: &str, _: &[String]) -> anyhow::Result<Page> {
        Ok(Page::default())
    }

    fn columns(&mut self, _: &Table) -> anyhow::Result<Vec<Column>> {
        Ok(Vec::new())
    }

    fn indexes(&mut self, _: &Table) -> anyhow::Result<Vec<Index>> {
        Ok(Vec::new())
    }

    fn foreign_keys(&mut self) -> anyhow::Result<HashMap<Table, Vec<ForeignKey>>> {
        Ok(HashMap::new())
    }
}

#[test]
fn editing_a_clickhouse_table_says_why_it_cannot() {
    let connection = Connection {
        driver: Driver::Clickhouse,
        database: Some("default".into()),
        ..Connection::sqlite("clickhouse", "")
    };
    let mut fake = Fake::sized(
        Browser::new(connection, Box::new(Columnar)).unwrap(),
        100,
        24,
    );
    fake.settle();
    fake.app.focus = app::tui::Focus::Grid;
    let message = "ClickHouse tables are read-only in tql: its updates are background mutations";

    fake.press("e");
    assert!(fake.screen().contains("read-only"), "{}", fake.screen());
    assert_eq!(fake.app.edits.read_only_reason(), Some(message));
    fake.press("\x1b");

    for key in ["E", "d", "N"] {
        fake.press(key);
        let screen = fake.screen();
        assert!(screen.contains(message), "after {key}: {screen}");
        fake.press("\x1b");
    }
    assert!(fake.app.edits.deletes.is_empty());
    assert!(fake.app.edits.inserts.is_empty());
}
