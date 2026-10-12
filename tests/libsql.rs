use std::sync::atomic::{AtomicUsize, Ordering};

use app::database::{self, Database, Kind, Table};
use app::driver::Driver;
use app::store::Connection;

fn server() -> Box<dyn Database> {
    let url = std::env::var("TQL_TEST_LIBSQL").expect("TQL_TEST_LIBSQL is a URL");
    database::connect(
        &Connection {
            driver: Driver::Libsql,
            host: Some(url),
            ..Connection::sqlite("turso", "")
        },
        None,
    )
    .unwrap()
}

fn fresh() -> (Box<dyn Database>, String) {
    static MADE: AtomicUsize = AtomicUsize::new(0);
    let name = format!(
        "people_{}_{}",
        std::process::id(),
        MADE.fetch_add(1, Ordering::Relaxed)
    );
    let mut database = server();
    database
        .run(&format!(
            "create table {name} (id integer primary key, name text, score real, photo blob, note text)"
        ))
        .unwrap();
    database
        .run(&format!(
            "insert into {name} values (1, 'Ada', 1.5, x'00ff', null), (2, 'Grace', 2, null, 'hi')"
        ))
        .unwrap();
    (database, name)
}

#[test]
#[ignore = "needs a libSQL server: set TQL_TEST_LIBSQL=http://host:port"]
fn lists_tables_and_reads_typed_rows() {
    let (mut database, name) = fresh();

    assert!(database.tables().unwrap().contains(&Table::bare(&name)));
    let page = database.rows(&Table::bare(&name), 10, 0, None).unwrap();
    assert_eq!(page.headers, ["id", "name", "score", "photo", "note"]);
    assert_eq!(page.rows[0][0].as_deref(), Some("1"));
    assert_eq!(page.rows[0][2].as_deref(), Some("1.5"));
    assert_eq!(page.rows[0][3].as_deref(), Some("<2 bytes>"));
    assert_eq!(page.rows[0][4], None);
    assert_eq!(page.kinds[0][0], Kind::Integer);
    assert_eq!(page.kinds[0][2], Kind::Float);

    assert_eq!(database.key_columns().unwrap()[&Table::bare(&name)], ["id"]);
    assert_eq!(database.columns(&Table::bare(&name)).unwrap().len(), 5);

    database.run(&format!("drop table {name}")).unwrap();
}

#[test]
#[ignore = "needs a libSQL server: set TQL_TEST_LIBSQL=http://host:port"]
fn edits_adds_and_deletes_by_primary_key() {
    let (mut database, name) = fresh();
    let table = Table::bare(&name);

    assert_eq!(
        database
            .update(&table, "name", "id", "1", Some("Lovelace"))
            .unwrap(),
        1
    );
    database
        .insert(
            &table,
            &vec![
                ("id".to_string(), Some("3".to_string())),
                ("name".to_string(), Some("Mary".to_string())),
            ],
        )
        .unwrap();
    assert_eq!(
        database.delete(&table, "id", &["2".to_string()]).unwrap(),
        1
    );

    let page = database.rows(&table, 10, 0, None).unwrap();
    let names: Vec<_> = page.rows.iter().map(|row| row[1].clone()).collect();
    assert_eq!(
        names,
        [Some("Lovelace".to_string()), Some("Mary".to_string())]
    );

    database.run(&format!("drop table {name}")).unwrap();
}

#[test]
#[ignore = "needs a libSQL server: set TQL_TEST_LIBSQL=http://host:port"]
fn refuses_a_write_hidden_in_a_read_and_leaves_the_rows() {
    let (mut database, name) = fresh();

    assert!(
        database
            .run_reading(&format!(
                "with gone as (select 1) delete from {name} where id in (select * from gone)"
            ))
            .is_err()
    );
    let counted = database
        .run_reading(&format!("select count(*) from {name}"))
        .unwrap();
    assert_eq!(counted.rows, [[Some("2".to_string())]]);

    database.run(&format!("drop table {name}")).unwrap();
}
