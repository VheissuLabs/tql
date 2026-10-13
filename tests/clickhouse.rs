use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};

use app::database::{
    self, Column, Database, Exporter, Filter, Filters, ForeignKey, Index, Joiner, Kind, Page, Sort,
    Table,
};
use app::driver::Driver;
use app::encrypter::Encrypter;
use app::store::Connection;
use app::terminal::Fake;
use app::tui::Browser;

fn server(database: &str) -> Box<dyn Database> {
    let encrypter = Encrypter::throwaway().unwrap();
    database::connect(
        &connection(database, "tql", "tql", &encrypter),
        Some(&encrypter),
    )
    .unwrap()
}

fn connection(database: &str, user: &str, password: &str, encrypter: &Encrypter) -> Connection {
    let place = std::env::var("TQL_TEST_CLICKHOUSE").expect("TQL_TEST_CLICKHOUSE is host:port");
    let (host, port) = place.rsplit_once(':').expect("host:port");
    Connection {
        name: "clickhouse".into(),
        driver: Driver::Clickhouse,
        host: Some(host.into()),
        port: Some(port.parse().unwrap()),
        database: Some(database.into()),
        username: Some(user.into()),
        password: Some(encrypter.encrypt_string(password).unwrap()),
        ..Connection::sqlite("clickhouse", "")
    }
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
    assert_eq!(cell(&page, 0, "tags").as_deref(), Some("['a','b']"));
    assert_eq!(cell(&page, 0, "counts").as_deref(), Some("{'x':1}"));
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
fn estimates_rows_from_the_table_metadata_and_counts_filtered_rows_exactly() {
    let (mut database, name) = scratch();

    assert_eq!(
        database.estimated_rows(&"events".into()),
        Some(database::Total::About(2))
    );
    let filters = Filters::new(
        vec![Filter::new("name", "is", "O'Brien\\path")],
        Joiner::And,
    );
    assert_eq!(
        database
            .count_rows(&"events".into(), Some(&filters))
            .unwrap(),
        1
    );
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

#[test]
#[ignore = "needs a ClickHouse server: set TQL_TEST_CLICKHOUSE=host:port (user tql, password tql)"]
fn pages_through_a_table_without_skipping_or_repeating_rows() {
    let (mut database, name) = scratch();
    database
        .run("create table parts (id UInt64, v String) engine = MergeTree order by tuple()")
        .unwrap();
    for part in 0..8 {
        database
            .run(&format!(
                "insert into parts select number * 8 + {part}, 'x' from numbers(500)"
            ))
            .unwrap();
    }
    database
        .run("create table loose (id UInt64) engine = Memory")
        .unwrap();
    for part in 0..8 {
        database
            .run(&format!(
                "insert into loose select number * 8 + {part} from numbers(500)"
            ))
            .unwrap();
    }

    let filters = Filters::new(vec![Filter::new("v", "contains", "x")], Joiner::And);
    for (table, filters) in [("parts", Some(&filters)), ("loose", None)] {
        let mut seen = std::collections::HashSet::new();
        for offset in (0..4000).step_by(250) {
            let page = database
                .filtered_rows(&table.into(), 250, offset, None, filters)
                .unwrap();
            for row in &page.rows {
                assert!(seen.insert(row[0].clone()), "{table} repeated {:?}", row[0]);
            }
        }
        assert_eq!(seen.len(), 4000, "{table}");
    }

    drop(&mut database, &name);
}

#[test]
#[ignore = "needs a ClickHouse server: set TQL_TEST_CLICKHOUSE=host:port (user tql, password tql)"]
fn a_failure_partway_through_an_answer_keeps_the_servers_words() {
    let mut database = server("default");
    let failed = database
        .run(
            "select number, throwIf(number = 900000, 'boom') from numbers(1000000) \
             settings max_block_size = 1000",
        )
        .err()
        .unwrap()
        .to_string();
    assert!(failed.contains("boom"), "{failed}");
    assert!(!failed.contains("default_format"), "{failed}");
}

#[test]
#[ignore = "needs a ClickHouse server: set TQL_TEST_CLICKHOUSE=host:port (user tql, password tql)"]
fn reads_an_answer_bigger_than_ten_megabytes() {
    let mut database = server("default");
    let outcome = database
        .run("select randomPrintableASCII(200) from numbers(100000)")
        .unwrap();
    assert_eq!(outcome.rows.len(), 100000);
}

#[test]
#[ignore = "needs a ClickHouse server: set TQL_TEST_CLICKHOUSE=host:port (user tql, password tql)"]
fn keeps_a_session_between_statements_and_counts_written_rows() {
    let (mut database, name) = scratch();

    database
        .run("create temporary table scratch (a Int8)")
        .unwrap();
    let written = database
        .run("insert into scratch select number from numbers(5)")
        .unwrap();
    assert_eq!(written.affected, Some(5));
    let read = database.run("select count() from scratch").unwrap();
    assert_eq!(read.rows, [[Some("5".to_string())]]);

    drop(&mut database, &name);
}

#[test]
#[ignore = "needs a ClickHouse server: set TQL_TEST_CLICKHOUSE=host:port (user tql, password tql)"]
fn counts_filtered_rows_of_a_table_with_a_question_mark_in_its_name() {
    let (mut database, name) = scratch();
    database
        .run("create table `q?` (name String) engine = Memory")
        .unwrap();
    database
        .run("insert into `q?` values ('a'), ('b')")
        .unwrap();

    let filters = Filters::new(vec![Filter::new("name", "is", "a")], Joiner::And);
    assert_eq!(
        database.count_rows(&"q?".into(), Some(&filters)).unwrap(),
        1
    );

    drop(&mut database, &name);
}

#[test]
#[ignore = "needs a ClickHouse server: set TQL_TEST_CLICKHOUSE=host:port (user tql, password tql)"]
fn a_read_only_user_can_connect_and_query() {
    let mut admin = server("default");
    let user = format!("tql_ro_{}", std::process::id());
    if admin
        .run(&format!(
            "create user {user} identified by 'ro' settings readonly = 1"
        ))
        .is_err()
    {
        eprintln!("this server does not let tql create users, so the read-only check is skipped");
        return;
    }
    admin
        .run(&format!("grant select on system.* to {user}"))
        .unwrap();

    let encrypter = Encrypter::throwaway().unwrap();
    let reader = database::connect(
        &connection("default", &user, "ro", &encrypter),
        Some(&encrypter),
    )
    .and_then(|mut reader| reader.run_reading("select 1 as n"));
    admin.run(&format!("drop user {user}")).unwrap();
    assert_eq!(reader.unwrap().rows, [[Some("1".to_string())]]);
}

#[test]
#[ignore = "needs a ClickHouse server: set TQL_TEST_CLICKHOUSE=host:port (user tql, password tql)"]
fn an_export_with_backslashes_and_quotes_loads_back() {
    let (mut database, name) = scratch();
    database
        .run("create table paths (id UInt8, path String) engine = MergeTree order by id")
        .unwrap();
    database
        .run("insert into paths values (1, 'C:\\\\'), (2, 'O''Brien\\\\n')")
        .unwrap();
    database
        .run("create table copied as paths engine = MergeTree order by id")
        .unwrap();

    let encrypter = Encrypter::throwaway().unwrap();
    let file = std::env::temp_dir().join(format!("{name}.sql"));
    Exporter::new(&connection(&name, "tql", "tql", &encrypter), None)
        .table(database.as_mut(), &"paths".into(), Some(file.clone()), None)
        .unwrap();
    let sql = std::fs::read_to_string(&file).unwrap();
    let _ = std::fs::remove_file(&file);
    let insert = sql
        .split_once("insert into")
        .map(|(_, rest)| format!("insert into {}", rest.trim().trim_end_matches(';')))
        .unwrap();
    database
        .run(&insert.replacen("`paths`", "`copied`", 1))
        .unwrap();

    let copied = database.run("select path from copied order by id").unwrap();
    assert_eq!(
        copied.rows,
        [[Some("C:\\".to_string())], [Some("O'Brien\\n".to_string())]]
    );

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
