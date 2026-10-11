use app::database::{self, Database, Kind};
use app::store::Connection;

fn sqlite(label: &str) -> Box<dyn Database> {
    let path =
        std::env::temp_dir().join(format!("tql-kinds-{}-{label}.sqlite", std::process::id()));
    let _ = std::fs::remove_file(&path);
    rusqlite::Connection::open(&path).unwrap();
    database::connect(&Connection::sqlite("kinds", path.to_str().unwrap()), None).unwrap()
}

fn herd(driver: &str, port: u16, database: &str) -> Box<dyn Database> {
    let connection = Connection {
        name: "herd".into(),
        driver: driver.into(),
        host: Some("127.0.0.1".into()),
        port: Some(port),
        database: Some(database.into()),
        username: Some("root".into()),
        ..Connection::sqlite("herd", "")
    };
    database::connect(&connection, None).unwrap()
}

#[test]
fn sqlite_says_what_each_value_is_stored_as() {
    let outcome = sqlite("kinds").run("select 7, 1.5, '8', null").unwrap();

    assert_eq!(
        outcome.kinds,
        [[Kind::Integer, Kind::Float, Kind::Text, Kind::Text]]
    );
}

#[test]
#[ignore = "needs Herd's Postgres on 127.0.0.1:5432"]
fn postgres_says_which_values_are_whole_numbers_and_booleans() {
    let outcome = herd("pgsql", 5432, "postgres")
        .run("select 7::int, true, false, 1.5::numeric, 'x'::text, 0.25::float8")
        .unwrap();

    assert_eq!(
        outcome.rows,
        [[
            Some("7".to_string()),
            Some("true".to_string()),
            Some("false".to_string()),
            Some("1.5".to_string()),
            Some("x".to_string()),
            Some("0.25".to_string()),
        ]]
    );
    assert_eq!(
        outcome.kinds,
        [[
            Kind::Integer,
            Kind::Boolean,
            Kind::Boolean,
            Kind::Text,
            Kind::Text,
            Kind::Float
        ]]
    );
}

#[test]
#[ignore = "needs Herd's MySQL on 127.0.0.1:3306"]
fn mysql_says_which_values_are_numbers_and_leaves_decimals_as_text() {
    let outcome = herd("mysql", 3306, "mysql")
        .run("select 7, 1.5e0, cast(2.50 as decimal(4, 2)), 'x'")
        .unwrap();

    assert_eq!(
        outcome.kinds,
        [[Kind::Integer, Kind::Float, Kind::Text, Kind::Text]]
    );
}

#[test]
#[ignore = "needs Herd's Postgres on 127.0.0.1:5432"]
fn postgres_errors_are_in_postgres_own_words() {
    let failed = herd("pgsql", 5432, "postgres")
        .run("select * from nowhere_at_all")
        .unwrap_err();

    assert_eq!(
        failed.to_string(),
        "relation \"nowhere_at_all\" does not exist"
    );
}

#[test]
fn sqlite_refuses_a_write_hidden_in_a_with_when_asked_to_only_read() {
    let mut database = sqlite("hidden-write");
    database.run("create table counted (x integer)").unwrap();
    database.run("insert into counted values (1)").unwrap();

    let refused = database
        .run_reading("with c as (select 1) update counted set x = 2")
        .unwrap_err();

    assert_eq!(refused.to_string(), "attempt to write a readonly database");
    assert_eq!(
        database.run_reading("select x from counted").unwrap().rows,
        [[Some("1".to_string())]]
    );
    database.run("update counted set x = 3").unwrap();
}

#[test]
#[ignore = "needs Herd's Postgres on 127.0.0.1:5432"]
fn postgres_refuses_a_write_hidden_in_a_with_when_asked_to_only_read() {
    let mut database = herd("pgsql", 5432, "postgres");
    database
        .run("drop table if exists tql_reading_check")
        .unwrap();
    database
        .run("create table tql_reading_check (x integer)")
        .unwrap();
    database
        .run("insert into tql_reading_check values (1)")
        .unwrap();

    let refused = database
        .run_reading("with gone as (delete from tql_reading_check returning *) select * from gone")
        .unwrap_err();
    let left = database
        .run("select count(*) from tql_reading_check")
        .unwrap();
    database.run("drop table tql_reading_check").unwrap();

    assert!(
        refused.to_string().contains("read-only transaction"),
        "{refused}"
    );
    assert_eq!(left.rows, [[Some("1".to_string())]]);
}

