use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use anyhow::bail;
use app::database::{Column, Database, Filters, ForeignKey, Index, Outcome, Page, Sort};
use app::store::Connection;
use app::tui::Browser;
use zerorust_tui::Fake;

const PALETTE: &str = "\x0b";
const DROP: &str = "\x04";
const ESCAPE: &str = "\x1b";
const UP: &str = "\x1b[A";

#[derive(Default)]
struct Ledger {
    databases: Vec<String>,
    created: Vec<String>,
    dropped: Vec<String>,
    dropped_from: Vec<String>,
    refuse: bool,
}

struct Server {
    current: String,
    ledger: Rc<RefCell<Ledger>>,
}

impl Database for Server {
    fn tables(&mut self) -> anyhow::Result<Vec<String>> {
        Ok(Vec::new())
    }

    fn filtered_rows(
        &mut self,
        _: &str,
        _: usize,
        _: usize,
        _: Option<&Sort>,
        _: Option<&Filters>,
    ) -> anyhow::Result<Page> {
        Ok(Page::default())
    }

    fn primary_keys(&mut self) -> anyhow::Result<HashMap<String, String>> {
        Ok(HashMap::new())
    }

    fn databases(&mut self) -> anyhow::Result<Vec<String>> {
        Ok(self.ledger.borrow().databases.clone())
    }

    fn use_database(&mut self, name: &str) -> anyhow::Result<()> {
        self.current = name.to_string();
        Ok(())
    }

    fn run(&mut self, statement: &str) -> anyhow::Result<Outcome> {
        let mut ledger = self.ledger.borrow_mut();
        if let Some(name) = statement.strip_prefix("create database ") {
            ledger.created.push(name.to_string());
            return Ok(Outcome::default());
        }
        if let Some(name) = statement.strip_prefix("drop database ") {
            if ledger.refuse {
                bail!("database is being accessed by other users");
            }
            let unquoted = name.trim_matches('`').to_string();
            ledger.databases.retain(|database| *database != unquoted);
            ledger.dropped.push(name.to_string());
            ledger.dropped_from.push(self.current.clone());
            return Ok(Outcome::default());
        }
        bail!("unexpected statement {statement}")
    }

    fn quote(&self, identifier: &str) -> String {
        format!("`{identifier}`")
    }

    fn select(&mut self, _: &str, _: &[String]) -> anyhow::Result<Page> {
        Ok(Page::default())
    }

    fn columns(&mut self, _: &str) -> anyhow::Result<Vec<Column>> {
        Ok(Vec::new())
    }

    fn indexes(&mut self, _: &str) -> anyhow::Result<Vec<Index>> {
        Ok(Vec::new())
    }

    fn foreign_keys(&mut self) -> anyhow::Result<HashMap<String, Vec<ForeignKey>>> {
        Ok(HashMap::new())
    }
}

fn server_with(
    databases: &[&str],
    configure: impl FnOnce(&mut Connection),
) -> (Fake<Browser>, Rc<RefCell<Ledger>>) {
    let mut connection = Connection {
        name: "server".into(),
        driver: "mysql".into(),
        database: Some("shop".into()),
        ..Connection::default()
    };
    configure(&mut connection);
    let ledger = Rc::new(RefCell::new(Ledger {
        databases: databases.iter().map(|name| name.to_string()).collect(),
        ..Ledger::default()
    }));
    let server = Server {
        current: connection.database.clone().unwrap_or_default(),
        ledger: ledger.clone(),
    };
    let browser = Browser::new(connection, Box::new(server)).unwrap();
    (Fake::sized(browser, 100, 30), ledger)
}

fn server(databases: &[&str]) -> (Fake<Browser>, Rc<RefCell<Ledger>>) {
    server_with(databases, |_| {})
}

fn command(fake: &mut Fake<Browser>, command: &str) {
    fake.press(&format!(":{command}\n"));
}

#[test]
fn creates_a_database_and_switches_to_it() {
    let (mut fake, ledger) = server(&["shop"]);

    command(&mut fake, "create database kmstools");

    assert_eq!(ledger.borrow().created, ["`kmstools`"]);
    assert_eq!(fake.app.active_database.as_deref(), Some("kmstools"));
    assert_eq!(fake.app.status(), "created kmstools · using it");
}

#[test]
fn takes_the_name_as_typed_quotes_and_all() {
    let (mut fake, ledger) = server(&["shop"]);

    command(&mut fake, "create database `kmstools`");

    assert_eq!(ledger.borrow().created, ["`kmstools`"]);
}

#[test]
fn asks_for_the_name_when_none_is_given() {
    let (mut fake, ledger) = server(&["shop"]);

    command(&mut fake, "create database");

    assert!(ledger.borrow().created.is_empty());
    assert_eq!(fake.app.command.as_deref(), Some("create database "));
}

