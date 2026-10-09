use std::path::PathBuf;

use app::database;
use app::store::Connection;
use app::terminal::Fake;
use app::tui::{Browser, Exit, Focus, Mode, PAGE};

fn fixture(name: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!("tql-rust-{}-{name}.sqlite", std::process::id()));
    let _ = std::fs::remove_file(&path);

    let connection = rusqlite::Connection::open(&path).unwrap();
    connection
        .execute_batch(
            "create table customers (id integer primary key, name text, email text, notes text);
             create table orders (id integer primary key, customer_id integer, total real);
             create table products (sku text primary key, title text);
             create table audit_log (happened_at text, message text);",
        )
        .unwrap();
    for id in 1..=250 {
        connection
            .execute(
                "insert into customers values (?1, ?2, ?3, ?4)",
                (
                    id,
                    format!("Customer {id:03}"),
                    format!("c{id}@example.com"),
                    if id % 2 == 0 { None } else { Some("vip") },
                ),
            )
            .unwrap();
    }
    connection
        .execute_batch("insert into orders values (1, 7, 19.5), (2, 3, 4.25); insert into products values ('B-2', 'Bolt'), ('A-1', 'Anvil');")
        .unwrap();
    path
}

fn browser(name: &str) -> Fake<Browser> {
    let path = fixture(name);
    let connection = Connection::sqlite("shop", path.to_str().unwrap());
    let database = database::connect(&connection, None).unwrap();
    Fake::sized(Browser::new(connection, database).unwrap(), 120, 30)
}

#[test]
fn opens_on_the_first_table_sorted_by_its_key() {
    let mut fake = browser("opens");

    assert_eq!(
        fake.app.tables,
        ["audit_log", "customers", "orders", "products"]
    );
    assert_eq!(fake.app.current_table().as_deref(), Some("audit_log"));
    assert_eq!(fake.app.focus, Focus::Sidebar);

    fake.press("j");
    let screen = fake.screen();
    assert!(screen.contains("[1] tql-rust-"), "{screen}");
    assert!(screen.contains("[2] customers"));
    assert!(screen.contains("id ▲"));
    assert!(screen.contains("Customer 001"));
    assert!(fake.app.status().starts_with("rows 1-100+ · "));
}

#[test]
fn the_selected_table_and_the_cursor_cell_are_highlighted() {
    let mut fake = browser("highlight");

    fake.press("j");
    assert_eq!(fake.highlighted(), ["customers"]);

    fake.press("\tl");
    assert_eq!(fake.highlighted(), ["customers", "Customer 001"]);
}

#[test]
fn null_reads_as_null() {
    let mut fake = browser("null");

    fake.press("j\t");
    assert_eq!(fake.app.rows[1][3], "NULL");
    assert!(fake.screen().contains("NULL"));
}

#[test]
fn tab_moves_between_panes_and_hjkl_move_the_cursor() {
    let mut fake = browser("cursor");

    fake.press("j\tjjl");
    assert_eq!(fake.app.focus, Focus::Grid);
    assert_eq!((fake.app.row_index, fake.app.column_index), (2, 1));

    fake.press("hkkkk");
    assert_eq!((fake.app.row_index, fake.app.column_index), (0, 0));
    assert!(fake.screen().contains(" ▸"));

    fake.press("h");
    assert_eq!(fake.app.focus, Focus::Sidebar);

    fake.press("l");
    assert_eq!(fake.app.focus, Focus::Grid);
    assert_eq!(fake.app.column_index, 0);
}

#[test]
fn pages_fetch_one_extra_row_to_know_there_is_more() {
    let mut fake = browser("paging");

    fake.press("j\t");
    assert!(fake.app.has_more);
    assert_eq!(fake.app.rows.len(), PAGE);

    fake.press("nn");
    assert_eq!(fake.app.offset, 200);
    assert!(!fake.app.has_more);
    assert_eq!(fake.app.rows.len(), 50);
    assert_eq!(fake.app.rows[0][1], "Customer 201");
    assert!(fake.app.status().starts_with("rows 201-250"));

    fake.press("n");
    assert_eq!(fake.app.offset, 200, "no page past the end");

    fake.press("ppp");
    assert_eq!(fake.app.offset, 0);
}