#[test]
#[ignore = "needs Herd's MySQL on 127.0.0.1:3306"]
fn mysql_refuses_a_write_hidden_in_a_with_when_asked_to_only_read() {
    let mut database = herd("mysql", 3306, "mysql");
    database
        .run("drop database if exists tql_reading_check")
        .unwrap();
    database.run("create database tql_reading_check").unwrap();
    database.use_database("tql_reading_check").unwrap();
    database
        .run("create table counted (x integer) engine = innodb")
        .unwrap();
    database.run("insert into counted values (1)").unwrap();

    let refused = database
        .run_reading("with c as (select 1) update counted set x = 2")
        .unwrap_err();
    let left = database.run("select x from counted").unwrap();
    database.run("drop database tql_reading_check").unwrap();

    assert!(refused.to_string().contains("READ ONLY"), "{refused}");
    assert_eq!(left.rows, [[Some("1".to_string())]]);
}

fn counts_the_rows_each_write_reached(database: &mut dyn Database, table: &str) {
    assert_eq!(
        database
            .update(table, "name", "id", "1", Some("after"))
            .unwrap(),
        1
    );
    assert_eq!(
        database
            .update(table, "name", "id", "1", Some("after"))
            .unwrap(),
        1
    );
    assert_eq!(
        database
            .update(table, "name", "id", "999", Some("after"))
            .unwrap(),
        0
    );
    assert_eq!(
        database
            .delete(table, "id", &["1".to_string(), "999".to_string()])
            .unwrap(),
        1
    );
    assert_eq!(
        database.delete(table, "id", &["999".to_string()]).unwrap(),
        0
    );
}

#[test]
fn sqlite_counts_the_rows_each_write_reached() {
    let mut database = sqlite("reached");
    database
        .run("create table reached (id integer primary key, name text)")
        .unwrap();
    database
        .run("insert into reached values (1, 'before')")
        .unwrap();

    counts_the_rows_each_write_reached(database.as_mut(), "reached");
}

#[test]
#[ignore = "needs Herd's Postgres on 127.0.0.1:5432"]
fn postgres_counts_the_rows_each_write_reached() {
    let mut database = herd("pgsql", 5432, "postgres");
    database.run("drop table if exists tql_reached").unwrap();
    database
        .run("create table tql_reached (id integer primary key, name text)")
        .unwrap();
    database
        .run("insert into tql_reached values (1, 'before')")
        .unwrap();

    counts_the_rows_each_write_reached(database.as_mut(), "tql_reached");
    database.run("drop table tql_reached").unwrap();
}

#[test]
#[ignore = "needs Herd's MySQL on 127.0.0.1:3306"]
fn mysql_counts_the_rows_each_write_reached_even_when_the_value_is_unchanged() {
    let mut database = herd("mysql", 3306, "mysql");
    database.run("drop database if exists tql_reached").unwrap();
    database.run("create database tql_reached").unwrap();
    database.use_database("tql_reached").unwrap();
    database
        .run("create table reached (id integer primary key, name text)")
        .unwrap();
    database
        .run("insert into reached values (1, 'before')")
        .unwrap();

    counts_the_rows_each_write_reached(database.as_mut(), "reached");
    database.run("drop database tql_reached").unwrap();
}

#[test]
#[ignore = "needs Herd's Postgres on 127.0.0.1:5432"]
fn postgres_shows_the_first_result_when_several_statements_return_rows() {
    let mut database = herd("pgsql", 5432, "postgres");

    let outcome = database
        .run("select 1 as a, 2 as b; select 3 as c")
        .unwrap();

    assert_eq!(outcome.headers, ["a", "b"]);
    assert_eq!(
        outcome.rows,
        [[Some("1".to_string()), Some("2".to_string())]]
    );
}