#[test]
fn will_not_create_a_database_on_a_read_only_connection() {
    let (mut fake, ledger) = server_with(&["shop"], |connection| connection.read_only = true);

    command(&mut fake, "create database kmstools");

    assert!(ledger.borrow().created.is_empty());
    assert_eq!(fake.app.status(), "this connection is marked read-only");
    assert_eq!(fake.app.active_database.as_deref(), Some("shop"));
}

#[test]
fn will_not_create_or_drop_a_database_on_sqlite() {
    let path = std::env::temp_dir().join(format!(
        "tql-rust-create-drop-{}.sqlite",
        std::process::id()
    ));
    let _ = std::fs::remove_file(&path);
    rusqlite::Connection::open(&path)
        .unwrap()
        .execute_batch("create table widgets (id integer primary key)")
        .unwrap();
    let connection = Connection::sqlite("widgets", path.to_str().unwrap());
    let database = app::database::connect(&connection, None).unwrap();
    let mut fake = Fake::sized(Browser::new(connection, database).unwrap(), 100, 30);

    command(&mut fake, "create database kmstools");
    assert!(fake.app.status().contains("one file"));

    command(&mut fake, "drop database main");
    assert!(fake.app.status().contains("one file"));
}

#[test]
fn shows_the_error_when_the_server_refuses() {
    let (mut fake, ledger) = server(&["jobs", "shop"]);
    ledger.borrow_mut().refuse = true;

    command(&mut fake, "drop database jobs");

    assert!(fake.app.problem.is_some());
    assert_eq!(fake.app.problem_title, "THE DATABASE SAID NO");
    assert_eq!(fake.app.active_database.as_deref(), Some("shop"));
}

#[test]
fn offers_to_create_a_database_from_the_palette() {
    let (mut fake, _) = server(&["shop"]);

    fake.press(PALETTE);
    fake.press("create a database");
    fake.press("\n");

    assert_eq!(fake.app.command.as_deref(), Some("create database "));
}

#[test]
fn offers_to_create_the_database_typed_into_the_list() {
    let (mut fake, ledger) = server(&["jobs", "shop"]);
    fake.press("b");
    assert!(fake.app.databases.is_some());

    fake.press("kmstools");
    let screen = fake.screen();
    assert!(screen.contains("+ create kmstools"), "{screen}");
    assert!(screen.contains("ctrl+d Drop"), "{screen}");

    fake.press("\n");

    assert_eq!(ledger.borrow().created, ["`kmstools`"]);
    assert!(fake.app.databases.is_none());
    assert_eq!(fake.app.active_database.as_deref(), Some("kmstools"));
}

#[test]
fn picks_a_matching_database_before_offering_to_create_one() {
    let (mut fake, ledger) = server(&["jobs", "shop"]);
    fake.press("b");

    fake.press("jo\n");

    assert!(ledger.borrow().created.is_empty());
    assert_eq!(fake.app.active_database.as_deref(), Some("jobs"));
}

#[test]
fn a_read_only_list_offers_no_drop() {
    let (mut fake, _) = server_with(&["jobs", "shop"], |connection| connection.read_only = true);
    fake.press("b");

    let screen = fake.screen();
    assert!(!screen.contains("ctrl+d Drop"), "{screen}");
    assert!(!fake.app.databases.as_ref().unwrap().creates);
}

#[test]
fn drops_a_database() {
    let (mut fake, ledger) = server(&["kmstools", "shop"]);

    command(&mut fake, "drop database kmstools");

    assert_eq!(ledger.borrow().dropped, ["`kmstools`"]);
    assert_eq!(fake.app.status(), "dropped kmstools");
    assert_eq!(fake.app.active_database.as_deref(), Some("shop"));
}

#[test]
fn opens_the_database_list_after_dropping_the_one_in_use() {
    let (mut fake, ledger) = server(&["jobs", "kmstools", "shop"]);
    fake.press("b");
    fake.press("kmstools\n");
    assert_eq!(fake.app.active_database.as_deref(), Some("kmstools"));

    command(&mut fake, "drop database kmstools");

    assert_eq!(ledger.borrow().dropped, ["`kmstools`"]);
    assert_eq!(ledger.borrow().dropped_from, ["shop"]);
    assert_eq!(fake.app.active_database, None);
    assert!(fake.app.tables.is_empty());
    assert_eq!(
        fake.app.databases.as_ref().unwrap().options,
        ["jobs", "shop"]
    );
    assert_eq!(fake.app.status(), "dropped kmstools");
}

