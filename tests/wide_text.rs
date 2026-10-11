use std::sync::atomic::{AtomicUsize, Ordering};

use app::database;
use app::settings::{self, Settings};
use app::store::Connection;
use app::terminal::{Fake, Key};
use app::tui::Browser;
use app::tui::clipboard::Recorded;

const CJK: &str = "日本語のテキストがここにありますとても長いセル日本語のテキストがここにあります";

fn browser(width: u16, height: u16) -> Fake<Browser> {
    static N: AtomicUsize = AtomicUsize::new(0);
    settings::install(Settings::default());
    let path = std::env::temp_dir().join(format!(
        "tql-wide-text-{}-{}.sqlite",
        std::process::id(),
        N.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = std::fs::remove_file(&path);
    rusqlite::Connection::open(&path)
        .unwrap()
        .execute_batch(&format!(
            "create table wide (id integer primary key, note text);
             insert into wide (note) values ('{CJK}');"
        ))
        .unwrap();
    let connection = Connection::sqlite("wide", path.to_str().unwrap());
    let database = database::connect(&connection, None).unwrap();
    let browser = Browser::new(connection, database)
        .unwrap()
        .with_clipboard(Recorded::default())
        .with_ai_environment(|_| None);
    Fake::sized(browser, width, height)
}

fn borders(line: &str) -> Vec<usize> {
    let mut column = 0;
    let mut found = Vec::new();
    for character in line.chars() {
        if character == '│' {
            found.push(column);
        }
        column += unicode_width::UnicodeWidthChar::width(character).unwrap_or(0);
    }
    found
}

fn keeps_borders(screen: &str, needle: &str) {
    let lines: Vec<&str> = screen.lines().collect();
    let at = lines
        .iter()
        .position(|line| line.contains(needle))
        .unwrap_or_else(|| panic!("no line with {needle:?}\n{screen}"));
    assert_eq!(
        borders(lines[at]),
        borders(lines[at - 1]),
        "a border moved or was eaten\n{screen}"
    );
}

fn line_with<'a>(screen: &'a str, needle: &str) -> &'a str {
    screen
        .lines()
        .find(|line| line.contains(needle))
        .unwrap_or_else(|| panic!("no line with {needle:?}\n{screen}"))
}

#[test]
fn sql_editor_wraps_wide_characters_by_display_width() {
    let mut fake = browser(80, 24);
    fake.key(Key::Char('s'));
    fake.paste(&format!(" -- {CJK}"));
    let screen = fake.screen();

    let row = line_with(&screen, "長いセル日本");
    assert!(row.ends_with('│'), "right border eaten: {row:?}\n{screen}");
    let lines: Vec<&str> = screen.lines().collect();
    let at = lines.iter().position(|line| *line == row).unwrap();
    assert!(
        lines[at + 1].contains("語のテキストがここにあります"),
        "tail of the line never shown\n{screen}"
    );
}

#[test]
fn value_viewer_keeps_its_border_with_wide_characters() {
    let mut fake = browser(100, 30);
    fake.key(Key::Tab);
    fake.key(Key::Char('l'));
    fake.key(Key::Char('I'));
    fake.settle();
    let screen = fake.screen();

    let row = line_with(&screen, "▸ 1 日本語");
    assert!(
        row.contains("セル日本語のテキストがここにあります"),
        "tail cut although the modal has room: {row:?}"
    );
}

#[test]
fn record_form_keeps_its_border_with_wide_characters() {
    let mut fake = browser(100, 30);
    fake.key(Key::Tab);
    fake.key(Key::Char('E'));
    fake.settle();
    let screen = fake.screen();

    keeps_borders(&screen, "note (text)");
}

#[test]
fn filter_value_keeps_its_border_with_wide_characters() {
    let mut fake = browser(100, 30);
    fake.key(Key::Tab);
    fake.key(Key::Char('l'));
    fake.key(Key::Char('f'));
    fake.settle();
    fake.paste(CJK);
    let screen = fake.screen();

    keeps_borders(&screen, "where note");
}
