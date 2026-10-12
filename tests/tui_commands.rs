use std::fs;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use app::database;
use app::paths;
use app::settings::{self, Settings};
use app::store::{Connection, Store};
use app::store_writer::StoreWriter;
use app::terminal::{Fake, Tui};
use app::tui::updating::CHECKING;
use app::tui::{Browser, Exit, Mode, Picker};
use app::updates::Releases;

struct Scratch {
    root: PathBuf,
}

impl Scratch {
    fn new(label: &str) -> Self {
        let root =
            std::env::temp_dir().join(format!("tql-tui-commands-{}-{label}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("tql")).unwrap();
        paths::use_config_directory(root.join("tql"));
        StoreWriter::prepare(&paths::store()).unwrap();
        settings::install(Settings::default());
        Scratch { root }
    }

    fn sqlite(&self, name: &str, tables: &[&str]) -> PathBuf {
        let path = self.root.join(format!("{name}.sqlite"));
        let connection = rusqlite::Connection::open(&path).unwrap();
        for (position, table) in tables.iter().enumerate() {
            connection
                .execute_batch(&format!(
                    "create table {table} (id integer primary key, name text);
                     insert into {table} (name) values ('one'), ('two'), ('{position}');"
                ))
                .unwrap();
        }
        path
    }

    fn browser(&self, name: &str, tables: &[&str]) -> Fake<Browser> {
        let path = self.sqlite(name, tables);
        let connection = Connection::sqlite(name, path.to_str().unwrap());
        let database = database::connect(&connection, None).unwrap();
        Fake::sized(
            Browser::new(connection, database)
                .unwrap()
                .with_store(paths::store()),
            120,
            30,
        )
    }

    fn picker(&self) -> Fake<Picker> {
        Fake::sized(
            Picker::new(Vec::new(), None).with_store(&paths::store(), &paths::key()),
            120,
            30,
        )
    }

    fn saved(&self) -> Vec<Connection> {
        Store::open(&paths::store()).unwrap().connections().unwrap()
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn unreachable_releases() {
    app::updates::releases::use_source(Releases::at(
        "VheissuLabs/tql",
        "http://127.0.0.1:9/latest",
        "http://127.0.0.1:9/download",
    ));
}

fn heard<App: Tui>(fake: &mut Fake<App>, done: impl Fn(&str) -> bool) -> String {
    let started = Instant::now();
    loop {
        fake.app.idle();
        let screen = fake.screen();
        if done(&screen) || started.elapsed() > Duration::from_secs(10) {
            return screen;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn colon_update_checks_without_holding_up_the_screen_and_says_what_it_found() {
    let scratch = Scratch::new("update-browser");
    unreachable_releases();
    let mut fake = scratch.browser("shop", &["orders"]);

    fake.press(":update\n");
    assert_eq!(fake.app.status(), CHECKING);

    let screen = heard(&mut fake, |screen| {
        screen.contains("Could not reach GitHub")
    });
    assert!(screen.contains("Could not reach GitHub"), "{screen}");
}

#[test]
fn the_connection_list_updates_too() {
    let scratch = Scratch::new("update-picker");
    unreachable_releases();
    let mut fake = scratch.picker();

    fake.press(":update\n");
    assert!(fake.screen().contains(CHECKING), "{}", fake.screen());

    let screen = heard(&mut fake, |screen| {
        screen.contains("Could not reach GitHub")
    });
    assert!(screen.contains("Could not reach GitHub"), "{screen}");
}

#[test]
fn colon_help_and_colon_h_open_help() {
    let scratch = Scratch::new("help");
    let mut fake = scratch.browser("shop", &["orders"]);

    fake.press(":help\n");
    assert_eq!(fake.app.mode, Mode::Help);

    fake.press("\x1b:h\n");
    assert_eq!(fake.app.mode, Mode::Help);
}

#[test]
fn colon_open_saves_a_sqlite_file_and_switches_to_it() {
    let scratch = Scratch::new("open-browser");
    let other = scratch.sqlite("other", &["invoices"]);
    let mut fake = scratch.browser("shop", &["orders"]);

    fake.press(&format!(":open {}\n", other.display()));

    let saved = scratch.saved();
    let opened = saved
        .iter()
        .find(|connection| connection.name == "other.sqlite")
        .unwrap_or_else(|| panic!("{saved:?} {}", fake.app.status()));
    assert_eq!(fake.output(), Some(&Exit::Open(opened.id)));
}

#[test]
fn colon_open_on_the_connection_list_opens_it_and_reuses_one_already_saved() {
    let scratch = Scratch::new("open-picker");
    let other = scratch.sqlite("other", &["invoices"]);

    let mut fake = scratch.picker();
    fake.press(&format!(":open {}\n", other.display()));
    let first = fake.output().cloned().flatten().expect("it opens");

    let mut fake = scratch.picker();
    fake.press(&format!(":open {}\n", other.display()));
    let second = fake.output().cloned().flatten().expect("it opens");

    assert_eq!(first.id, second.id);
    assert_eq!(scratch.saved().len(), 1);
}

#[test]
fn colon_open_says_what_is_wrong_and_alone_waits_for_a_path() {
    let scratch = Scratch::new("open-wrong");
    let mut fake = scratch.browser("shop", &["orders"]);

    fake.press(":open /nowhere/at/all.sqlite\n");
    assert!(!fake.exited());
    assert_eq!(fake.app.status(), "No such file: /nowhere/at/all.sqlite");

    fake.press(":open\n");
    assert_eq!(fake.app.command.as_deref(), Some("open "));
}

#[test]
fn colon_export_database_writes_every_table_into_one_file() {
    let scratch = Scratch::new("export-database");
    let mut fake = scratch.browser("shop", &["orders", "customers"]);

    fake.press(":export database\n");

    let status = fake.app.status().to_string();
    assert!(
        status.starts_with("exported 6 rows from 2 tables"),
        "{status}"
    );
    let path = status.rsplit(" to ").next().unwrap();
    let written = fs::read_to_string(path).unwrap();
    assert!(written.contains("-- orders"), "{written}");
    assert!(written.contains("-- customers"), "{written}");
}