#[test]
fn drops_the_saved_database_from_another_of_yours() {
    let (mut fake, ledger) = server(&["information_schema", "mysql", "kmstools", "shop"]);

    command(&mut fake, "drop database shop");

    assert_eq!(ledger.borrow().dropped_from, ["kmstools"]);
}

#[test]
fn drops_from_a_system_database_when_there_is_nothing_else() {
    let (mut fake, ledger) = server(&["information_schema", "shop"]);

    command(&mut fake, "drop database shop");

    assert_eq!(ledger.borrow().dropped_from, ["information_schema"]);
}

#[test]
fn stays_on_the_database_when_dropping_it_fails() {
    let (mut fake, ledger) = server(&["jobs", "shop"]);
    ledger.borrow_mut().refuse = true;

    command(&mut fake, "drop database shop");

    assert!(fake.app.problem.is_some());
    assert_eq!(fake.app.active_database.as_deref(), Some("shop"));
    assert!(fake.app.databases.is_none());
}

#[test]
fn keeps_the_database_in_use_when_there_is_nowhere_to_move_to() {
    let (mut fake, ledger) = server(&["shop"]);

    command(&mut fake, "drop database shop");

    assert!(ledger.borrow().dropped.is_empty());
    assert_eq!(fake.app.active_database.as_deref(), Some("shop"));
    assert!(
        fake.app
            .status()
            .contains("no other database to drop it from")
    );
}

#[test]
fn will_not_drop_a_database_on_a_read_only_connection() {
    let (mut fake, ledger) = server_with(&["kmstools", "shop"], |connection| {
        connection.read_only = true
    });

    command(&mut fake, "drop database kmstools");

    assert!(ledger.borrow().dropped.is_empty());
    assert_eq!(fake.app.status(), "this connection is marked read-only");
}

#[test]
fn asks_before_dropping_a_database_from_the_list() {
    let (mut fake, ledger) = server(&["kmstools", "shop"]);
    fake.press("b");

    fake.press(UP);
    fake.press(DROP);

    assert!(ledger.borrow().dropped.is_empty());
    assert!(fake.app.databases.is_none());
    assert_eq!(fake.app.command.as_deref(), Some("drop database kmstools"));

    fake.press("\n");

    assert_eq!(ledger.borrow().dropped, ["`kmstools`"]);
}

#[test]
fn drops_nothing_when_the_confirmation_is_cancelled() {
    let (mut fake, ledger) = server(&["kmstools", "shop"]);
    fake.press("b");

    fake.press(DROP);
    fake.press(ESCAPE);

    assert!(ledger.borrow().dropped.is_empty());
    assert_eq!(fake.app.command, None);
}

#[test]
fn records_creates_and_drops_in_the_history_as_tui() {
    let store = std::env::temp_dir().join(format!(
        "tql-rust-create-drop-history-{}.sqlite",
        std::process::id()
    ));
    let _ = std::fs::remove_file(&store);
    rusqlite::Connection::open(&store)
        .unwrap()
        .execute_batch(
            "create table query_executions (id integer primary key autoincrement, connection_id integer,
                statement text, source text, succeeded integer, error text, row_count integer,
                duration_ms integer, created_at text, updated_at text);",
        )
        .unwrap();
    let ledger = Rc::new(RefCell::new(Ledger {
        databases: vec!["jobs".into(), "shop".into()],
        refuse: true,
        ..Ledger::default()
    }));
    let connection = Connection {
        id: 4,
        name: "server".into(),
        driver: "mysql".into(),
        database: Some("shop".into()),
        ..Connection::default()
    };
    let server = Server {
        current: "shop".into(),
        ledger: ledger.clone(),
    };
    let browser = Browser::new(connection, Box::new(server))
        .unwrap()
        .with_store(store.clone());
    let mut fake = Fake::sized(browser, 100, 30);

    command(&mut fake, "create database kmstools");
    command(&mut fake, "drop database jobs");

    let reader = rusqlite::Connection::open(&store).unwrap();
    let mut statement = reader
        .prepare("select connection_id, statement, source, succeeded, error from query_executions order by id")
        .unwrap();
    let recorded: Vec<(i64, String, String, bool, Option<String>)> = statement
        .query_map([], |row| {
            Ok((
                row.get(0)?,
                row.get(1)?,
                row.get(2)?,
                row.get(3)?,
                row.get(4)?,
            ))
        })
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    std::fs::remove_file(&store).unwrap();

    assert_eq!(
        recorded,
        [
            (
                4,
                "create database `kmstools`".into(),
                "tui".into(),
                true,
                None
            ),
            (
                4,
                "drop database `jobs`".into(),
                "tui".into(),
                false,
                Some("database is being accessed by other users".into())
            ),
        ]
    );
}
