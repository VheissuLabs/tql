use std::path::{Path, PathBuf};

use app::database;
use app::settings::{self, Settings};
use app::store::Connection;
use app::tui::clipboard::Recorded;
use app::tui::{Browser, Exit, State};
use zerorust_tui::Fake;

const SAVE: &str = "\x13";
const NULL: &str = "\x0e";
const NOW: &str = "\x14";

fn fixture(name: &str, sql: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!(
        "tql-rust-editing-{}-{name}.sqlite",
        std::process::id()
    ));
    let _ = std::fs::remove_file(&path);
    rusqlite::Connection::open(&path)
        .unwrap()
        .execute_batch(sql)
        .unwrap();
    path
}

fn open(path: &Path, read_only: bool) -> Fake<Browser> {
    settings::install(Settings::default());
    let connection = Connection {
        read_only,
        ..Connection::sqlite("shop", path.to_str().unwrap())
    };
    let database = database::connect(&connection, None).unwrap();
    let mut fake = Fake::sized(Browser::new(connection, database).unwrap(), 100, 30);
    fake.press("\t");
    fake
}

fn recorded(path: &Path) -> (Recorded, Fake<Browser>) {
    settings::install(Settings::default());
    let clipboard = Recorded::default();
    let connection = Connection::sqlite("shop", path.to_str().unwrap());
    let database = database::connect(&connection, None).unwrap();
    let browser = Browser::new(connection, database)
        .unwrap()
        .with_clipboard(clipboard.clone());
    let mut fake = Fake::sized(browser, 100, 30);
    fake.press("\t");
    (clipboard, fake)
}

const LINKED: &str = "
    create table albums (id integer primary key, title text, artist_id integer references artists(id));
    create table artists (id integer primary key, name text not null);
    insert into artists (name) values ('AC/DC'), ('Accept');
    insert into albums (title, artist_id) values ('Powerage', 1), ('Balls to the Wall', 2);
";

fn widgets(name: &str) -> (PathBuf, Fake<Browser>) {
    let path = fixture(
        name,
        "create table widgets (id integer primary key, name text, qty integer default 1);
         insert into widgets (name, qty) values ('alpha', 1), ('beta', 2), ('gamma', 3);",
    );
    let fake = open(&path, false);
    (path, fake)
}

fn films(name: &str) -> (PathBuf, Fake<Browser>) {
    let path = fixture(
        name,
        "create table films (
            id integer primary key,
            title text not null,
            rating text default 'G',
            meta text,
            created_at datetime default current_timestamp,
            token text default (lower(hex(randomblob(4))))
         );
         insert into films (title, meta) values ('Alien', '{\"year\":1979}');",
    );
    let fake = open(&path, false);
    (path, fake)
}

fn query(path: &Path, sql: &str) -> Vec<Vec<Option<String>>> {
    let connection = rusqlite::Connection::open(path).unwrap();
    let mut statement = connection.prepare(sql).unwrap();
    let width = statement.column_count();
    statement
        .query_map([], |row| {
            (0..width)
                .map(|index| {
                    Ok(match row.get_ref(index)? {
                        rusqlite::types::ValueRef::Null => None,
                        rusqlite::types::ValueRef::Integer(number) => Some(number.to_string()),
                        other => Some(String::from_utf8_lossy(other.as_bytes().unwrap()).into()),
                    })
                })
                .collect()
        })
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap()
}

fn column(path: &Path, sql: &str) -> Vec<Option<String>> {
    query(path, sql)
        .into_iter()
        .map(|mut row| row.remove(0))
        .collect()
}

fn names(path: &Path) -> Vec<Option<String>> {
    column(path, "select name from widgets order by id")
}

fn named(values: &[(String, Option<String>)], name: &str) -> Option<String> {
    values
        .iter()
        .find(|(column, _)| column == name)
        .and_then(|(_, value)| value.clone())
}

