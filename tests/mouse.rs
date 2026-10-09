use app::settings::{self, Settings};
use app::store::Connection;
use app::terminal::{Fake, Tui};
use app::tui::{Browser, Picker};

fn connections() -> Vec<Connection> {
    vec![
        Connection::sqlite("first", "/tmp/first.sqlite"),
        Connection::sqlite("second", "/tmp/second.sqlite"),
    ]
}

#[test]
fn clicking_a_connection_opens_it() {
    settings::install(Settings::default());
    let mut fake = Fake::new(Picker::new(connections(), None));

    fake.click(10, 6);
    assert_eq!(
        fake.exit()
            .cloned()
            .flatten()
            .map(|connection| connection.name),
        Some("second".to_string())
    );
}

#[test]
fn clicking_below_the_connections_does_nothing() {
    settings::install(Settings::default());
    let mut fake = Fake::new(Picker::new(connections(), None));

    fake.click(10, 20);
    assert!(!fake.exited());
}

#[test]
fn the_mouse_setting_turns_capture_off() {
    let mut settings = Settings::default();
    settings.set("ui.mouse", &toml::Value::Boolean(false));
    settings::install(settings);

    assert!(!Picker::new(connections(), None).wants_mouse());
}

#[test]
fn double_clicking_a_cell_opens_it_for_editing() {
    settings::install(Settings::default());
    let path = std::env::temp_dir().join(format!("tql-mouse-{}.sqlite", std::process::id()));
    let _ = std::fs::remove_file(&path);
    rusqlite::Connection::open(&path)
        .unwrap()
        .execute_batch("create table things (id integer primary key, name text); insert into things values (1, 'a'), (2, 'b');")
        .unwrap();
    let connection = Connection::sqlite("things", path.to_str().unwrap());
    let database = app::database::connect(&connection, None).unwrap();
    let mut fake = Fake::new(Browser::new(connection, database).unwrap());

    fake.click(40, 6);
    assert_eq!(fake.app.row_index, 1);
    assert_eq!(fake.app.edits.editing(), None);
    fake.click(40, 6);
    assert_eq!(fake.app.edits.editing().as_deref(), Some("b"));
}
