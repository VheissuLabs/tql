use app::database::{self, Database, Kind};
use app::store::Connection;

fn sqlite() -> Box<dyn Database> {
    let path = std::env::temp_dir().join(format!("tql-kinds-{}.sqlite", std::process::id()));
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
    let outcome = sqlite().run("select 7, 1.5, '8', null").unwrap();

    assert_eq!(
        outcome.kinds,
        [[Kind::Integer, Kind::Float, Kind::Text, Kind::Text]]
    );
}

#[test]
#[ignore = "needs Herd's Postgres on 127.0.0.1:5432"]
fn postgres_says_which_values_are_whole_numbers_and_booleans() {
    let outcome = herd("pgsql", 5432, "postgres")
        .run("select 7::int, true, false, 1.5::numeric, 'x'::text")
        .unwrap();

    assert_eq!(
        outcome.rows,
        [[
            Some("7".to_string()),
            Some("true".to_string()),
            Some("false".to_string()),
            Some("1.5".to_string()),
            Some("x".to_string()),
        ]]
    );
    assert_eq!(
        outcome.kinds,
        [[
            Kind::Integer,
            Kind::Boolean,
            Kind::Boolean,
            Kind::Text,
            Kind::Text
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