fn status_line(fake: &mut Fake<Browser>) -> String {
    fake.screen().lines().last().unwrap_or_default().to_string()
}

fn some(values: &[&str]) -> Vec<Option<String>> {
    values.iter().map(|value| Some(value.to_string())).collect()
}

#[test]
fn edits_a_cell_and_writes_it_only_on_colon_w() {
    let (path, mut fake) = widgets("edit-write");

    fake.press("jle");
    assert_eq!(fake.app.edits.editing().as_deref(), Some("beta"));

    fake.press("-edited\x04");
    assert_eq!(names(&path), some(&["alpha", "beta", "gamma"]));
    assert!(fake.screen().contains("beta-edited"));

    fake.press(":w\n");
    assert_eq!(names(&path), some(&["alpha", "beta-edited", "gamma"]));
    assert_eq!(fake.app.status(), "wrote 1 row updated");
}

#[test]
fn leaves_the_database_alone_when_an_edit_is_cancelled() {
    let (path, mut fake) = widgets("edit-cancel");

    fake.press("jleZ\x1b");

    assert_eq!(fake.app.edits.editing(), None);
    assert_eq!(fake.app.status(), "edit cancelled");
    assert!(!fake.app.edits.pending());
    assert_eq!(names(&path), some(&["alpha", "beta", "gamma"]));
}

#[test]
fn an_emptied_cell_is_written_as_null() {
    let (path, mut fake) = widgets("edit-null");

    fake.press("le\x7f\x7f\x7f\x7f\x7f\n:w\n");

    assert_eq!(names(&path)[0], None);
}

#[test]
fn a_table_without_a_single_column_key_opens_read_only() {
    let path = fixture(
        "no-key",
        "create table pairs (a text, b text); insert into pairs values ('x', 'y');",
    );
    let mut fake = open(&path, false);

    fake.press("e");

    assert!(!fake.app.edits.editable());
    assert_eq!(
        fake.app.edits.read_only_reason(),
        Some("pairs has no single-column primary key")
    );
    assert!(fake.screen().contains("a  ·  read-only"));

    fake.press("zz");
    assert_eq!(fake.app.edits.editing().as_deref(), Some("x"));
}

#[test]
fn a_read_only_connection_refuses_every_write() {
    let path = fixture(
        "read-only",
        "create table widgets (id integer primary key, name text);
         insert into widgets values (1, 'alpha');",
    );
    let mut fake = open(&path, true);

    fake.press("e");
    assert!(!fake.app.edits.editable());
    assert_eq!(
        fake.app.edits.read_only_reason(),
        Some("this connection is marked read-only")
    );

    fake.press("\x1b");
    fake.press("N");
    assert!(fake.app.edits.form().is_none());
    assert_eq!(fake.app.status(), "this connection is marked read-only");

    fake.press("E");
    assert!(fake.app.edits.form().is_none());

    fake.press("d");
    assert!(!fake.app.edits.pending());
    assert_eq!(fake.app.status(), "this connection is marked read-only");
}

#[test]
fn capital_i_views_a_value_and_e_turns_the_viewer_into_the_editor() {
    let (_, mut fake) = widgets("view");

    fake.press("lI");
    assert!(!fake.app.edits.editable());
    assert_eq!(
        fake.app.edits.read_only_reason(),
        Some("opened with I — press e to edit")
    );

    fake.press("z");
    assert_eq!(fake.app.edits.editing().as_deref(), Some("alpha"));

    fake.press("e");
    assert!(fake.app.edits.editable());
    fake.press("z");
    assert_eq!(fake.app.edits.editing().as_deref(), Some("alphaz"));
}

