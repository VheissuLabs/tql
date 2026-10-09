use std::path::PathBuf;

use app::database;
use app::settings::{self, Settings};
use app::store::Connection;
use app::terminal::Fake;
use app::tui::{Browser, Mode};

fn fixture(name: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!("tql-paste-{}-{name}.sqlite", std::process::id()));
    let _ = std::fs::remove_file(&path);
    rusqlite::Connection::open(&path)
        .unwrap()
        .execute_batch(
            "create table customers (id integer primary key, name text, email text);
             insert into customers values (1, 'Ada', 'ada@example.com'), (2, 'Grace', null);",
        )
        .unwrap();
    path
}

fn browser(name: &str) -> Fake<Browser> {
    settings::install(Settings::default());
    let path = fixture(name);
    let connection = Connection::sqlite("shop", path.to_str().unwrap());
    let database = database::connect(&connection, None).unwrap();
    Fake::sized(Browser::new(connection, database).unwrap(), 120, 30)
}

fn vim() {
    let mut settings = Settings::default();
    settings.set("ui.sql_editor", &toml::Value::String("vim".into()));
    settings::install(settings);
}

#[test]
fn a_paste_lands_in_the_sql_editor_as_it_was_written() {
    let mut fake = browser("sql");
    fake.press("s");
    let before = fake.app.query.editor.buffer();

    fake.paste("select id,\r\n  name\r\nfrom customers\twhere id = 1");

    assert_eq!(
        fake.app.query.editor.buffer(),
        format!("{before}select id,\n  name\nfrom customers\twhere id = 1")
    );
    assert_eq!(fake.app.mode, Mode::Query);
}

#[test]
fn one_undo_takes_back_a_whole_paste() {
    let mut fake = browser("sql-undo");
    fake.press("s").press("select ");
    let before = fake.app.query.editor.buffer();

    fake.paste("id from customers").press("\x1a");

    assert_eq!(fake.app.query.editor.buffer(), before);
}

#[test]
fn a_paste_in_vim_normal_mode_is_not_run_as_commands() {
    let mut fake = browser("vim-normal");
    vim();
    fake.press("s");
    let before = fake.app.query.editor.buffer();

    fake.paste("ddu");

    assert_eq!(fake.app.query.editor.buffer(), before);
}

#[test]
fn a_paste_in_vim_insert_mode_is_typed() {
    let mut fake = browser("vim-insert");
    vim();
    fake.press("s").press("A");
    let before = fake.app.query.editor.buffer();

    fake.paste("\nselect\n  1");

    assert_eq!(
        fake.app.query.editor.buffer(),
        format!("{before}\nselect\n  1")
    );
}

#[test]
fn a_paste_while_browsing_does_nothing() {
    let mut fake = browser("browse");

    fake.press("\t").paste("dq");

    assert!(!fake.exited());
    assert_eq!(fake.app.edits.deletes.len(), 0);
}

#[test]
fn a_paste_on_the_command_line_stays_on_one_line_and_runs_nothing() {
    let mut fake = browser("command");

    fake.press(":").paste("w\nq");

    assert_eq!(fake.app.command.as_deref(), Some("w q"));
    assert!(!fake.exited());
}

#[test]
fn backspace_on_an_empty_command_line_keeps_it_open() {
    let mut fake = browser("command-backspace");

    fake.press(":").press("\x7f");
    assert_eq!(fake.app.command.as_deref(), Some(""));

    fake.press("ab").press("\x08");
    assert_eq!(fake.app.command.as_deref(), Some("a"));
}

#[test]
fn a_paste_into_the_table_filter_stays_on_one_line() {
    let mut fake = browser("table-filter");

    fake.press("/").paste("cust\nomers");

    assert_eq!(fake.app.filter, "cust omers");
    assert!(fake.app.filtering);
}

#[test]
fn a_paste_into_a_cell_keeps_its_lines_and_does_not_save_it() {
    let mut fake = browser("cell");

    fake.press("\tlle").paste("line one\r\nline two");

    assert_eq!(
        fake.app.edits.editing().as_deref(),
        Some("ada@example.comline one\nline two")
    );
}

#[test]
fn a_paste_into_the_row_filter_stays_on_one_line() {
    let mut fake = browser("row-filter");

    fake.press("\tf").paste("Gr\nace");

    let form = fake.app.filter_form.as_ref().unwrap();
    assert_eq!(form.editor.as_ref().unwrap().text(), "Gr ace");
}
