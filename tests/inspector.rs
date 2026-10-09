use std::sync::atomic::{AtomicUsize, Ordering};

use app::database;
use app::settings::{self, Settings};
use app::store::Connection;
use app::terminal::Fake;
use app::tui::Browser;
use app::tui::clipboard::{Reached, Recorded};

const ARTISTS: &str = "
    create table artists (id integer primary key, name text not null, profile text);
    create table albums (id integer primary key, artist_id integer references artists(id), title text);
    create table passports (id integer primary key, artist_id integer unique references artists(id), number text);
    insert into artists (name, profile) values ('AC/DC', '{\"formed\": 1973, \"members\": [\"Angus\"]}'), ('Accept', null);
    insert into albums (artist_id, title) values (1, 'Let There Be Rock'), (1, 'Powerage'), (2, 'Balls to the Wall');
    insert into passports (artist_id, number) values (1, 'X1');
";

fn browser_with(sql: &str, clipboard: Recorded) -> Fake<Browser> {
    static FIXTURES: AtomicUsize = AtomicUsize::new(0);
    let path = std::env::temp_dir().join(format!(
        "tql-rust-{}-inspector-{}.sqlite",
        std::process::id(),
        FIXTURES.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = std::fs::remove_file(&path);
    rusqlite::Connection::open(&path)
        .unwrap()
        .execute_batch(sql)
        .unwrap();

    let connection = Connection::sqlite("inspect", path.to_str().unwrap());
    let database = database::connect(&connection, None).unwrap();
    let browser = Browser::new(connection, database)
        .unwrap()
        .with_clipboard(clipboard);
    Fake::sized(browser, 120, 40)
}

fn on_artists(sql: &str) -> Fake<Browser> {
    let mut fake = browser_with(sql, Recorded::default());
    fake.press("j\n");
    fake
}

fn with_settings(change: impl FnOnce(&mut Settings)) {
    let mut changed = Settings::default();
    change(&mut changed);
    settings::install(changed);
}

#[test]
fn shows_the_record_with_its_types_and_what_is_related() {
    let mut fake = on_artists(ARTISTS);

    fake.press("i");
    let screen = fake.screen();

    assert!(screen.contains("╭─ RECORD  (3)"), "{screen}");
    assert!(screen.contains(&format!("name{}AC/DC", " ".repeat(18))));
    assert!(screen.contains("{\"formed\":1973,\"members\":[\"Angus\"]}"));
    assert!(screen.contains("╭─ RELATED  (2)"));
    assert!(screen.contains("▸ albums  ·  has many  (2)"));
    assert!(screen.contains("▸ passports  ·  has one"));
    assert!(!screen.contains("Powerage"));
    assert!(screen.contains(" row   ↵ folds    j/k moves"));
}

#[test]
fn loads_a_related_table_only_once_it_is_unfolded() {
    let mut fake = on_artists(ARTISTS);

    fake.press("i\t\n");
    let screen = fake.screen();

    assert!(screen.contains("▾ albums  ·  has many  (2)"), "{screen}");
    assert!(screen.contains("title"));
    assert!(screen.contains("Let There Be Rock"));
    assert!(screen.contains("Powerage"));
    assert!(!screen.contains("artist_id"));
}

#[test]
fn shows_a_record_it_belongs_to_as_fields() {
    let mut fake = browser_with(ARTISTS, Recorded::default());

    fake.press("\ti\t\n");
    let screen = fake.screen();

    assert!(screen.contains("▾ artists  ·  belongs to"), "{screen}");
    assert!(screen.contains(&format!("name{}AC/DC", " ".repeat(18))));
}

#[test]
fn shows_what_is_on_the_far_side_of_a_pivot() {
    let mut fake = on_artists(&format!(
        "{ARTISTS}
         create table festivals (id integer primary key, name text);
         create table bookings (artist_id integer references artists(id), festival_id integer references festivals(id), created_at text);
         insert into festivals (name) values ('Monsters of Rock'), ('Wacken');
         insert into bookings values (1, 1, 'now'), (1, 2, 'now');"
    ));

    fake.press("i\tj\n");
    let screen = fake.screen();

    assert!(
        screen.contains("festivals  ·  has many through bookings  (2)"),
        "{screen}"
    );
    assert!(screen.contains("Monsters of Rock"));
    assert!(screen.contains("Wacken"));
}

#[test]
fn caps_the_related_rows_and_says_how_many_there_are() {
    with_settings(|settings| settings.inspect_related = 1);
    let mut fake = on_artists(ARTISTS);

    fake.press("i\t\n");
    let screen = fake.screen();

    assert!(screen.contains("albums  ·  has many  (1 of 2)"), "{screen}");
    assert!(screen.contains("Let There Be Rock"));
    assert!(!screen.contains("Powerage"));
}

#[test]
fn loads_nothing_related_when_the_limit_is_zero() {
    with_settings(|settings| settings.inspect_related = 0);
    let mut fake = on_artists(ARTISTS);

    fake.press("i");

    assert!(fake.screen().contains("RECORD  (3)"));
    assert!(!fake.screen().contains("RELATED"));
}

#[test]
fn the_cursor_never_stops_on_a_box_title() {
    let mut fake = on_artists(ARTISTS);
    let on_a_title = |fake: &mut Fake<Browser>| {
        fake.highlighted()
            .iter()
            .any(|run| run.starts_with("RECORD") || run.starts_with("RELATED"))
    };

    fake.press("ik");
    assert!(!on_a_title(&mut fake));

    fake.press("g");
    assert!(!on_a_title(&mut fake));

    fake.press("\tk");
    assert!(!on_a_title(&mut fake));
}

#[test]
fn inspects_a_related_record_and_goes_back_with_escape() {
    let mut fake = on_artists(ARTISTS);

    fake.press("i\t\njj");
    assert!(fake.screen().contains("i Inspect"));

    fake.press("i");
    let screen = fake.screen();
    assert!(screen.contains("artists  ·  belongs to"), "{screen}");
    assert!(screen.contains(&format!("title{}Let There Be Rock", " ".repeat(17))));
    assert!(screen.contains("esc Back"));
    assert_eq!(fake.app.status(), "inspecting albums · esc goes back");

    fake.press("\x1b");
    let screen = fake.screen();
    assert!(screen.contains("▾ albums  ·  has many  (2)"), "{screen}");
    assert!(screen.contains("esc Close"));

    fake.press("\x1b");
    assert!(fake.app.inspector.is_none());
    assert!(!fake.screen().contains("RECORD"));
}

#[test]
fn says_so_when_i_is_not_on_a_related_record() {
    let mut fake = on_artists(ARTISTS);

    fake.press("ii");

    assert_eq!(
        fake.app.status(),
        "i opens a related record, move onto one first"
    );
    assert!(fake.screen().contains("RECORD  (3)"));
}

#[test]
fn will_not_edit_a_related_record_through_the_grid() {
    let mut fake = on_artists(ARTISTS);

    fake.press("i\t\njjie");

    assert!(fake.app.inspector.is_some());
    assert_eq!(
        fake.app.status(),
        "this record is not in the grid, open its table to edit it"
    );
}

#[test]
fn wraps_inside_a_box_and_switches_boxes_with_tab() {
    let mut fake = on_artists(ARTISTS);

    fake.press("i");
    let first = fake.highlighted();
    fake.press("jjj");
    assert_eq!(fake.highlighted(), first);

    fake.press("\t");
    assert!(
        fake.highlighted()
            .iter()
            .any(|run| run.contains("albums  ·  has many"))
    );

    fake.press("\x1b[Z");
    assert!(
        fake.highlighted()
            .iter()
            .any(|run| run.starts_with("id") && run.ends_with("integer"))
    );
}

#[test]
fn says_so_when_there_is_no_row_to_inspect() {
    let mut fake = browser_with(
        "create table things (id integer primary key)",
        Recorded::default(),
    );

    fake.press("\ni");

    assert!(fake.app.inspector.is_none());
    assert_eq!(fake.app.status(), "nothing to open, this table has no rows");
}

#[test]
fn yanks_the_whole_document_or_the_selected_lines() {
    let clipboard = Recorded::default();
    let mut fake = browser_with(ARTISTS, clipboard.clone());
    fake.press("j\n");

    fake.press("iy");
    assert_eq!(clipboard.last().unwrap().lines().count(), 7);
    assert_eq!(fake.app.status(), "yanked 7 lines to the system clipboard");

    fake.press("Vjy");
    let yanked = clipboard.last().unwrap();
    assert_eq!(yanked.lines().count(), 2);
    assert!(yanked.starts_with("  id"));
    assert!(yanked.lines().nth(1).unwrap().contains("AC/DC"));
    assert_eq!(fake.app.status(), "yanked 2 lines to the system clipboard");
}

#[test]
fn clears_a_selection_with_escape_before_closing() {
    let mut fake = on_artists(ARTISTS);

    fake.press("iV\x1b");
    assert!(fake.app.inspector.is_some());
    assert_eq!(fake.app.status(), "selection cleared");

    fake.press("\x1b");
    assert!(fake.app.inspector.is_none());
}

#[test]
fn yanks_the_value_under_the_cursor() {
    let clipboard = Recorded::default();
    let mut fake = browser_with(ARTISTS, clipboard.clone());

    fake.press("j\nly");
    assert_eq!(clipboard.last().as_deref(), Some("AC/DC"));
    assert_eq!(fake.app.status(), "yanked name to the system clipboard");

    fake.press("jly");
    assert_eq!(clipboard.last().as_deref(), Some("NULL"));
    assert_eq!(fake.app.status(), "yanked profile to the system clipboard");
}

#[test]
fn yanks_the_row_as_an_object_with_its_json_decoded() {
    let clipboard = Recorded::default();
    let mut fake = browser_with(ARTISTS, clipboard.clone());

    fake.press("j\nY");

    assert_eq!(
        clipboard.last().unwrap(),
        "{\n    \"id\": 1,\n    \"name\": \"AC/DC\",\n    \"profile\": {\n        \"formed\": 1973,\n        \"members\": [\n            \"Angus\"\n        ]\n    }\n}"
    );
    assert_eq!(fake.app.status(), "yanked row to the system clipboard");
}

#[test]
fn says_when_only_the_terminal_clipboard_was_reachable() {
    let mut fake = browser_with(ARTISTS, Recorded::reaching(Reached::Terminal));

    fake.press("j\ny");

    assert_eq!(fake.app.status(), "yanked id to the terminal clipboard");
}

#[test]
fn yanks_nothing_from_an_empty_table() {
    let clipboard = Recorded::default();
    let mut fake = browser_with(
        "create table things (id integer primary key)",
        clipboard.clone(),
    );

    fake.press("\nyY");

    assert!(clipboard.copies().is_empty());
    assert!(!fake.app.status().contains("yanked"));
}

#[test]
fn yanks_each_value_as_what_it_is_stored_as_whatever_the_column_says() {
    let clipboard = Recorded::default();
    let mut fake = browser_with(
        "create table loose (id integer primary key, n, s text, r);
         insert into loose values (1, 7, '8', 2.5);",
        clipboard.clone(),
    );

    fake.press("\tY");

    assert_eq!(
        clipboard.last().unwrap(),
        "{\n    \"id\": 1,\n    \"n\": 7,\n    \"s\": \"8\",\n    \"r\": 2.5\n}"
    );
}