#[test]
fn json_is_edited_pretty_validated_and_kept_compact() {
    let (path, mut fake) = films("json");

    fake.press("llle");
    assert_eq!(
        fake.app.edits.editing().as_deref(),
        Some("{\n    \"year\": 1979\n}")
    );

    fake.press("x\n");
    assert!(fake.app.edits.editing().is_some());
    assert_eq!(
        fake.app.status(),
        "not valid json — fix it or press esc to cancel"
    );

    fake.press("\x7f\x1b[B\x1b[F\x7f\x7f\x7f\x7f2001\n:w\n");
    assert_eq!(
        column(&path, "select meta from films"),
        some(&["{\"year\":2001}"])
    );
}

#[test]
fn shift_enter_adds_a_line_and_enter_keeps_it() {
    let (path, mut fake) = widgets("new-line");

    fake.press("le\x1b[13;2usecond\n:w\n");

    assert_eq!(names(&path)[0].as_deref(), Some("alpha\nsecond"));
}

#[test]
fn d_marks_rows_and_colon_w_deletes_them() {
    let (path, mut fake) = widgets("delete");

    fake.press("dd");
    assert_eq!(fake.app.edits.deletes, ["1", "2"]);
    assert_eq!(fake.app.row_index, 2);

    fake.press("kd");
    assert_eq!(fake.app.edits.deletes, ["1"]);

    fake.press(":w\n");
    assert_eq!(names(&path), some(&["beta", "gamma"]));
    assert_eq!(fake.app.status(), "wrote 1 deleted");
}

#[test]
fn the_status_line_calls_out_unwritten_changes_on_the_right() {
    for width in [60, 100] {
        let (_, mut fake) = widgets(&format!("callout-{width}"));
        fake.resize(width, 30);

        fake.press("d");

        let status = status_line(&mut fake);
        assert!(
            status.ends_with("1 marked for deletion · :w writes · u clears"),
            "{status}"
        );
        assert_eq!(status.matches(":w writes").count(), 1);
        assert!(status.trim_start().starts_with("shop"), "{status}");
    }
}

#[test]
fn u_drops_every_pending_change() {
    let (path, mut fake) = widgets("undo");

    fake.press("dle!\n");
    fake.press("N\n");
    fake.press(&format!("new\n{SAVE}"));
    assert_eq!(fake.app.rows.len(), 4);

    fake.press("u");

    assert!(!fake.app.edits.pending());
    assert_eq!(fake.app.rows.len(), 3);
    assert_eq!(fake.app.status(), "pending changes dropped");
    fake.press(":w\n");
    assert_eq!(fake.app.status(), "nothing to write");
    assert_eq!(names(&path), some(&["alpha", "beta", "gamma"]));
}

#[test]
fn quitting_with_unwritten_changes_drops_them_first() {
    let (_, mut fake) = widgets("quit");

    fake.press("dq");
    assert!(!fake.exited());
    assert_eq!(
        fake.app.status(),
        "1 unwritten change dropped — :q again to quit"
    );

    fake.press(":q\n");
    assert_eq!(fake.exit(), Some(&Exit::Quit));
}

#[test]
fn changing_table_drops_pending_changes() {
    let path = fixture(
        "switch",
        "create table a (id integer primary key); create table b (id integer primary key);
         insert into a values (1); insert into b values (1);",
    );
    let mut fake = open(&path, false);

    fake.press("d");
    assert!(fake.app.edits.pending());

    fake.press("\tj");
    assert!(!fake.app.edits.pending());
}

#[test]
fn n_adds_a_row_through_the_form_and_writes_it() {
    let (path, mut fake) = widgets("add");

    fake.press("N");
    let form = fake.app.edits.form().unwrap();
    assert_eq!(form.current().name, "name");
    assert_eq!(form.value("qty"), Some("1"));
    assert_eq!(form.state("id"), State::Untouched);
    assert_eq!(form.fields[0].hint, "auto");

    fake.press(&format!("\nbeta\n{SAVE}"));
    assert!(fake.app.edits.form().is_none());
    assert_eq!(fake.app.edits.inserts.len(), 1);
    assert_eq!(fake.app.row_index, 0);
    assert_eq!(fake.app.rows[0], ["", "beta", "1"]);

    fake.press(":w\n");
    assert_eq!(
        query(&path, "select name, qty from widgets where id = 4"),
        [some(&["beta", "1"])]
    );
    assert_eq!(fake.app.status(), "wrote 1 row added");
}

