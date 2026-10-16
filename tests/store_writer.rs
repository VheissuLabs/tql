use std::fs;
use std::path::{Path, PathBuf};

use app::encrypter::Encrypter;
use app::store::Store;
use app::store_writer::{Field, StoreWriter};

struct Scratch {
    directory: PathBuf,
}

impl Scratch {
    fn new(label: &str) -> Self {
        let directory =
            std::env::temp_dir().join(format!("tql-writer-{label}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&directory);
        StoreWriter::prepare(&directory.join("tql/tql.sqlite")).unwrap();
        Scratch { directory }
    }

    fn store(&self) -> PathBuf {
        self.directory.join("tql/tql.sqlite")
    }

    fn key(&self) -> PathBuf {
        self.directory.join("tql/key")
    }

    fn writer(&self) -> StoreWriter {
        StoreWriter::open(&self.store(), &self.key()).unwrap()
    }

    fn raw(&self, sql: &str) -> Option<String> {
        rusqlite::Connection::open(self.store())
            .unwrap()
            .query_row(sql, [], |row| row.get(0))
            .unwrap()
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.directory);
    }
}

fn mysql(name: &str, password: &str) -> Vec<(&'static str, Field)> {
    vec![
        ("name", Field::Text(name.into())),
        ("driver", Field::Text("mysql".into())),
        ("host", Field::Text("db.test".into())),
        ("port", Field::Number(3306)),
        ("database", Field::Text("shop".into())),
        ("username", Field::Text("alice".into())),
        ("password", Field::Secret(password.into())),
        ("read_only", Field::Flag(true)),
    ]
}

const FRESH_COLUMNS: [&str; 29] = [
    "id",
    "name",
    "driver",
    "host",
    "port",
    "database",
    "username",
    "password",
    "read_only",
    "last_used_at",
    "created_at",
    "updated_at",
    "ssh_host",
    "ssh_port",
    "ssh_user",
    "ssh_key",
    "ssh_password",
    "ssl_mode",
    "ssl_key",
    "ssl_cert",
    "ssl_ca",
    "colour",
    "tag",
    "group_name",
    "warehouse",
    "role",
    "authenticator",
    "private_key",
    "session",
];

fn columns(store: &Path) -> Vec<String> {
    let connection = rusqlite::Connection::open(store).unwrap();
    let mut statement = connection
        .prepare("select name from pragma_table_info('connections') order by cid")
        .unwrap();
    statement
        .query_map([], |row| row.get(0))
        .unwrap()
        .map(Result::unwrap)
        .collect()
}

fn migrations(store: &Path) -> Vec<(String, i64)> {
    let connection = rusqlite::Connection::open(store).unwrap();
    let mut statement = connection
        .prepare("select migration, batch from migrations order by id")
        .unwrap();
    statement
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
        .unwrap()
        .map(Result::unwrap)
        .collect()
}

#[test]
fn a_fresh_store_has_tqls_tables_and_migrations() {
    let scratch = Scratch::new("fresh");

    assert_eq!(
        scratch.raw("select cast(count(*) as text) from migrations"),
        Some("10".into())
    );
    assert_eq!(columns(&scratch.store()), FRESH_COLUMNS);
    assert!(
        Store::open(&scratch.store())
            .unwrap()
            .connections()
            .unwrap()
            .is_empty()
    );
}

#[test]
fn creates_a_connection_with_timestamps_and_an_encrypted_password() {
    let scratch = Scratch::new("create");

    let id = scratch.writer().create(&mysql("shop", "hunter2")).unwrap();

    let saved = Store::open(&scratch.store())
        .unwrap()
        .connection(id)
        .unwrap()
        .unwrap();
    let payload = saved.password.clone().unwrap();
    assert_ne!(payload, "hunter2");
    assert_eq!(
        Encrypter::from_file(&scratch.key())
            .unwrap()
            .decrypt_string(&payload)
            .unwrap(),
        "hunter2"
    );
    assert!(saved.read_only);
    assert_eq!(saved.port, Some(3306));
    let created = scratch
        .raw(&format!(
            "select created_at from connections where id = {id}"
        ))
        .unwrap();
    assert_eq!(created.len(), "2026-01-01 00:00:00".len());
    assert_eq!(
        scratch.raw(&format!(
            "select updated_at from connections where id = {id}"
        )),
        Some(created)
    );
    assert_eq!(
        scratch.raw(&format!(
            "select last_used_at from connections where id = {id}"
        )),
        None
    );
}

#[test]
fn updates_only_what_changed() {
    let scratch = Scratch::new("update");
    let writer = scratch.writer();
    let id = writer.create(&mysql("shop", "hunter2")).unwrap();
    let payload = scratch.raw("select password from connections");
    rusqlite::Connection::open(scratch.store())
        .unwrap()
        .execute(
            "update connections set updated_at = '2020-01-01 00:00:00'",
            [],
        )
        .unwrap();

    assert!(!writer.update(id, &mysql("shop", "hunter2")).unwrap());
    assert_eq!(
        scratch.raw("select updated_at from connections"),
        Some("2020-01-01 00:00:00".into())
    );

    assert!(writer.update(id, &mysql("shop!", "hunter2")).unwrap());
    assert_eq!(
        scratch.raw("select name from connections"),
        Some("shop!".into())
    );
    assert_eq!(scratch.raw("select password from connections"), payload);
    assert_ne!(
        scratch.raw("select updated_at from connections"),
        Some("2020-01-01 00:00:00".into())
    );

    writer
        .update(
            id,
            &[("password", Field::Null), ("tag", Field::Text("".into()))],
        )
        .unwrap();
    assert_eq!(scratch.raw("select password from connections"), None);
    assert_eq!(scratch.raw("select tag from connections"), Some("".into()));
}

#[test]
fn deleting_a_connection_takes_its_history_with_it() {
    let scratch = Scratch::new("delete");
    let writer = scratch.writer();
    let gone = writer.create(&mysql("gone", "")).unwrap();
    let kept = writer.create(&mysql("kept", "")).unwrap();
    rusqlite::Connection::open(scratch.store())
        .unwrap()
        .execute(
            &format!(
                "insert into query_executions (connection_id, statement, source) values ({gone}, 'select 1', 'editor'), ({kept}, 'select 2', 'editor')"
            ),
            [],
        )
        .unwrap();

    assert_eq!(writer.delete(&[gone]).unwrap(), 1);

    assert_eq!(
        scratch.raw("select group_concat(name) from connections"),
        Some("kept".into())
    );
    assert_eq!(
        scratch.raw("select group_concat(statement) from query_executions"),
        Some("select 2".into())
    );
}

#[test]
fn touching_records_when_it_was_last_used() {
    let scratch = Scratch::new("touch");
    let writer = scratch.writer();
    let id = writer.create(&mysql("shop", "")).unwrap();

    writer.touch(id).unwrap();

    let used = Store::open(&scratch.store())
        .unwrap()
        .connection(id)
        .unwrap()
        .unwrap()
        .last_used_at;
    assert!(used.is_some());
}

#[test]
fn numbers_a_name_that_is_already_taken() {
    let scratch = Scratch::new("names");
    let writer = scratch.writer();
    let first = writer.create(&mysql("database.sqlite", "")).unwrap();

    assert!(writer.name_taken("database.sqlite", None).unwrap());
    assert!(!writer.name_taken("database.sqlite", Some(first)).unwrap());
    assert_eq!(
        writer.free_name("database.sqlite").unwrap(),
        "database.sqlite (2)"
    );
    writer.create(&mysql("database.sqlite (2)", "")).unwrap();
    assert_eq!(
        writer.free_name("database.sqlite").unwrap(),
        "database.sqlite (3)"
    );
    assert_eq!(writer.free_name("other").unwrap(), "other");
}

#[test]
fn finds_a_saved_connection_by_where_it_points_never_by_password() {
    let scratch = Scratch::new("matching");
    let writer = scratch.writer();
    let id = writer.create(&mysql("shop", "old")).unwrap();

    let identity = [
        ("driver", Field::Text("mysql".into())),
        ("host", Field::Text("db.test".into())),
        ("port", Field::Number(3306)),
        ("database", Field::Text("shop".into())),
        ("username", Field::Text("alice".into())),
        ("ssh_host", Field::Null),
    ];
    assert_eq!(writer.matching(&identity).unwrap(), Some(id));
    assert_eq!(
        writer
            .matching(&[("driver", Field::Text("pgsql".into()))])
            .unwrap(),
        None
    );
}

#[test]
fn a_store_from_before_groups_gains_the_group_column_in_a_new_batch() {
    let scratch = Scratch::new("before-groups");
    scratch.writer().create(&mysql("shop", "secret")).unwrap();
    rusqlite::Connection::open(scratch.store())
        .unwrap()
        .execute_batch(
            "alter table connections drop column group_name;
             alter table connections drop column warehouse;
             alter table connections drop column role;
             alter table connections drop column authenticator;
             alter table connections drop column private_key;
             alter table connections drop column session;
             delete from migrations where migration in (
                 '2026_10_07_000000_add_group_to_connections_table',
                 '2026_10_16_000000_add_warehouse_and_role_to_connections_table',
                 '2026_10_16_100000_add_snowflake_sign_in_to_connections_table'
             );",
        )
        .unwrap();

    StoreWriter::prepare(&scratch.store()).unwrap();

    assert_eq!(columns(&scratch.store()), FRESH_COLUMNS);
    assert_eq!(
        migrations(&scratch.store())[7..],
        [
            (
                "2026_10_07_000000_add_group_to_connections_table".to_string(),
                2
            ),
            (
                "2026_10_16_000000_add_warehouse_and_role_to_connections_table".to_string(),
                2
            ),
            (
                "2026_10_16_100000_add_snowflake_sign_in_to_connections_table".to_string(),
                2
            )
        ]
    );
    assert_eq!(
        scratch.raw("select name from connections"),
        Some("shop".into())
    );
}

#[test]
fn a_store_from_the_first_release_is_brought_all_the_way_forward() {
    let directory = std::env::temp_dir().join(format!("tql-writer-first-{}", std::process::id()));
    let _ = fs::remove_dir_all(&directory);
    fs::create_dir_all(&directory).unwrap();
    let store = directory.join("tql.sqlite");
    rusqlite::Connection::open(&store)
        .unwrap()
        .execute_batch(
            r#"CREATE TABLE "migrations" ("id" integer primary key autoincrement not null, "migration" varchar not null, "batch" integer not null);
            CREATE TABLE "connections" ("id" integer primary key autoincrement not null, "name" varchar not null, "driver" varchar not null, "host" varchar, "port" integer, "database" varchar, "username" varchar, "password" text, "color" varchar, "read_only" tinyint(1) not null default '0', "last_used_at" datetime, "created_at" datetime, "updated_at" datetime);
            CREATE UNIQUE INDEX "connections_name_unique" on "connections" ("name");
            CREATE TABLE "query_executions" ("id" integer primary key autoincrement not null, "connection_id" integer not null, "statement" text not null, "source" varchar not null, "succeeded" tinyint(1) not null default '1', "error" text, "row_count" integer, "duration_ms" integer, "created_at" datetime, "updated_at" datetime, foreign key("connection_id") references "connections"("id") on delete cascade);
            insert into "migrations" ("migration", "batch") values ('2026_09_22_051510_create_connections_table', 1);
            insert into "migrations" ("migration", "batch") values ('2026_09_22_051511_create_query_executions_table', 1);
            insert into connections (name, driver, color) values ('local', 'sqlite', 'red');"#,
        )
        .unwrap();

    StoreWriter::prepare(&store).unwrap();

    assert_eq!(columns(&store), FRESH_COLUMNS);
    let ran = migrations(&store);
    assert_eq!(ran.len(), 10);
    assert!(ran[2..].iter().all(|(_, batch)| *batch == 2));
    assert!(
        Store::open(&store)
            .unwrap()
            .connections()
            .unwrap()
            .iter()
            .any(|connection| connection.name == "local")
    );
    fs::remove_dir_all(&directory).unwrap();
}

#[test]
fn preparing_a_store_that_is_up_to_date_changes_nothing() {
    let scratch = Scratch::new("current");
    let before = migrations(&scratch.store());

    StoreWriter::prepare(&scratch.store()).unwrap();

    assert_eq!(migrations(&scratch.store()), before);
}
