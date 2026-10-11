use std::sync::atomic::{AtomicUsize, Ordering};

use app::database::{self, Database, Direction, Filter, Filters, Joiner, Pivot, Relations, Sort};
use app::driver::Driver;
use app::encrypter::Encrypter;
use app::store::Connection;

fn server(database: &str, ssl_mode: Option<&str>) -> Box<dyn Database> {
    let place = std::env::var("TQL_TEST_SQLSRV").expect("TQL_TEST_SQLSRV is host:port");
    let (host, port) = place
        .rsplit_once(':')
        .expect("TQL_TEST_SQLSRV is host:port");
    let encrypter = Encrypter::throwaway().unwrap();
    let password = std::env::var("TQL_TEST_SQLSRV_PASSWORD").unwrap();
    let connection = Connection {
        name: "mssql".into(),
        driver: Driver::Sqlsrv,
        host: Some(host.into()),
        port: Some(port.parse().unwrap()),
        database: Some(database.into()),
        username: Some("sa".into()),
        password: Some(encrypter.encrypt_string(&password).unwrap()),
        ssl_mode: ssl_mode.map(String::from),
        ..Connection::sqlite("mssql", "")
    };
    database::connect(&connection, Some(&encrypter)).unwrap()
}

struct Scratch {
    name: String,
    database: Box<dyn Database>,
}

impl Scratch {
    fn new() -> Self {
        static MADE: AtomicUsize = AtomicUsize::new(0);
        let name = format!(
            "tql_rust_test_{}_{}",
            std::process::id(),
            MADE.fetch_add(1, Ordering::Relaxed)
        );
        let mut master = server("master", None);
        master
            .run_server_wide(&format!("create database [{name}]"))
            .unwrap();
        let mut database = server("master", None);
        database.use_database(&name).unwrap();
        database
            .run(
                "create table customers (id int identity primary key, name nvarchar(100) not null default N'nobody', \
                 joined datetime2(0), balance decimal(8, 2), active bit default 1, photo varbinary(10));\
                 create table tags (id int primary key, label varchar(30) unique);\
                 create table customer_tag (customer_id int references customers(id), tag_id int references tags(id), created_at datetime);\
                 create table orders (id int primary key, customer_id int not null references customers(id), total money);\
                 create index orders_customer on orders (customer_id);",
            )
            .unwrap();
        database
            .run(
                "insert into customers (name, joined, balance, active, photo) values \
                 (N'Ada', '2024-01-02 03:04:05', 0.50, 1, 0x01AB), (N'Bob', null, -12.25, 0, null), (N'Cy', null, 3, 1, null);\
                 insert into tags values (1, 'vip'), (2, 'new');\
                 insert into customer_tag values (1, 1, null), (1, 2, null);\
                 insert into orders values (10, 1, 12.5), (11, 1, 3), (12, 2, 1);",
            )
            .unwrap();
        Scratch { name, database }
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let mut master = server("master", None);
        let _ = master.run_server_wide(&format!(
            "alter database [{0}] set single_user with rollback immediate; drop database [{0}]",
            self.name
        ));
    }
}

fn texts(row: &[Option<String>]) -> Vec<&str> {
    row.iter()
        .map(|value| value.as_deref().unwrap_or("NULL"))
        .collect()
}

#[test]
#[ignore = "needs a SQL Server: set TQL_TEST_SQLSRV=host:port and TQL_TEST_SQLSRV_PASSWORD for sa"]
fn lists_tables_and_databases_and_switches_between_them() {
    let mut scratch = Scratch::new();

    assert_eq!(
        scratch.database.tables().unwrap(),
        ["customer_tag", "customers", "orders", "tags"]
    );
    let databases = scratch.database.databases().unwrap();
    assert!(databases.contains(&"master".to_string()), "{databases:?}");
    assert!(databases.contains(&scratch.name), "{databases:?}");

    scratch.database.use_database("master").unwrap();
    assert!(
        !scratch
            .database
            .tables()
            .unwrap()
            .contains(&"customers".to_string())
    );
}