#[test]
fn an_explicit_null_is_written_rather_than_the_default() {
    let (path, mut fake) = widgets("add-null");

    fake.press(&format!("Njj{NULL}{SAVE}"));
    assert_eq!(fake.app.rows[0][2], "NULL");

    fake.press(":w\n");
    assert_eq!(
        column(&path, "select qty from widgets where id = 4"),
        [None]
    );
}

#[test]
fn the_newest_added_row_goes_on_top_and_survives_a_reload() {
    let (path, mut fake) = widgets("add-order");

    fake.press(&format!("N\nfirst\n{SAVE}N\nsecond\n{SAVE}r"));

    assert_eq!(fake.app.rows[0][1], "second");
    assert_eq!(fake.app.rows[1][1], "first");
    assert_eq!(fake.app.rows[2][1], "alpha");

    fake.press(":w\n");
    assert_eq!(
        names(&path),
        some(&["alpha", "beta", "gamma", "second", "first"])
    );
}

#[test]
fn a_cell_of_an_added_row_is_edited_in_place() {
    let (_, mut fake) = widgets("add-cell");

    fake.press(&format!("N{SAVE}lebeta\n"));

    assert_eq!(
        named(&fake.app.edits.inserts[0], "name").as_deref(),
        Some("beta")
    );
    assert_eq!(fake.app.rows[0][1], "beta");
}

#[test]
fn rows_are_added_to_a_table_without_a_key() {
    let path = fixture(
        "add-no-key",
        "create table notes (body text); insert into notes values ('one');",
    );
    let mut fake = open(&path, false);

    fake.press(&format!("N\ntwo\n{SAVE}:w\n"));

    assert_eq!(
        column(&path, "select body from notes"),
        some(&["one", "two"])
    );
}

#[test]
fn a_key_the_database_will_not_give_out_is_filled_with_the_next_one() {
    let path = fixture(
        "add-key",
        "create table widgets (id smallint not null primary key, name text not null);
         insert into widgets values (1, 'alpha');",
    );
    let mut fake = open(&path, false);

    fake.press("N");
    let form = fake.app.edits.form().unwrap();
    assert_eq!(form.value("id"), Some("2"));
    assert_eq!(form.current().name, "name");
    assert_eq!(form.fields[1].hint, "required");

    fake.press(&format!("\nbeta\n{SAVE}:w\n"));
    assert_eq!(
        query(&path, "select id, name from widgets order by id"),
        [some(&["1", "alpha"]), some(&["2", "beta"])]
    );
}

#[test]
fn a_failed_write_says_why_and_keeps_the_row() {
    let path = fixture(
        "add-fails",
        "create table widgets (code text not null primary key, name text not null);
         insert into widgets values ('a', 'alpha');",
    );
    let mut fake = open(&path, false);

    fake.press("N");
    let hints: Vec<String> = fake
        .app
        .edits
        .form()
        .unwrap()
        .fields
        .iter()
        .map(|field| field.hint.clone())
        .collect();
    assert_eq!(hints, ["required", "required"]);

    fake.press(&format!("\na\n{SAVE}:w\n"));

    let screen = fake.screen();
    assert!(screen.contains("COULD NOT ADD THE ROW"), "{screen}");
    assert!(screen.contains("The row is still here"), "{screen}");
    assert_eq!(fake.app.edits.inserts.len(), 1);

    fake.press("\x1b");
    assert!(!fake.screen().contains("COULD NOT ADD THE ROW"));
}

