use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

use app::database::{self, Column, Index, Link, links};
use app::store::Connection;
use app::terminal::Fake;
use app::tui::Browser;

const LINKED: &str = "
    create table artists (id integer primary key, name text not null);
    create table albums (
        id integer primary key,
        title text not null default 'untitled',
        artist_id integer references artists(id)
    );
    create index albums_artist_id on albums (artist_id);
    insert into artists (name) values ('AC/DC'), ('Accept');
    insert into albums (title, artist_id) values ('Let There Be Rock', 1), ('Balls to the Wall', 2);
";

fn fixture(sql: &str) -> PathBuf {
    static FIXTURES: AtomicUsize = AtomicUsize::new(0);
    let path = std::env::temp_dir().join(format!(
        "tql-rust-{}-links-{}.sqlite",
        std::process::id(),
        FIXTURES.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = std::fs::remove_file(&path);
    rusqlite::Connection::open(&path)
        .unwrap()
        .execute_batch(sql)
        .unwrap();
    path
}

fn browser(sql: &str) -> Fake<Browser> {
    let path = fixture(sql);
    let connection = Connection::sqlite("links", path.to_str().unwrap());
    let database = database::connect(&connection, None).unwrap();
    let mut fake = Fake::sized(Browser::new(connection, database).unwrap(), 120, 30);
    fake.press("\n");
    fake
}

fn status_line(fake: &mut Fake<Browser>) -> String {
    fake.screen().lines().last().unwrap_or_default().to_string()
}

#[test]
fn reads_columns_indexes_and_foreign_keys_from_sqlite() {
    let path = fixture(&format!(
        "{LINKED} create table regions (code text, region text, primary key (code, region));
         create table visits (id integer primary key, artist_id integer references artists);"
    ));
    let mut database =
        database::connect(&Connection::sqlite("schema", path.to_str().unwrap()), None).unwrap();

    assert_eq!(
        database.columns("albums").unwrap()[1],
        Column {
            name: "title".into(),
            type_name: "text".into(),
            nullable: false,
            default: Some("'untitled'".into()),
            auto_increment: false,
        }
    );
    assert!(database.columns("albums").unwrap()[0].auto_increment);
    assert_eq!(
        database.indexes("albums").unwrap(),
        [
            Index {
                name: "albums_artist_id".into(),
                columns: vec!["artist_id".into()],
                unique: false,
                primary: false,
            },
            Index {
                name: "primary".into(),
                columns: vec!["id".into()],
                unique: true,
                primary: true,
            },
        ]
    );
    assert_eq!(
        database.indexes("regions").unwrap()[0].columns,
        ["code", "region"]
    );

    let keys = database.foreign_keys().unwrap();
    let artist = Link {
        table: "artists".into(),
        column: "id".into(),
    };
    assert_eq!(
        links(&keys["albums"]),
        [("artist_id".into(), artist.clone())]
    );
    assert_eq!(links(&keys["visits"]), [("artist_id".into(), artist)]);
}

#[test]
fn shows_the_structure_of_the_table_on_t() {
    let mut fake = browser(LINKED);

    fake.press("t");
    let screen = fake.screen();

    assert!(screen.contains("STRUCTURE  ·  albums"), "{screen}");
    assert!(screen.contains("id         integer       primary key  ·  auto"));
    assert!(screen.contains("title      text          not null  ·  default 'untitled'"));
    assert!(screen.contains("artist_id  integer       → artists.id"));
    assert!(screen.contains("indexes"));
    assert!(screen.contains("albums_artist_id  (artist_id)"));
    assert!(screen.contains("primary  (id)  primary unique"));
}

#[test]
fn closes_the_structure_with_t_q_or_escape() {
    for key in ["t", "q", "\x1b"] {
        let mut fake = browser(LINKED);

        fake.press("t").press(key);

        assert!(fake.app.structure.is_none(), "{key:?} left it open");
        assert!(!fake.screen().contains("STRUCTURE"));
    }
}

#[test]
fn scrolls_a_structure_that_runs_past_its_box() {
    let path = fixture(LINKED);
    let connection = Connection::sqlite("links", path.to_str().unwrap());
    let database = database::connect(&connection, None).unwrap();
    let mut fake = Fake::sized(Browser::new(connection, database).unwrap(), 80, 14);
    fake.press("\nt");

    assert!(fake.screen().contains("j / ↓ for more"));

    fake.press("G");
    assert!(!fake.screen().contains("j / ↓ for more"));
    assert!(fake.screen().contains("albums_artist_id"));

    fake.press("g");
    assert!(fake.screen().contains("j / ↓ for more"));
}

#[test]
fn hints_where_l_goes_only_on_a_link() {
    let mut fake = browser(LINKED);

    assert!(!status_line(&mut fake).contains("L → artists"));

    fake.press("ll");
    assert!(status_line(&mut fake).contains("L → artists"));
    assert!(!fake.screen().contains("artist_id →"));
}

#[test]
fn follows_a_foreign_key_to_the_row_it_points_at() {
    let mut fake = browser(LINKED);

    fake.press("llL");
    let screen = fake.screen();

    assert_eq!(fake.app.current_table().as_deref(), Some("artists"));
    assert_eq!(fake.app.rows, [["1", "AC/DC"]]);
    assert!(screen.contains("[2] artists ·  filtered"), "{screen}");
    assert!(
        fake.app
            .status()
            .contains("followed artist_id → artists  ·  esc goes back")
    );
    assert!(
        fake.app
            .last_statement
            .as_deref()
            .unwrap()
            .contains("where \"id\" = 1 order by \"id\" asc")
    );
}

#[test]
fn goes_back_with_ctrl_o_or_escape() {
    for back in ["\x0f", "\x1b"] {
        let mut fake = browser(LINKED);

        fake.press("jllL").press(back);

        assert_eq!(fake.app.current_table().as_deref(), Some("albums"));
        assert_eq!(fake.app.rows.len(), 2);
        assert_eq!(fake.app.row_index, 1);
        assert!(fake.app.filters.is_none());
        assert_eq!(fake.app.status(), "back in albums");
    }
}

#[test]
fn says_so_when_there_is_nowhere_to_go_back_to() {
    let mut fake = browser(LINKED);

    fake.press("\x0f");
    assert_eq!(fake.app.status(), "nowhere to go back to");

    fake.press("\x1b");
    assert_eq!(fake.app.current_table().as_deref(), Some("albums"));
}

#[test]
fn says_so_when_the_column_is_not_a_link_and_nothing_points_here() {
    let mut fake = browser(LINKED);

    fake.press("L");

    assert_eq!(fake.app.current_table().as_deref(), Some("albums"));
    assert_eq!(
        fake.app.status(),
        "nothing links to albums, and id is not a foreign key"
    );
}

#[test]
fn does_not_follow_an_empty_foreign_key() {
    let mut fake = browser(&format!(
        "{LINKED} insert into albums (title, artist_id) values ('Orphan', null);"
    ));

    fake.press("jjllL");

    assert_eq!(fake.app.current_table().as_deref(), Some("albums"));
    assert_eq!(fake.app.status(), "artist_id is empty on this row");
}

#[test]
fn follows_a_row_backwards_to_what_references_it() {
    let mut fake = browser(LINKED);

    fake.press("\x1b1jL");

    assert_eq!(fake.app.current_table().as_deref(), Some("albums"));
    assert_eq!(fake.app.rows, [["1", "Let There Be Rock", "1"]]);
    assert_eq!(
        fake.app.status(),
        "followed → albums where artist_id is 1  ·  esc goes back"
    );

    fake.press("\x1b");
    assert_eq!(fake.app.current_table().as_deref(), Some("artists"));
}

#[test]
fn offers_a_list_when_several_tables_reference_the_row() {
    let mut fake = browser(&format!(
        "{LINKED} create table gigs (id integer primary key, artist_id integer references artists(id), city text);
         insert into gigs (artist_id, city) values (1, 'Toronto');"
    ));

    fake.press("\x1b1jL");
    let screen = fake.screen();
    assert!(screen.contains("REFERENCED BY"), "{screen}");
    assert!(screen.contains("albums.artist_id"));
    assert!(screen.contains("gigs.artist_id"));

    fake.press("gigs\n");
    assert_eq!(fake.app.current_table().as_deref(), Some("gigs"));
    assert_eq!(fake.app.rows, [["1", "1", "Toronto"]]);
    assert!(!fake.screen().contains("REFERENCED BY"));
}

#[test]
fn leaves_the_list_alone_on_escape() {
    let mut fake = browser(&format!(
        "{LINKED} create table gigs (id integer primary key, artist_id integer references artists(id));"
    ));

    fake.press("\x1b1jL\x1b");

    assert!(!fake.screen().contains("REFERENCED BY"));
    assert_eq!(fake.app.current_table().as_deref(), Some("artists"));
}

#[test]
fn unwinds_a_chain_of_links_one_step_at_a_time() {
    let mut fake = browser(LINKED);

    fake.press("llLL");
    assert_eq!(fake.app.current_table().as_deref(), Some("albums"));
    assert!(fake.app.filters.is_some());

    fake.press("\x1b");
    assert_eq!(fake.app.current_table().as_deref(), Some("artists"));

    fake.press("\x1b");
    assert_eq!(fake.app.current_table().as_deref(), Some("albums"));
    assert!(fake.app.filters.is_none());
}

#[test]
fn follows_a_link_to_a_table_the_sidebar_filter_is_hiding() {
    let mut fake = browser(LINKED);

    fake.press("\x1b1/album\n\tllL");

    assert_eq!(fake.app.current_table().as_deref(), Some("artists"));
    assert_eq!(fake.app.filter, "");
    assert_eq!(fake.app.rows, [["1", "AC/DC"]]);
}

#[test]
fn keeps_a_sidebar_filter_that_still_shows_the_table() {
    let mut fake = browser(LINKED);

    fake.press("\x1b1/a\n\tllL");

    assert_eq!(fake.app.current_table().as_deref(), Some("artists"));
    assert_eq!(fake.app.filter, "a");

    fake.press("\x1b");
    assert_eq!(fake.app.current_table().as_deref(), Some("albums"));
}

#[test]
fn offers_a_way_back_in_the_hotkey_bar_after_a_jump() {
    let mut fake = browser(LINKED);
    let hotkeys = |fake: &mut Fake<Browser>| fake.screen().lines().nth(27).unwrap().to_string();

    assert!(!hotkeys(&mut fake).contains("esc Back"));

    fake.press("llL");
    assert!(hotkeys(&mut fake).contains("esc Back"));
}

#[test]
fn escape_unwinds_the_jump_before_the_filter_it_came_back_to() {
    let mut fake = browser(LINKED);

    fake.press("lf").press("Rock").press("\x13");
    assert_eq!(fake.app.rows, [["1", "Let There Be Rock", "1"]]);

    fake.press("llL");
    assert_eq!(fake.app.current_table().as_deref(), Some("artists"));
    assert!(fake.screen().contains("[2] artists ·  filtered"));

    fake.press("\x1b");
    assert_eq!(fake.app.current_table().as_deref(), Some("albums"));
    assert_eq!(fake.app.rows, [["1", "Let There Be Rock", "1"]]);
    assert!(fake.screen().contains("[2] albums ·  filtered"));

    fake.press("\x1b");
    assert!(fake.app.filters.is_none());
    assert_eq!(fake.app.rows.len(), 2);
    assert_eq!(fake.app.status(), "filter cleared");
}

#[test]
fn query_results_follow_no_links_and_escape_goes_back_to_the_table() {
    let mut fake = browser(LINKED);

    fake.press("s").press("\x12").press("\x1b");
    assert!(fake.app.query.results);

    fake.press("llL");
    assert_eq!(
        fake.app.status(),
        "nothing links to albums, and artist_id is not a foreign key"
    );
    assert!(!status_line(&mut fake).contains("L → artists"));

    fake.press("\x1b");
    assert!(!fake.app.query.results);
    assert_eq!(fake.app.status(), "back to albums");
}
