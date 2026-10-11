use std::path::PathBuf;

use app::database;
use app::settings::{self, Settings};
use app::store::Connection;
use app::terminal::Fake;
use app::tui::Browser;

fn fixture(name: &str) -> PathBuf {
    let path =
        std::env::temp_dir().join(format!("tql-margins-{}-{name}.sqlite", std::process::id()));
    let _ = std::fs::remove_file(&path);
    rusqlite::Connection::open(&path)
        .unwrap()
        .execute_batch(
            "create table alpha (id integer primary key);
             create table beta (id integer primary key);
             create table gamma (id integer primary key);",
        )
        .unwrap();
    path
}

fn browser(name: &str, margins: [i64; 4]) -> Fake<Browser> {
    let mut chosen = Settings::default();
    for (key, rows) in ["top", "bottom", "left", "right"].iter().zip(margins) {
        chosen.set(&format!("ui.{key}_margin"), &toml::Value::Integer(rows));
    }
    settings::install(chosen);
    let path = fixture(name);
    let connection = Connection::sqlite("shop", path.to_str().unwrap());
    let database = database::connect(&connection, None).unwrap();
    Fake::sized(Browser::new(connection, database).unwrap(), 80, 20)
}

#[test]
fn the_frame_fills_the_terminal_when_every_margin_is_zero() {
    let mut fake = browser("none", [0, 0, 0, 0]);
    let screen = fake.screen();
    let lines: Vec<&str> = screen.split('\n').collect();

    assert!(lines[0].starts_with('╭'), "{screen}");
    assert_eq!(lines[0].chars().count(), 80, "{screen}");
    assert!(!lines[19].is_empty(), "{screen}");
}

#[test]
fn each_margin_leaves_its_own_blank_rows_and_columns() {
    let mut fake = browser("all", [1, 2, 2, 3]);
    let screen = fake.screen();
    let lines: Vec<&str> = screen.split('\n').collect();

    assert_eq!(lines[0], "", "{screen}");
    assert!(lines[1].starts_with("  ╭"), "{screen}");
    assert_eq!(lines[1].chars().count(), 77, "{screen}");
    assert!(lines[2].starts_with("  │"), "{screen}");
    assert!(!lines[17].is_empty(), "{screen}");
    assert_eq!(lines[18], "", "{screen}");
    assert_eq!(lines[19], "", "{screen}");
}

#[test]
fn a_click_lands_where_the_frame_was_drawn_and_not_in_the_margin() {
    let mut fake = browser("click", [1, 0, 2, 3]);

    fake.click(1, 4);
    assert_eq!(fake.app.current_table().as_deref(), Some("alpha"));

    fake.click(5, 4);
    assert_eq!(fake.app.current_table().as_deref(), Some("beta"));
}