#[test]
fn capital_e_opens_the_row_on_the_field_you_were_on() {
    let (_, mut fake) = films("form-open");

    fake.press("llE");

    let form = fake.app.edits.form().unwrap();
    assert!(!form.adds());
    assert_eq!(form.current().name, "rating");
    assert_eq!(form.value("title"), Some("Alien"));
    let screen = fake.screen();
    assert!(screen.contains("EDIT ROW  ·  films  ·  id 1"), "{screen}");
    assert!(screen.contains("  title (text): Alien"), "{screen}");
    assert!(screen.contains("ctrl+s keep  esc cancel"), "{screen}");
}

#[test]
fn the_form_queues_only_the_fields_that_changed() {
    let (path, mut fake) = films("form-changed");

    fake.press(&format!("lE\n\x7f\x7f\x7f\x7f\x7fAliens\n{SAVE}"));
    assert!(fake.app.edits.form().is_none());
    assert_eq!(
        fake.app.edits.updates,
        [(
            "1".to_string(),
            vec![("title".to_string(), Some("Aliens".to_string()))]
        )]
    );

    fake.press(":w\n");
    assert_eq!(column(&path, "select title from films"), some(&["Aliens"]));
}

#[test]
fn the_form_says_so_when_nothing_changed() {
    let (_, mut fake) = films("form-unchanged");

    fake.press(&format!("E{SAVE}"));

    assert!(!fake.app.edits.pending());
    assert_eq!(fake.app.status(), "nothing changed");
}

#[test]
fn the_form_asks_before_throwing_changes_away() {
    let (_, mut fake) = films("form-discard");

    fake.press("lE\nx\n\x1b");
    assert!(fake.app.edits.form().unwrap().discarding);
    assert!(fake.screen().contains("esc again throws the row away"));

    fake.press("\x1b");
    assert!(fake.app.edits.form().is_none());
    assert!(!fake.app.edits.pending());
    assert_eq!(fake.app.status(), "row thrown away");
}

#[test]
fn escape_while_typing_puts_the_field_back_and_backspace_resets_it() {
    let (_, mut fake) = films("form-reset");

    fake.press("lE\nzzz\x1b");
    let form = fake.app.edits.form().unwrap();
    assert!(form.editor.is_none());
    assert_eq!(form.value("title"), Some("Alien"));

    fake.press("\nzzz\n");
    assert_eq!(
        fake.app.edits.form().unwrap().value("title"),
        Some("Alienzzz")
    );

    fake.press("k\x7f");
    let form = fake.app.edits.form().unwrap();
    assert_eq!(form.value("title"), Some("Alien"));
    assert!(!form.dirty());
}

#[test]
fn the_form_will_not_edit_the_key_or_null_a_not_null_column() {
    let (_, mut fake) = films("form-refuses");

    fake.press("Eg\n");
    let form = fake.app.edits.form().unwrap();
    assert!(form.editor.is_none());
    assert!(form.error.as_deref().unwrap().contains("names the row"));

    fake.press(&format!("j{NULL}"));
    let form = fake.app.edits.form().unwrap();
    assert_eq!(form.value("title"), Some("Alien"));
    assert!(form.error.as_deref().unwrap().contains("not null"));
}

#[test]
fn a_new_row_fills_plain_defaults_and_writes_now_as_the_time() {
    let (path, mut fake) = films("form-defaults");

    fake.press("N");
    let form = fake.app.edits.form().unwrap();
    assert!(form.adds());
    assert_eq!(form.value("rating"), Some("G"));
    assert_eq!(form.value("created_at"), Some("now()"));
    assert_eq!(form.state("token"), State::Untouched);
    assert_eq!(form.fields[5].hint, "default lower(hex(randomblob(4)))");
    assert_eq!(form.fields[1].hint, "required");

    fake.press(&format!("\nHeat\n{SAVE}"));
    let values = &fake.app.edits.inserts[0];
    assert_eq!(named(values, "title").as_deref(), Some("Heat"));
    assert_eq!(named(values, "rating").as_deref(), Some("G"));
    let stamp = named(values, "created_at").unwrap();
    assert_eq!(stamp.len(), "2026-01-01 00:00:00".len());
    assert!(!stamp.contains("now"));

    fake.press(":w\n");
    let written = query(&path, "select title, token from films where id = 2");
    assert_eq!(written[0][0].as_deref(), Some("Heat"));
    assert_eq!(written[0][1].as_ref().unwrap().len(), 8);
}

