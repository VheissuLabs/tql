use std::fs;
use std::path::PathBuf;

use app::driver::Driver;
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
fn opens_a_duckdb_file_as_duckdb() {
    let scratch = Scratch::new("duckdb");
    let path = scratch.root.join("warehouse.duckdb");
    duckdb::Connection::open(&path)
        .unwrap()
        .execute_batch("create table t (id integer)")
        .unwrap();

    assert!(opener::looks_like_duckdb(&path));
    assert!(!opener::looks_like_sqlite(&path));
    let target = opener::target(&path.display().to_string()).unwrap();
    assert_eq!(target.driver, Driver::Duckdb);
    assert_eq!(target.name, "warehouse.duckdb");
}

#[test]
fn refuses_a_file_that_is_not_a_database() {
    let scratch = Scratch::new("not-a-database");
    let text = scratch.root.join("notes.txt");
    fs::write(&text, "just some text").unwrap();

    assert_eq!(
        opener::target(&text.display().to_string()).map_err(|error| error.to_string()),
        Err("notes.txt is not a SQLite or DuckDB database.".to_string())
    );
}

#[test]
fn refuses_a_file_that_is_not_there() {
    assert_eq!(
        opener::target("/tmp/definitely-not-here.sqlite").map_err(|error| error.to_string()),
        Err("No such file: /tmp/definitely-not-here.sqlite".to_string())
    );
}

#[test]
fn refuses_a_scheme_it_does_not_understand() {
    assert_eq!(
        opener::target("mongodb://localhost").map_err(|error| error.to_string()),
        Err("That connection string is not one I understand.".to_string())
    );
}

#[test]
fn takes_a_sql_server_connection_string() {
    for scheme in ["sqlsrv", "mssql"] {
        let parsed = opener::target(&format!("{scheme}://sa:pw@db.example.com/shop")).unwrap();

        assert_eq!(parsed.driver, Driver::Sqlsrv);
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
            .contains("notes.txt is not a SQLite or DuckDB database.")
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

#[test]
fn saves_the_tunnel_and_tag_from_a_tableplus_url() {
    let scratch = Scratch::new("tableplus");
    let opened = scratch.open(
        "mysql+ssh://deploy:hop@bastion.example.com/dbuser:pw@127.0.0.1/shop?env=prod",
        None,
        None,
        false,
    );

    let connection = &opened.connection;
    assert_eq!(connection.name, "shop on bastion.example.com");
    assert_eq!(connection.ssh_host.as_deref(), Some("bastion.example.com"));
    assert_eq!(connection.ssh_user.as_deref(), Some("deploy"));
    assert_eq!(connection.tag.as_deref(), Some("production"));
    let encrypter = opened.encrypter.unwrap();
    assert_eq!(
        encrypter
            .decrypt_string(connection.ssh_password.as_ref().unwrap())
            .unwrap(),
        "hop"
    );
}

#[test]
fn a_tag_given_on_the_command_line_wins_over_the_urls() {
    let scratch = Scratch::new("tag-wins");
    let opened = scratch.open("mysql://u@h/db?env=prod", None, Some("dev"), false);

    assert_eq!(opened.connection.tag.as_deref(), Some("dev"));
}

#[test]
fn keeps_two_servers_behind_different_tunnels_apart() {
    let scratch = Scratch::new("tunnels");
    let one = scratch.open(
        "mysql+ssh://me@one.example.com/root@127.0.0.1/shop",
        None,
        None,
        false,
    );
    let again = scratch.open(
        "mysql+ssh://me@one.example.com/root@127.0.0.1/shop",
        None,
        None,
        false,
    );
    let other = scratch.open(
        "mysql+ssh://me@two.example.com/root@127.0.0.1/shop",
        None,
        None,
        false,
    );
    let direct = scratch.open("mysql://root@127.0.0.1/shop", None, None, false);

    assert_eq!(one.connection.id, again.connection.id);
    assert_ne!(one.connection.id, other.connection.id);
    assert_ne!(one.connection.id, direct.connection.id);
    assert_ne!(other.connection.id, direct.connection.id);
    assert_eq!(scratch.count(), 3);
}

#[test]
fn peeks_through_a_tunnel_without_saving_it() {
    let scratch = Scratch::new("peek-ssh");
    let opened = scratch.open(
        "mysql+ssh://me:hop@jump/root@127.0.0.1/shop",
        None,
        None,
        true,
    );

    assert_eq!(scratch.count(), 0);
    assert_eq!(opened.connection.ssh_host.as_deref(), Some("jump"));
    assert_eq!(
        opened
            .encrypter
            .unwrap()
            .decrypt_string(opened.connection.ssh_password.as_ref().unwrap())
            .unwrap(),
        "hop"
    );
}

#[test]
fn connect_is_another_name_for_open() {
    let scratch = Scratch::new("connect");
    let text = scratch.root.join("notes.txt");
    fs::write(&text, "just some text").unwrap();

    let connected = app::application().run_with(["connect", &text.display().to_string()], "");
    assert_eq!(connected.code, 1);
    assert!(
        format!("{}{}", connected.stdout, connected.stderr)
            .contains("notes.txt is not a SQLite or DuckDB database.")
    );

    let listed = app::application().run_with(["--help"], "");
    assert!(listed.stdout.contains("connect"), "{}", listed.stdout);
    assert_eq!(
        app::argv::rewrite(vec!["tql".into(), "connect".into()]),
        ["tql", "connect"]
    );
}

#[test]
fn browse_says_it_needs_a_terminal_rather_than_failing() {
    let browsed = app::application().run_with(["browse"], "");

    assert_eq!(browsed.code, 1);
    assert!(
        format!("{}{}", browsed.stdout, browsed.stderr)
            .contains("tql needs a terminal. Run it directly rather than piping into it."),
        "{browsed:?}"
    );
}

#[test]
fn an_empty_name_keeps_the_name_from_the_file_and_a_tag_is_kept_as_typed() {
    let scratch = Scratch::new("empty-name");
    let path = scratch.sqlite_file("shop.sqlite");

    let opened = scratch.open(
        &path.display().to_string(),
        Some(""),
        Some("Production"),
        false,
    );

    assert_eq!(opened.connection.name, "shop.sqlite");
    assert_eq!(opened.connection.tag.as_deref(), Some("Production"));
}
