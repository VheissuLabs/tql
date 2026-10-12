use std::path::PathBuf;

use app::database;
use app::settings::{self, Settings};
use app::store::Connection;
use app::terminal::Fake;
use app::tui::{Browser, Focus};

fn fixture(name: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!(
        "tql-rust-keys-{}-{name}.sqlite",
        std::process::id()
    ));
    let _ = std::fs::remove_file(&path);

    let connection = rusqlite::Connection::open(&path).unwrap();
    connection
        .execute_batch(
            "create table alpha (id integer primary key);
             create table widgets (id integer primary key, name text, size text, colour text, weight text);
             create table zebras (id integer primary key);
             create table zulus (id integer primary key);",
        )
        .unwrap();
    for id in 1..=40 {
        connection
            .execute(
                "insert into widgets values (?1, ?2, 'm', 'red', '1kg')",
                (id, format!("widget {id}")),
            )
            .unwrap();
    }
    path
}

fn browser(name: &str, keys: &[(&str, toml::Value)]) -> Fake<Browser> {
    let mut configured = Settings::default();
    for (action, value) in keys {
        configured.set(&format!("keys.{action}"), value);
    }
    settings::install(configured);

    let path = fixture(name);
    let connection = Connection::sqlite("keys", path.to_str().unwrap());
    let database = database::connect(&connection, None).unwrap();
    Fake::sized(Browser::new(connection, database).unwrap(), 120, 30)
}

fn text(key: &str) -> toml::Value {
    toml::Value::from(key)
}

#[test]
fn drives_the_grid_with_the_rebound_key() {
    let mut fake = browser(
        "rebound",
        &[("filter_rows", text("F")), ("new_row", text("ctrl+n"))],
    );
    fake.press("j\t");

    fake.press("f");
    assert!(fake.app.filter_form.is_none());

    fake.press("F");
    assert!(fake.app.filter_form.is_some());

    fake.press("\x1b").press("\x1b").press("\x0e");
    assert!(fake.app.edits.form().is_some());
}

#[test]
fn shows_the_rebound_key_in_help_and_on_the_hotkey_bar() {
    let mut fake = browser("shown", &[("filter_rows", text("F"))]);
    fake.press("j\t");

    assert!(fake.screen().contains("F Filter"), "{}", fake.screen());

    fake.press("?");
    let help = fake.screen();
    assert!(
        help.lines()
            .any(|line| line.contains(" F ") && line.contains("filter rows")),
        "{help}"
    );
}

#[test]
fn takes_several_keys_for_one_action() {
    let mut fake = browser(
        "several",
        &[(
            "yank_value",
            toml::Value::Array(vec![text("y"), text("ctrl+y")]),
        )],
    );
    fake.press("j\t\x19");

    assert!(
        fake.app.status().contains("yanked"),
        "{}",
        fake.app.status()
    );
}

#[test]
fn a_key_two_actions_ask_for_does_what_the_earlier_one_does() {
    let mut fake = browser("clash", &[("filter_rows", text("r"))]);
    fake.press("j\tr");

    assert!(fake.app.filter_form.is_some());
}

#[test]
fn pane_titles_follow_the_keys_that_focus_them() {
    let mut fake = browser(
        "panes",
        &[("focus_tables", text("T")), ("focus_rows", text("ctrl+g"))],
    );
    fake.press("j");
    let screen = fake.screen();

    assert!(screen.contains("[T] "), "{screen}");
    assert!(screen.contains("[ctrl+g] widgets"), "{screen}");

    fake.press("\x07");
    assert_eq!(fake.app.focus, Focus::Grid);
    fake.press("T");
    assert_eq!(fake.app.focus, Focus::Sidebar);
}

#[test]
fn hiding_the_tables_says_which_key_shows_them() {
    let mut fake = browser("hidden", &[("toggle_tables", text("ctrl+b"))]);
    fake.press("\x02");

    assert_eq!(fake.app.status(), "table list hidden · ctrl+b shows it");
}

#[test]
fn a_count_moves_the_rows_that_many_times() {
    let mut fake = browser("count-rows", &[]);
    fake.press("j\t5j");
    assert_eq!(fake.app.row_index, 5);

    fake.press("12j");
    assert_eq!(fake.app.row_index, 17);

    fake.press("3k");
    assert_eq!(fake.app.row_index, 14);

    fake.press("999j");
    assert_eq!(fake.app.row_index, 39);

    fake.press("99999999999999999999999k");
    assert_eq!(fake.app.row_index, 0);
}