#[test]
fn tab_moves_on_keeping_what_was_typed() {
    let (_, mut fake) = films("form-tab");

    fake.press("N\nHeat\t");

    let form = fake.app.edits.form().unwrap();
    assert!(form.editor.is_none());
    assert_eq!(form.value("title"), Some("Heat"));
    assert_eq!(form.current().name, "rating");
}

#[test]
fn a_row_not_written_yet_is_edited_in_place_through_the_form() {
    let (_, mut fake) = films("form-pending");

    fake.press(&format!("N\nHeat\n{SAVE}lE"));
    let form = fake.app.edits.form().unwrap();
    assert_eq!(form.insert_at, Some(0));
    assert_eq!(form.value("title"), Some("Heat"));

    fake.press(&format!("\n\x7f\x7f\x7f\x7fRan\n{SAVE}"));
    assert_eq!(fake.app.edits.inserts.len(), 1);
    assert_eq!(fake.app.rows[0][1], "Ran");
}

#[test]
fn capital_e_is_refused_without_a_key_to_name_the_row() {
    let path = fixture(
        "form-no-key",
        "create table films (title text); insert into films values ('Alien');",
    );
    let mut fake = open(&path, false);

    fake.press("E");

    assert!(fake.app.edits.form().is_none());
    assert_eq!(fake.app.status(), "films has no single-column primary key");
}

#[test]
fn a_long_value_is_clamped_inside_the_form() {
    let path = fixture(
        "form-long",
        &format!(
            "create table films (id integer primary key, title text);
             insert into films (title) values ('{}');",
            "A very long title ".repeat(20)
        ),
    );
    let mut fake = open(&path, false);

    fake.press("E");

    let screen = fake.screen();
    let lines: Vec<&str> = screen.lines().collect();
    let top = lines
        .iter()
        .position(|line| line.contains("EDIT ROW"))
        .unwrap();
    let right = lines[top]
        .chars()
        .position(|character| character == '╮')
        .unwrap();
    for line in lines[top + 1..]
        .iter()
        .take_while(|line| !line.contains('╯'))
    {
        assert_eq!(line.chars().nth(right), Some('│'), "{line}");
    }
    assert!(screen.contains("… ↗"));
}

#[test]
fn now_becomes_the_time_the_way_each_column_takes_it() {
    let path = fixture(
        "now",
        "create table posts (id integer primary key, title text, published_on date, starts_at time, created_at datetime);
         insert into posts (title) values ('hello');",
    );
    let mut fake = open(&path, false);

    fake.press(&format!("N{SAVE}"));
    fake.press("llenow()\n");
    fake.press("leNOW()\n");
    fake.press(&format!("le{NOW}"));
    assert!(status_line(&mut fake).contains("ctrl+t now UTC"));
    fake.press("\n");

    let values = &fake.app.edits.inserts[0];
    let shape = |name: &str| {
        named(values, name)
            .unwrap()
            .chars()
            .map(|character| {
                if character.is_ascii_digit() {
                    '9'
                } else {
                    character
                }
            })
            .collect::<String>()
    };
    assert_eq!(shape("published_on"), "9999-99-99");
    assert_eq!(shape("starts_at"), "99:99:99");
    assert_eq!(shape("created_at"), "9999-99-99 99:99:99");
}

#[test]
fn ctrl_t_is_only_offered_on_a_time_column() {
    let path = fixture(
        "now-hint",
        "create table posts (id integer primary key, title text, created_at datetime);
         insert into posts (title) values ('hello');",
    );
    let mut fake = open(&path, false);

    fake.press("le");
    assert!(!status_line(&mut fake).contains("ctrl+t"));

    fake.press("\x1b");
    fake.press("le");
    assert!(status_line(&mut fake).contains("ctrl+t now UTC"));
}