#[test]
#[ignore = "needs a SQL Server: set TQL_TEST_SQLSRV=host:port and TQL_TEST_SQLSRV_PASSWORD for sa"]
fn pages_sorts_and_filters_rows() {
    let mut scratch = Scratch::new();

    let first = scratch
        .database
        .filtered_rows("customers", 2, 0, None, None)
        .unwrap();
    assert_eq!(
        first.statement,
        "select * from [customers] order by (select null) offset 0 rows fetch next 2 rows only"
    );
    assert_eq!(
        first.headers,
        ["id", "name", "joined", "balance", "active", "photo"]
    );
    assert_eq!(
        texts(&first.rows[0]),
        ["1", "Ada", "2024-01-02 03:04:05", "0.50", "1", "0x01AB"]
    );
    assert_eq!(first.rows.len(), 2);

    let sort = Sort {
        column: "balance".into(),
        direction: Direction::Desc,
    };
    let sorted = scratch
        .database
        .filtered_rows("customers", 10, 1, Some(&sort), None)
        .unwrap();
    assert_eq!(
        sorted
            .rows
            .iter()
            .map(|row| row[1].clone().unwrap())
            .collect::<Vec<_>>(),
        ["Ada", "Bob"]
    );

    let filters = Filters::new(
        vec![
            Filter::new("name", "contains", "y"),
            Filter::new("id", "is one of", "1, 3"),
        ],
        Joiner::Or,
    );
    let filtered = scratch
        .database
        .filtered_rows("customers", 10, 0, None, Some(&filters))
        .unwrap();
    assert_eq!(filtered.rows.len(), 2);
    assert_eq!(
        filtered.statement,
        "select * from [customers] where [name] like '%y%' or [id] in (1, 3) order by (select null) offset 0 rows fetch next 10 rows only"
    );

    let empty = scratch
        .database
        .filtered_rows(
            "customers",
            10,
            0,
            None,
            Some(&Filters::new(
                vec![Filter::new("name", "is", "O'Brien")],
                Joiner::And,
            )),
        )
        .unwrap();
    assert!(empty.rows.is_empty());
    assert_eq!(empty.headers.len(), 6);
}

#[test]
#[ignore = "needs a SQL Server: set TQL_TEST_SQLSRV=host:port and TQL_TEST_SQLSRV_PASSWORD for sa"]
fn reads_the_schema_the_way_laravel_does() {
    let mut scratch = Scratch::new();

    let columns = scratch.database.columns("customers").unwrap();
    assert_eq!(columns[0].name, "id");
    assert_eq!(columns[0].type_name, "int");
    assert!(columns[0].auto_increment);
    assert!(!columns[0].nullable);
    assert_eq!(columns[1].default.as_deref(), Some("(N'nobody')"));
    assert_eq!(columns[4].default.as_deref(), Some("((1))"));
    assert!(columns[2].nullable);

    let indexes = scratch.database.indexes("orders").unwrap();
    let primary = indexes.iter().find(|index| index.primary).unwrap();
    assert_eq!(primary.columns, ["id"]);
    assert!(primary.unique);
    assert!(primary.name.starts_with("pk__orders"));
    let secondary = indexes
        .iter()
        .find(|index| index.name == "orders_customer")
        .unwrap();
    assert_eq!(secondary.columns, ["customer_id"]);
    assert!(!secondary.unique);

    let keys = scratch.database.primary_keys().unwrap();
    assert_eq!(keys.get("customers").map(String::as_str), Some("id"));
    assert_eq!(keys.get("customer_tag"), None);
    assert_eq!(scratch.database.key_columns().unwrap()["customers"], ["id"]);

    let foreign = scratch.database.foreign_keys().unwrap();
    let pivot = &foreign["customer_tag"];
    assert_eq!(pivot.len(), 2);
    assert!(
        pivot.iter().any(|key| key.table == "tags"
            && key.columns == ["tag_id"]
            && key.references == ["id"])
    );
    assert_eq!(foreign["orders"][0].table, "customers");
}

#[test]
#[ignore = "needs a SQL Server: set TQL_TEST_SQLSRV=host:port and TQL_TEST_SQLSRV_PASSWORD for sa"]
fn changes_adds_and_deletes_rows() {
    let mut scratch = Scratch::new();

    assert_eq!(
        scratch
            .database
            .update("customers", "name", "id", "2", Some("Bobby"))
            .unwrap(),
        1
    );
    assert_eq!(
        scratch
            .database
            .update("customers", "name", "id", "999", Some("Bobby"))
            .unwrap(),
        0
    );
    scratch
        .database
        .update("customers", "joined", "id", "2", None)
        .unwrap();
    scratch.database.insert("customers", &Vec::new()).unwrap();
    scratch
        .database
        .insert(
            "tags",
            &vec![
                ("id".to_string(), Some("3".to_string())),
                ("label".to_string(), None),
            ],
        )
        .unwrap();
    assert_eq!(
        scratch.database.highest("customers", "id").unwrap(),
        Some("4".to_string())
    );

    let page = scratch
        .database
        .select(
            "select name, active from customers where id in (?, ?) order by id",
            &["2".to_string(), "4".to_string()],
        )
        .unwrap();
    assert_eq!(texts(&page.rows[0]), ["Bobby", "0"]);
    assert_eq!(texts(&page.rows[1]), ["nobody", "1"]);

    scratch
        .database
        .delete("tags", "id", &["2".to_string(), "3".to_string()])
        .unwrap_err();
    scratch
        .database
        .delete("customer_tag", "tag_id", &["2".to_string()])
        .unwrap();
    scratch
        .database
        .delete("tags", "id", &["2".to_string(), "3".to_string()])
        .unwrap();
    assert_eq!(scratch.database.tables().unwrap().len(), 4);
    assert_eq!(
        scratch
            .database
            .select("select count(*) from tags", &[])
            .unwrap()
            .rows[0][0]
            .as_deref(),
        Some("1")
    );
}

