use std::fs;
use std::path::PathBuf;

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

#[test]
fn a_fresh_store_has_tqls_tables_and_migrations() {
    let scratch = Scratch::new("fresh");

    assert_eq!(
        scratch.raw("select cast(count(*) as text) from migrations"),
        Some("7".into())
    );
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