#[test]
fn the_grid_offers_write_and_undo_while_changes_are_pending() {
    let (_, mut fake) = widgets("offers");

    assert!(!fake.screen().contains(":w Write"));

    fake.press("d");
    assert!(fake.screen().contains(":w Write   u Undo"));
}

#[test]
fn marked_edited_and_added_rows_are_drawn_as_one_coloured_bar() {
    let (_, mut fake) = widgets("tones");

    fake.press("d");
    let highlighted = fake.highlighted();
    assert!(
        highlighted
            .iter()
            .any(|run| run.contains("1") && run.contains("alpha")),
        "{highlighted:?}"
    );
}

fn run_sql(fake: &mut Fake<Browser>, statement: &str) {
    fake.press("s");
    for _ in 0..fake.app.query.editor.buffer().chars().count() {
        fake.press("\x7f");
    }
    fake.press(statement);
    fake.press("\x12");
    fake.press("\x1b");
}

#[test]
fn an_edit_made_while_filtered_is_written_and_the_filter_stays() {
    let (path, mut fake) = widgets("filtered-write");

    fake.press("lf");
    fake.press(&format!("ga{SAVE}"));
    assert_eq!(fake.app.rows.len(), 1);

    fake.press("le!\n:w\n");

    assert_eq!(names(&path), some(&["alpha", "beta", "gamma!"]));
    assert!(fake.app.filters.is_some());
    assert_eq!(fake.app.rows.len(), 1);
    assert_eq!(fake.app.rows[0][1], "gamma!");
}

#[test]
fn applying_a_filter_drops_marks_and_edits_but_keeps_added_rows() {
    let (_, mut fake) = widgets("filter-pending");

    fake.press("dle!\n");
    fake.press(&format!("N\nnew\n{SAVE}"));
    fake.press("f");
    fake.press(&format!("ga{SAVE}"));

    assert!(fake.app.edits.deletes.is_empty());
    assert!(fake.app.edits.updates.is_empty());
    assert_eq!(fake.app.edits.inserts.len(), 1);
    assert_eq!(fake.app.rows[0][1], "new");
    assert_eq!(fake.app.rows[1][1], "gamma");
}

#[test]
fn query_results_cannot_be_edited_and_pending_changes_wait_for_the_table() {
    let (path, mut fake) = widgets("query-pending");

    fake.press("jle!\n");
    run_sql(&mut fake, "select name from widgets where id = 2");
    assert!(fake.app.query.results);
    assert!(fake.app.edits.pending());
    assert!(status_line(&mut fake).ends_with("1 row edited · :w writes · u clears"));

    fake.press("e");
    assert!(!fake.app.edits.editable());
    assert_eq!(
        fake.app.edits.read_only_reason(),
        Some("query results have no row to write back to")
    );
    fake.press("\x1b");

    fake.press("d");
    assert_eq!(
        fake.app.status(),
        "query results have no row to delete — open the table itself"
    );
    fake.press("N");
    assert!(fake.app.edits.form().is_none());
    assert_eq!(fake.app.status(), "query results have no table to add to");

    fake.press("\x1b");
    assert!(!fake.app.query.results);
    assert!(fake.screen().contains("beta!"));

    fake.press(":w\n");
    assert_eq!(names(&path), some(&["alpha", "beta!", "gamma"]));
}

#[test]
fn a_query_that_moves_to_another_table_drops_the_pending_changes() {
    let path = fixture(
        "query-other-table",
        "create table a (id integer primary key); create table b (id integer primary key);
         insert into a values (1); insert into b values (1);",
    );
    let mut fake = open(&path, false);

    fake.press("d");
    run_sql(&mut fake, "select * from a");
    assert!(fake.app.edits.pending());

    run_sql(&mut fake, "select * from b");
    assert!(!fake.app.edits.pending());
    assert_eq!(fake.app.current_table().as_deref(), Some("b"));
}