#[test]
#[ignore = "needs a SQL Server: set TQL_TEST_SQLSRV=host:port and TQL_TEST_SQLSRV_PASSWORD for sa"]
fn runs_statements_and_counts_what_they_changed() {
    let mut scratch = Scratch::new();

    let changed = scratch
        .database
        .run("update orders set total = total + 1 where customer_id = 1")
        .unwrap();
    assert_eq!(changed.affected, Some(2));
    assert!(changed.headers.is_empty());

    let read = scratch
        .database
        .run("select id, total from orders order by id")
        .unwrap();
    assert_eq!(read.affected, None);
    assert_eq!(texts(&read.rows[0]), ["10", "13.5000"]);

    let error = scratch.database.run("select * from nowhere").unwrap_err();
    assert_eq!(error.to_string(), "Invalid object name 'nowhere'.");

    let scalars = scratch
        .database
        .scalars("select id, name, balance, photo from customers where id = 1")
        .unwrap();
    assert_eq!(
        scalars.rows[0]
            .iter()
            .map(|value| value.literal(Driver::Sqlsrv))
            .collect::<Vec<_>>(),
        ["1", "N'Ada'", "0.50", "0x01ab"]
    );
}

#[test]
#[ignore = "needs a SQL Server: set TQL_TEST_SQLSRV=host:port and TQL_TEST_SQLSRV_PASSWORD for sa"]
fn follows_links_with_offset_and_fetch() {
    let mut scratch = Scratch::new();

    let related = scratch
        .database
        .related("orders", "customer_id", "1", 1)
        .unwrap();
    assert_eq!(related.rows.len(), 2);

    let far = Pivot {
        table: "tags".into(),
        on: "tag_id".into(),
        references: "id".into(),
    };
    let through = scratch
        .database
        .through("customer_tag", "customer_id", &far, "1", 5)
        .unwrap();
    assert_eq!(through.rows.len(), 2);

    let counts = scratch.database.counts(&[app::database::Count {
        table: "orders".into(),
        column: "customer_id".into(),
        value: "1".into(),
    }]);
    assert_eq!(counts, [Some(2)]);
}

#[test]
#[ignore = "needs a SQL Server: set TQL_TEST_SQLSRV=host:port and TQL_TEST_SQLSRV_PASSWORD for sa"]
fn connects_unencrypted_when_ssl_is_disabled_and_refuses_an_unverified_certificate() {
    let mut plain = server("master", Some("disable"));
    assert_eq!(
        plain.select("select 1", &[]).unwrap().rows[0][0].as_deref(),
        Some("1")
    );

    let place = std::env::var("TQL_TEST_SQLSRV").unwrap();
    let (host, port) = place.rsplit_once(':').unwrap();
    let verified = Connection {
        name: "mssql".into(),
        driver: Driver::Sqlsrv,
        host: Some(host.into()),
        port: Some(port.parse().unwrap()),
        username: Some("sa".into()),
        ssl_mode: Some("verify-full".into()),
        ..Connection::sqlite("mssql", "")
    };
    let refused = database::connect(&verified, None).err().unwrap();
    assert!(
        format!("{refused:#}").contains("cannot connect to sqlsrv://sa@"),
        "{refused:#}"
    );
}

#[test]
#[ignore = "needs a SQL Server: set TQL_TEST_SQLSRV=host:port and TQL_TEST_SQLSRV_PASSWORD for sa"]
fn a_write_hidden_in_a_with_is_refused_when_asked_to_only_read() {
    let mut scratch = Scratch::new();

    let refused = scratch
        .database
        .run_reading("with c as (select 1 as one) update tags set label = 'gone' where id = 1")
        .unwrap_err();
    let left = scratch
        .database
        .run_reading("select label from tags where id = 1")
        .unwrap();

    assert!(
        refused
            .to_string()
            .contains("only statements that read run here"),
        "{refused}"
    );
    assert_eq!(texts(&left.rows[0]), ["vip"]);
}

#[test]
#[ignore = "needs a SQL Server: set TQL_TEST_SQLSRV=host:port and TQL_TEST_SQLSRV_PASSWORD for sa"]
fn refuses_a_write_that_returns_rows_when_asked_to_only_read() {
    let mut scratch = Scratch::new();

    for statement in [
        "with c as (select 1 as one) update tags set label = 'gone' output inserted.label where id = 1",
        "select * from (select 1 as one) a; update tags set label = 'gone'",
        "select * into tags_copy from tags",
    ] {
        let refused = scratch.database.run_reading(statement).unwrap_err();
        assert!(
            refused
                .to_string()
                .contains("only statements that read run here"),
            "{statement}: {refused}"
        );
    }
    let left = scratch
        .database
        .run_reading("select label from tags where id = 1")
        .unwrap();

    assert_eq!(texts(&left.rows[0]), ["vip"]);
}