#[test]
fn sorting_cycles_ascending_descending_and_back_to_the_key() {
    let mut fake = browser("sort");

    fake.press("j\tl");
    fake.press("o");
    assert_eq!(fake.app.status(), "sorted by name asc");
    assert_eq!(fake.app.rows[0][1], "Customer 001");

    fake.press("o");
    assert_eq!(fake.app.status(), "sorted by name desc");
    assert_eq!(fake.app.rows[0][1], "Customer 250");
    assert!(fake.screen().contains("name ▼"));

    fake.press("o");
    assert_eq!(fake.app.status(), "sorted by id asc");
}

#[test]
fn a_table_without_a_key_clears_the_sort() {
    let mut fake = browser("nokey");

    fake.press("\to");
    assert_eq!(fake.app.status(), "sorted by happened_at asc");
    fake.press("oo");
    assert_eq!(fake.app.status(), "sort cleared");
    assert!(fake.app.sort.is_none());
}

#[test]
fn slash_filters_the_table_list() {
    let mut fake = browser("filter");

    fake.press("/ord");
    assert!(fake.app.filtering);
    assert_eq!(fake.app.current_table().as_deref(), Some("orders"));
    assert!(fake.screen().contains(" /ord█"));

    fake.press("\n");
    assert!(!fake.app.filtering);
    assert!(
        fake.screen().contains("/ord ─"),
        "the heading keeps the filter"
    );

    fake.press("/zzz");
    assert!(fake.screen().contains("nothing matches"));

    fake.press("\x1b");
    assert_eq!(fake.app.filter, "");
    assert_eq!(fake.app.visible_tables().len(), 4);
}

#[test]
fn backslash_hides_and_shows_the_tables() {
    let mut fake = browser("hide");

    fake.press("\\");
    assert!(fake.app.tables_hidden);
    assert_eq!(fake.app.focus, Focus::Grid);
    assert!(!fake.screen().contains("[1]"));

    fake.press("\x1b1");
    assert!(!fake.app.tables_hidden);
    assert_eq!(fake.app.focus, Focus::Sidebar);
}

#[test]
fn help_lists_the_keys_and_closes_with_escape() {
    let mut fake = browser("help");

    fake.press("?");
    assert_eq!(fake.app.mode, Mode::Help);
    let screen = fake.screen();
    assert!(screen.contains("HELP"));
    assert!(screen.contains("sort: asc, desc, key"));

    fake.press("\x1b");
    assert_eq!(fake.app.mode, Mode::Browse);
}

#[test]
fn colon_runs_commands() {
    let mut fake = browser("command");

    fake.press(":wat\n");
    assert_eq!(fake.app.status(), "unknown command: :wat");

    fake.press(":q\n");
    assert_eq!(fake.exit(), Some(&Exit::Quit));
}

#[test]
fn q_quits_and_c_goes_back_to_connections() {
    let mut quitting = browser("quit");
    quitting.press("q");
    assert_eq!(quitting.exit(), Some(&Exit::Quit));

    let mut leaving = browser("leave");
    leaving.press("c");
    assert_eq!(leaving.exit(), Some(&Exit::Connections));
}

#[test]
fn a_small_window_asks_to_be_bigger() {
    let mut fake = browser("small");

    fake.resize(50, 10);
    assert!(fake.screen().contains("Make the window bigger"));
}

#[test]
fn moving_right_scrolls_wide_tables_sideways() {
    let mut fake = browser("wide");
    fake.resize(70, 20);

    fake.press("j\tlll");
    assert_eq!(fake.app.column_index, 3);
    let screen = fake.screen();
    assert!(screen.contains("notes"), "{screen}");
    assert!(fake.app.column_offset > 0);
}