#[test]
#[ignore = "needs Herd's MySQL on 127.0.0.1:3306"]
fn mysql_shows_dates_and_times_the_same_with_or_without_a_filter() {
    let mut database = herd("mysql", 3306, "mysql");
    database.run("drop database if exists tql_moments").unwrap();
    database.run("create database tql_moments").unwrap();
    database.use_database("tql_moments").unwrap();
    database
        .run(
            "create table moments (id integer primary key, day date, clock time(3), stamp datetime(6), plain datetime)",
        )
        .unwrap();
    database
        .run("insert into moments values (1, '2024-03-05', '12:34:56.789', '2024-03-05 01:02:03.000450', '2024-03-05 01:02:03')")
        .unwrap();

    let unfiltered = database
        .select("select day, clock, stamp, plain from moments", &[])
        .unwrap();
    let filtered = database
        .select(
            "select day, clock, stamp, plain from moments where id = ?",
            &["1".to_string()],
        )
        .unwrap();
    database.run("drop database tql_moments").unwrap();

    let expected = [[
        Some("2024-03-05".to_string()),
        Some("12:34:56.789".to_string()),
        Some("2024-03-05 01:02:03.000450".to_string()),
        Some("2024-03-05 01:02:03".to_string()),
    ]];
    assert_eq!(unfiltered.rows, expected);
    assert_eq!(filtered.rows, expected);
}

#[test]
#[ignore = "needs Herd's Postgres on 127.0.0.1:5432"]
fn postgres_lists_every_column_of_a_composite_key_in_key_order() {
    let mut database = herd("pgsql", 5432, "postgres");
    database.run("drop table if exists tql_pairs").unwrap();
    database
        .run("create table tql_pairs (a integer, b integer, primary key (b, a))")
        .unwrap();

    let keys = database.key_columns().unwrap();
    let single = database.primary_keys().unwrap();
    database.run("drop table tql_pairs").unwrap();

    assert_eq!(keys["tql_pairs"], ["b", "a"]);
    assert!(!single.contains_key("tql_pairs"));
}

#[test]
#[ignore = "needs Herd's MySQL on 127.0.0.1:3306"]
fn mysql_lists_every_column_of_a_composite_key_in_key_order() {
    let mut database = herd("mysql", 3306, "mysql");
    database.run("drop database if exists tql_pairs").unwrap();
    database.run("create database tql_pairs").unwrap();
    database.use_database("tql_pairs").unwrap();
    database
        .run("create table pairs (a integer, b integer, primary key (b, a))")
        .unwrap();

    let keys = database.key_columns().unwrap();
    database.run("drop database tql_pairs").unwrap();

    assert_eq!(keys["pairs"], ["b", "a"]);
}

#[test]
#[ignore = "needs Herd's MySQL on 127.0.0.1:3306"]
fn mysql_shows_bits_as_numbers_and_writes_them_back_as_numbers() {
    let mut database = herd("mysql", 3306, "mysql");
    database.run("drop database if exists tql_bits").unwrap();
    database.run("create database tql_bits").unwrap();
    database.use_database("tql_bits").unwrap();
    database
        .run("create table flags (id integer primary key, mask bit(8), flag bit(1))")
        .unwrap();
    database
        .run("insert into flags values (1, b'01000001', b'1')")
        .unwrap();

    let shown = database
        .select("select mask, flag from flags", &[])
        .unwrap();
    let filtered = database
        .select(
            "select mask, flag from flags where id = ?",
            &["1".to_string()],
        )
        .unwrap();
    database
        .update("flags", "mask", "id", "1", Some("200"))
        .unwrap();
    database
        .insert(
            "flags",
            &vec![
                ("id".to_string(), Some("2".to_string())),
                ("mask".to_string(), Some("7".to_string())),
                ("flag".to_string(), Some("0".to_string())),
            ],
        )
        .unwrap();
    let written = database
        .run("select id, mask + 0, flag + 0 from flags order by id")
        .unwrap();
    database.run("drop database tql_bits").unwrap();

    let expected = [[Some("65".to_string()), Some("1".to_string())]];
    assert_eq!(shown.rows, expected);
    assert_eq!(filtered.rows, expected);
    assert_eq!(
        written.rows,
        [
            ["1", "200", "1"].map(|value| Some(value.to_string())),
            ["2", "7", "0"].map(|value| Some(value.to_string())),
        ]
    );
}
