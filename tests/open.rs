use std::fs;
use std::path::PathBuf;

use app::dsn;
use app::encrypter::Encrypter;
use app::opener::{self, Opened};
use app::store::Store;
use app::store_writer::StoreWriter;

struct Scratch {
    root: PathBuf,
}

impl Scratch {
    fn new(label: &str) -> Self {
        let root = std::env::temp_dir().join(format!("tql-open-{label}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        StoreWriter::prepare(&root.join("tql/tql.sqlite")).unwrap();
        Scratch { root }
    }

    fn sqlite_file(&self, name: &str) -> PathBuf {
        let path = self.root.join(name);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        let database = rusqlite::Connection::open(&path).unwrap();
        database
            .execute_batch(
                "create table widgets (id integer primary key, name text);
                 insert into widgets (name) values ('alpha');",
            )
            .unwrap();
        path
    }

    fn open(&self, path: &str, called: Option<&str>, tag: Option<&str>, peek: bool) -> Opened {
        opener::connection_for(
            &self.root.join("tql/tql.sqlite"),
            &self.root.join("tql/key"),
            opener::target(path).unwrap(),
            called,
            tag,
            peek,
        )
        .unwrap()
    }

    fn count(&self) -> usize {
        Store::open(&self.root.join("tql/tql.sqlite"))
            .unwrap()
            .connections()
            .unwrap()
            .len()
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

#[test]
fn recognises_a_sqlite_file_by_its_header() {
    let scratch = Scratch::new("header");
    let database = scratch.sqlite_file("app.sqlite");
    let text = scratch.root.join("notes.txt");
    fs::write(&text, "just some text").unwrap();

    assert!(opener::looks_like_sqlite(&database));
    assert!(!opener::looks_like_sqlite(&text));
}

#[test]
fn refuses_a_file_that_is_not_a_database() {
    let scratch = Scratch::new("not-a-database");
    let text = scratch.root.join("notes.txt");
    fs::write(&text, "just some text").unwrap();

    assert_eq!(
        opener::target(&text.display().to_string()),
        Err("notes.txt is not a SQLite database.".to_string())
    );
}

#[test]
fn refuses_a_file_that_is_not_there() {
    assert_eq!(
        opener::target("/tmp/definitely-not-here.sqlite"),
        Err("No such file: /tmp/definitely-not-here.sqlite".to_string())
    );
}

#[test]
fn refuses_a_scheme_it_does_not_understand() {
    assert_eq!(
        opener::target("redis://localhost"),
        Err("That connection string is not one I understand.".to_string())
    );
}

#[test]
fn takes_a_sql_server_connection_string() {
    for scheme in ["sqlsrv", "mssql"] {
        let parsed = opener::target(&format!("{scheme}://sa:pw@db.example.com/shop")).unwrap();

        assert_eq!(parsed.driver, "sqlsrv");
        assert_eq!(parsed.host.as_deref(), Some("db.example.com"));
        assert_eq!(parsed.port, Some(1433));
        assert_eq!(parsed.database.as_deref(), Some("shop"));
        assert_eq!(parsed.username.as_deref(), Some("sa"));
    }
}

#[test]
fn says_so_when_a_tag_is_not_one_of_them() {
    assert_eq!(
        opener::refuse_tag(Some("scratch")).as_deref(),
        Some("[scratch] is not a tag. Try production, staging, dev, local.")
    );
    assert_eq!(opener::refuse_tag(Some("Staging")), None);
    assert_eq!(opener::refuse_tag(None), None);
}

#[test]
fn the_command_refuses_before_touching_any_store() {
    let scratch = Scratch::new("command");
    let text = scratch.root.join("notes.txt");
    fs::write(&text, "just some text").unwrap();

    let not_a_database = app::application().run_with(["open", &text.display().to_string()], "");
    assert_eq!(not_a_database.code, 1);
    assert!(
        format!("{}{}", not_a_database.stdout, not_a_database.stderr)
            .contains("notes.txt is not a SQLite database.")
    );

    let bad_tag = app::application().run_with(
        [
            "open",
            "mysql://alice:pw@scratch.test/shop",
            "--tag=scratch",
        ],
        "",
    );
    assert_eq!(bad_tag.code, 1);
    assert!(format!("{}{}", bad_tag.stdout, bad_tag.stderr).contains("[scratch] is not a tag"));
}

#[test]
fn remembers_a_file_it_opened() {
    let scratch = Scratch::new("file");
    let path = scratch.sqlite_file("shop.sqlite");

    let opened = scratch.open(&path.display().to_string(), None, None, false);

    assert_ne!(opened.connection.id, 0);
    assert_eq!(opened.connection.name, "shop.sqlite");
    assert_eq!(
        opened.connection.database,
        Some(fs::canonicalize(&path).unwrap().display().to_string())
    );
    assert_eq!(scratch.count(), 1);
}

#[test]
fn numbers_a_second_file_with_the_same_name() {
    let scratch = Scratch::new("same-name");
    let first = scratch.sqlite_file("one/database.sqlite");
    let second = scratch.sqlite_file("two/database.sqlite");

    scratch.open(&first.display().to_string(), None, None, false);
    let opened = scratch.open(&second.display().to_string(), None, None, false);

    assert_eq!(opened.connection.name, "database.sqlite (2)");
}

#[test]
fn peeks_without_remembering() {
    let scratch = Scratch::new("peek");
    let path = scratch.sqlite_file("peek.sqlite");

    let opened = scratch.open(&path.display().to_string(), None, None, true);

    assert_eq!(opened.connection.id, 0);
    assert_eq!(scratch.count(), 0);
}

#[test]
fn peeking_at_a_connection_string_keeps_its_password_usable() {
    let scratch = Scratch::new("peek-password");

    let opened = scratch.open("mysql://alice:pw@peek.test/shop", None, None, true);

    let payload = opened.connection.password.unwrap();
    assert_eq!(
        opened.encrypter.unwrap().decrypt_string(&payload).unwrap(),
        "pw"
    );
    assert_eq!(scratch.count(), 0);
}

#[test]
fn remembers_a_connection_string_without_being_asked() {
    let scratch = Scratch::new("dsn");

    let opened = scratch.open("mysql://alice:pw@example.test/shop", None, None, false);

    assert_eq!(opened.connection.name, "shop on example.test");
    assert_eq!(opened.connection.port, Some(3306));
    let payload = opened.connection.password.unwrap();
    assert_eq!(
        Encrypter::from_file(&scratch.root.join("tql/key"))
            .unwrap()
            .decrypt_string(&payload)
            .unwrap(),
        "pw"
    );
}

#[test]
fn reuses_a_saved_connection_rather_than_duplicating_it() {
    let scratch = Scratch::new("reuse");

    let first = scratch.open(
        "mysql://alice:pw@dupe.test/shop",
        Some("first"),
        None,
        false,
    );
    let second = scratch.open(
        "mysql://alice:rotated@dupe.test/shop",
        Some("first"),
        None,
        false,
    );

    assert_eq!(second.connection.id, first.connection.id);
    assert_eq!(scratch.count(), 1);
}

#[test]
fn names_it_what_name_says() {
    let scratch = Scratch::new("named");

    let opened = scratch.open(
        "mysql://alice:pw@named.test/shop",
        Some("the shop"),
        None,
        false,
    );

    assert_eq!(opened.connection.name, "the shop");
}

#[test]
fn tags_it_what_tag_says_and_keeps_the_name_from_the_database() {
    let scratch = Scratch::new("tagged");

    let opened = scratch.open(
        "mysql://alice:pw@tagged.test/shop",
        None,
        Some("staging"),
        false,
    );

    assert_eq!(opened.connection.tag.as_deref(), Some("staging"));
    assert_eq!(opened.connection.name, "shop on tagged.test");
}

#[test]
fn renames_an_existing_connection_rather_than_duplicating_it() {
    let scratch = Scratch::new("rename");

    let first = scratch.open(
        "mysql://alice:pw@rename.test/shop",
        Some("old"),
        None,
        false,
    );
    let second = scratch.open(
        "mysql://alice:pw@rename.test/shop",
        Some("new"),
        Some("dev"),
        false,
    );

    assert_eq!(second.connection.id, first.connection.id);
    assert_eq!(second.connection.name, "new");
    assert_eq!(second.connection.tag.as_deref(), Some("dev"));
    assert_eq!(scratch.count(), 1);
}

#[test]
fn a_sqlite_url_reads_the_same_as_its_path() {
    let parsed = dsn::parse("sqlite:///tmp/app.sqlite").unwrap();

    assert_eq!(parsed.database.as_deref(), Some("/tmp/app.sqlite"));
}