#[test]
fn a_failed_write_uses_the_shared_error_box() {
    let path = fixture(
        "failed-write-box",
        "create table widgets (code text not null primary key, name text not null);
         insert into widgets values ('a', 'alpha');",
    );
    let mut fake = open(&path, false);

    fake.press(&format!("N\nb\n{SAVE}:w\n"));

    assert_eq!(fake.app.problem_title, "COULD NOT ADD THE ROW");
    assert!(fake.app.problem.as_deref().unwrap().contains("NOT NULL"));
    assert_eq!(
        fake.app.problem_notes,
        ["The row is still here — fix it and :w again, or u to drop it."]
    );
}

#[test]
fn following_a_link_and_coming_back_drop_the_pending_changes() {
    let path = fixture("link-pending", LINKED);
    let mut fake = open(&path, false);

    fake.press(&format!("dN\nNew\n{SAVE}"));
    assert!(fake.app.edits.pending());

    fake.press("jllL");
    assert_eq!(fake.app.current_table().as_deref(), Some("artists"));
    assert!(!fake.app.edits.pending());

    fake.press("le!\n");
    assert!(fake.app.edits.pending());

    fake.press("\x0f");
    assert_eq!(fake.app.current_table().as_deref(), Some("albums"));
    assert!(!fake.app.edits.pending());
    assert_eq!(
        column(&path, "select name from artists order by id"),
        some(&["AC/DC", "Accept"])
    );
}

#[test]
fn a_row_reached_through_a_link_is_edited_and_written_in_place() {
    let path = fixture("link-write", LINKED);
    let mut fake = open(&path, false);

    fake.press("jllL");
    fake.press("le!\n:w\n");

    assert_eq!(
        column(&path, "select name from artists order by id"),
        some(&["AC/DC", "Accept!"])
    );
    assert!(fake.app.filters.is_some());
    assert_eq!(fake.app.rows, [["2", "Accept!"]]);
}

#[test]
fn e_in_the_inspector_edits_the_value_on_its_line_through_the_editor() {
    let path = fixture("inspect-edit", LINKED);
    let mut fake = open(&path, false);

    fake.press("i");
    assert!(fake.screen().contains("e Edit"));

    fake.press("je");
    assert!(fake.app.inspector.is_none());
    assert!(fake.app.edits.editable());
    assert_eq!(fake.app.headers[fake.app.column_index], "title");
    assert_eq!(fake.app.edits.editing().as_deref(), Some("Powerage"));

    fake.press("!\n:w\n");
    assert_eq!(
        column(&path, "select title from albums where id = 1"),
        some(&["Powerage!"])
    );
}

#[test]
fn y_in_the_viewer_yanks_the_selected_lines() {
    let path = fixture(
        "viewer-yank",
        "create table things (id integer primary key, body text);
         insert into things (body) values ('one\ntwo\nthree');",
    );
    let (clipboard, mut fake) = recorded(&path);

    fake.press("lIgjVjy");

    assert_eq!(clipboard.last().as_deref(), Some("two\nthree"));
    assert_eq!(fake.app.status(), "yanked 2 lines to the system clipboard");
    assert!(fake.screen().contains("y Yank"));
}

#[test]
fn y_copies_a_failed_write_without_closing_it() {
    let path = fixture(
        "copy-failure",
        "create table widgets (code text not null primary key, name text not null);
         insert into widgets values ('a', 'alpha');",
    );
    let (clipboard, mut fake) = recorded(&path);

    fake.press(&format!("N\nb\n{SAVE}:w\n"));
    assert!(fake.screen().contains("y Copy"));
    fake.press("y");

    assert!(clipboard.last().unwrap().contains("NOT NULL"));
    assert_eq!(fake.app.status(), "copied the error");
    assert!(fake.app.problem.is_some());
}