#[test]
fn a_count_moves_the_columns_and_stops_at_the_edges() {
    let mut fake = browser("count-columns", &[]);
    fake.press("j\t3l");
    assert_eq!(fake.app.column_index, 3);

    fake.press("9l");
    assert_eq!(fake.app.column_index, 4);

    fake.press("2h");
    assert_eq!(fake.app.column_index, 2);

    fake.press("9h");
    assert_eq!((fake.app.column_index, fake.app.focus), (0, Focus::Grid));
}

#[test]
fn a_count_moves_through_the_table_list() {
    let mut fake = browser("count-tables", &[]);
    fake.press("2j");
    assert_eq!(
        fake.app
            .current_table()
            .map(|table| table.to_string())
            .as_deref(),
        Some("zebras")
    );

    fake.press("9j");
    assert_eq!(
        fake.app
            .current_table()
            .map(|table| table.to_string())
            .as_deref(),
        Some("zulus")
    );

    fake.press("10k");
    assert_eq!(
        fake.app
            .current_table()
            .map(|table| table.to_string())
            .as_deref(),
        Some("alpha")
    );

    fake.press("3l");
    assert_eq!(fake.app.focus, Focus::Grid);
}

#[test]
fn zero_alone_is_not_a_count_and_another_key_drops_one() {
    let mut fake = browser("count-zero", &[]);
    fake.press("j\t0j");
    assert_eq!(fake.app.row_index, 1);
    assert!(fake.app.count_prefix.is_empty());

    fake.press("10j");
    assert_eq!(fake.app.row_index, 11);

    fake.press("4?").press("\x1b").press("j");
    assert_eq!(fake.app.row_index, 12);
}

#[test]
fn a_count_reaches_the_rebound_keys_without_changing_them() {
    let mut fake = browser("count-rebound", &[("filter_rows", text("F"))]);
    fake.press("j\t3F");

    assert!(fake.app.filter_form.is_some());
    assert!(fake.app.count_prefix.is_empty());
}

fn sql_pane_shown(fake: &mut Fake<Browser>) -> bool {
    fake.screen().contains("] SQL")
}

#[test]
fn the_palette_shows_and_hides_the_sql_editor() {
    let mut fake = browser("toggle-sql-palette", &[]);
    assert!(!sql_pane_shown(&mut fake), "{}", fake.screen());

    fake.press("\x0bshow or hide the sql\n");
    assert_eq!(fake.app.mode, app::tui::Mode::Query);
    assert!(sql_pane_shown(&mut fake), "{}", fake.screen());

    fake.press("\x0bshow or hide the sql\n");
    assert_eq!(fake.app.mode, app::tui::Mode::Browse);
    assert!(!sql_pane_shown(&mut fake), "{}", fake.screen());
    assert_eq!(fake.app.status(), "SQL editor hidden · | shows it");
}

#[test]
fn hiding_the_sql_editor_holds_even_when_it_is_always_shown() {
    let mut configured = Settings::default();
    configured.set("ui.sql_always", &toml::Value::Boolean(true));
    settings::install(configured);
    let path = fixture("toggle-sql-always");
    let connection = Connection::sqlite("keys", path.to_str().unwrap());
    let database = database::connect(&connection, None).unwrap();
    let mut fake = Fake::sized(Browser::new(connection, database).unwrap(), 120, 30);
    assert!(sql_pane_shown(&mut fake), "{}", fake.screen());

    fake.press("|");
    assert!(!sql_pane_shown(&mut fake), "{}", fake.screen());
    fake.press("j");
    assert!(!sql_pane_shown(&mut fake), "{}", fake.screen());

    fake.press("|");
    assert!(sql_pane_shown(&mut fake), "{}", fake.screen());
    assert_eq!(fake.app.mode, app::tui::Mode::Query);
}

#[test]
fn colon_v_says_which_version_of_tql_this_is() {
    let version = format!("tql v{}", env!("CARGO_PKG_VERSION"));
    let mut fake = browser("version", &[]);

    fake.press(":v\n");
    assert_eq!(fake.app.status(), version);

    fake.press(":version\n");
    assert_eq!(fake.app.status(), version);

    let mut picker = Fake::sized(app::tui::Picker::new(Vec::new(), None), 120, 30);
    picker.press(":v\n");
    assert!(picker.screen().contains(&version), "{}", picker.screen());

    let mut picker = Fake::sized(app::tui::Picker::new(Vec::new(), None), 120, 30);
    picker.press("\x0bversion\n");
    assert!(picker.screen().contains(&version), "{}", picker.screen());
}
