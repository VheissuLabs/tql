use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

use app::database;
use app::settings::{self, Settings};
use app::store::Connection;
use app::tui::clipboard::Recorded;
use app::tui::vim::VimMode;
use app::tui::{Browser, Mode};
use zerorust_tui::Fake;

static OPENED: AtomicUsize = AtomicUsize::new(0);

const REDO: &str = "\x12";

fn temp(name: &str, kind: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!(
        "tql-rust-vim-{}-{name}-{kind}.sqlite",
        std::process::id()
    ));
    let _ = std::fs::remove_file(&path);
    path
}

fn fixture(name: &str) -> PathBuf {
    let path = temp(name, "fixture");
    rusqlite::Connection::open(&path)
        .unwrap()
        .execute_batch(
            "create table fruit (id integer primary key, name text, qty integer);
             insert into fruit (name, qty) values ('cherry', 3), ('apple', 9), ('banana', 1);",
        )
        .unwrap();
    path
}

fn vim_settings() -> Settings {
    let mut vim = Settings::default();
    vim.set("ui.sql_editor", &toml::Value::String("vim".to_string()));
    vim
}

fn opened(name: &str, clipboard: Recorded) -> Fake<Browser> {
    settings::install(vim_settings());
    let connection = Connection::sqlite("fruit", fixture(name).to_str().unwrap());
    let database = database::connect(&connection, None).unwrap();
    let mut fake = Fake::sized(
        Browser::new(connection, database)
            .unwrap()
            .with_clipboard(clipboard),
        100,
        30,
    );
    fake.press("s");
    fake
}

fn vim_on(marked: &str) -> Fake<Browser> {
    let name = OPENED.fetch_add(1, Ordering::Relaxed).to_string();
    let mut fake = opened(&name, Recorded::default());
    place(&mut fake, marked);
    fake
}

fn place(fake: &mut Fake<Browser>, marked: &str) {
    let at = marked
        .chars()
        .position(|character| character == '|')
        .unwrap();
    fake.app.query.editor.set(&marked.replace('|', ""));
    fake.app.query.editor.move_to(at);
}

fn keys(fake: &mut Fake<Browser>, typed: &str) {
    for character in typed.chars() {
        fake.press(&character.to_string());
    }
}

fn with_cursor(fake: &Fake<Browser>) -> String {
    let buffer: Vec<char> = fake.app.query.editor.buffer().chars().collect();
    let at = fake.app.query.editor.cursor();
    format!(
        "{}|{}",
        buffer[..at].iter().collect::<String>(),
        buffer[at..].iter().collect::<String>()
    )
}

fn after(from: &str, typed: &str) -> (String, Fake<Browser>) {
    let mut fake = vim_on(from);
    keys(&mut fake, typed);
    (with_cursor(&fake), fake)
}

fn check(cases: &[(&str, &str, &str)]) {
    for (from, typed, to) in cases {
        let (landed, _) = after(from, typed);
        assert_eq!(&landed, to, "{from:?} after {typed:?}");
    }
}

#[test]
fn moves_with_motions_and_counts() {
    check(&[
        ("|select name from fruit", "w", "select |name from fruit"),
        ("|select name from fruit", "3w", "select name from |fruit"),
        ("select name from |fruit", "b", "select name |from fruit"),
        ("|select name from fruit", "e", "selec|t name from fruit"),
        ("a\nb\n|c", "gg", "|a\nb\nc"),
        ("|a\nb\nc", "G", "a\nb\n|c"),
        ("|a\nb\nc", "2G", "a\n|b\nc"),
        ("|a\nb\nc", "2gg", "a\n|b\nc"),
        ("  sel|ect", "^", "  |select"),
        ("|select a, b, c", "f,", "select a|, b, c"),
        ("|select a, b, c", "f,;", "select a, b|, c"),
        ("|select a, b, c", "f,;,", "select a|, b, c"),
        ("|select a, b, c", "t,", "select |a, b, c"),
        ("|count(id)", "%", "count(id|)"),
        ("|a\nb\n\nc", "}", "a\nb\n|\nc"),
        ("a\nb\n\n|c", "{", "a\nb\n|\nc"),
        ("|a\n  b", "\n", "a\n  |b"),
        ("|select", "$", "selec|t"),
        ("|ab\nabcdef\nab", "j$j", "ab\nabcdef\na|b"),
        ("abcd|ef\nab\nabcdef", "jj", "abcdef\nab\nabcd|ef"),
        ("|select", "5l", "selec|t"),
    ]);
}

#[test]
fn deletes_with_d_and_a_motion_or_text_object() {
    check(&[
        ("|select name from", "dw", "|name from"),
        ("|select name from", "d2w", "|from"),
        ("|select name from", "2dw", "|from"),
        ("|a b c d e f g", "2d3w", "|g"),
        ("|select name", "de", "| name"),
        ("sel|ect name", "d$", "se|l"),
        ("sel|ect name", "D", "se|l"),
        ("select |name", "db", "|name"),
        ("select |name\nfrom", "dw", "select| \nfrom"),
        ("a\n|b\nc\nd", "dj", "a\n|d"),
        ("a\nb\n|c", "dk", "|a"),
        ("a\n|b\nc", "dG", "|a"),
        ("a\n|b\nc", "dgg", "|c"),
        ("a\n|b\nc\nd", "2dd", "a\n|d"),
        ("a\nb\n|c", "dd", "a\n|b"),
        ("select |a, b, c", "dt,", "select |, b, c"),
        ("|select a, b", "df,", "| b"),
        ("select na|me from", "diw", "select | from"),
        ("select na|me from", "daw", "select |from"),
        ("count(i|d)", "di(", "count(|)"),
        ("count(i|d)", "da(", "coun|t"),
        ("count(i|d)", "dib", "count(|)"),
        ("where name = 'ch|erry'", "di'", "where name = '|'"),
        ("select \"na|me\" from", "da\"", "select |from"),
        ("a\n\n|b\n\nc", "dap", "a\n\n|c"),
        ("select |name", "dz", "select |name"),
        ("|select a", "d\x1bw", "select |a"),
    ]);
}

#[test]
fn changes_with_c_and_leaves_you_in_insert() {
    for (from, typed, to) in [
        ("|select name", "cw", "| name"),
        ("|select name", "c2w", "|"),
        ("select na|me from", "ciw", "select | from"),
        ("count(i|d)", "ci(", "count(|)"),
        ("where x in (\n  sel|ect 1\n)", "ci(", "where x in (|)"),
        ("sel|ect name", "C", "sel|"),
        ("sel|ect name", "c$", "sel|"),
        ("a\n  b|b\nc", "cc", "a\n  |\nc"),
        ("a\n  b|b\nc", "S", "a\n  |\nc"),
        ("|select", "s", "|elect"),
        ("|select", "3s", "|ect"),
    ] {
        let (landed, fake) = after(from, typed);
        assert_eq!(landed, to, "{from:?} after {typed:?}");
        assert_eq!(
            fake.app.query.vim.mode,
            VimMode::Insert,
            "{from:?} after {typed:?}"
        );
    }
}

#[test]
fn edits_with_the_shortcuts() {
    check(&[
        ("|select", "x", "|elect"),
        ("|select", "3x", "|ect"),
        ("selec|t", "x", "sele|c"),
        ("sel|ect", "X", "se|ect"),
        ("sel|ect", "2X", "s|ect"),
        ("|select", "rS", "|Select"),
        ("|select", "3rx", "xx|xect"),
        ("|select", "9rx", "|select"),
        ("|select\n  name\nfrom", "J", "select| name\nfrom"),
        ("|select\n  name\nfrom", "3J", "select name| from"),
        ("|select\n)", "J", "select|)"),
        ("|select", "~", "S|elect"),
        ("|select", "3~", "SEL|ect"),
        ("|a\nb", ">>", "  |a\nb"),
        ("|a\nb", "2>>", "  |a\n  b"),
        ("|a\nb", ">j", "  |a\n  b"),
        ("    |a\nb", "<<", "  |a\nb"),
        ("a\n\n|b", ">k", "a\n|\n  b"),
    ]);
}

#[test]
fn yanks_and_puts() {
    check(&[
        ("|select name", "yw$p", "select nameselect| "),
        ("|select name", "ywP", "select| select name"),
        ("|a\nb", "yyp", "a\n|a\nb"),
        ("a\n|b", "yyP", "a\n|b\nb"),
        ("|a\nb", "yy2p", "a\n|a\na\nb"),
        ("|a\nb", "Yjp", "a\nb\n|a"),
        ("|a\nb", "ddp", "b\n|a"),
        ("|ab", "xp", "b|a"),
        ("sel|ect", "yiwP", "selec|tselect"),
        ("|select", "p", "|select"),
    ]);
}

#[test]
fn copies_a_yank_to_the_clipboard_too() {
    let clipboard = Recorded::default();
    let mut fake = opened("clipboard", clipboard.clone());
    place(&mut fake, "|select");
    keys(&mut fake, "yiw");

    assert_eq!(fake.app.query.vim.register, "select");
    assert_eq!(fake.app.status(), "yanked 6 characters");
    assert_eq!(clipboard.copies(), ["select"]);

    keys(&mut fake, "yy");
    assert_eq!(fake.app.status(), "yanked 1 line");
    assert_eq!(clipboard.last().as_deref(), Some("select\n"));

    keys(&mut fake, "x");
    assert_eq!(clipboard.copies().len(), 2);
}

#[test]
fn undoes_and_redoes_a_change_at_a_time() {
    let mut fake = vim_on("|select name from");
    keys(&mut fake, "dwdw");
    assert_eq!(fake.app.query.editor.buffer(), "from");

    keys(&mut fake, "u");
    assert_eq!(fake.app.query.editor.buffer(), "name from");
    keys(&mut fake, "u");
    assert_eq!(fake.app.query.editor.buffer(), "select name from");
    keys(&mut fake, "u");
    assert_eq!(fake.app.query.editor.buffer(), "select name from");
    assert!(fake.app.status().contains("oldest"));

    fake.press(REDO);
    assert_eq!(fake.app.query.editor.buffer(), "name from");
    assert!(!fake.app.query.results);

    fake.press(REDO).press(REDO);
    assert_eq!(fake.app.query.editor.buffer(), "from");
    assert!(fake.app.status().contains("newest"));
}

#[test]
fn undoes_a_whole_insert_as_one_step() {
    let mut fake = vim_on("|");
    keys(&mut fake, "iselect name\nfrom fruit\x1b");
    assert_eq!(fake.app.query.editor.buffer(), "select name\nfrom fruit");

    keys(&mut fake, "u");
    assert_eq!(fake.app.query.editor.buffer(), "");
}

#[test]
fn drops_the_redo_history_after_a_new_change() {
    let mut fake = vim_on("|a b c");
    keys(&mut fake, "dwux");
    fake.press(REDO);

    assert_eq!(fake.app.query.editor.buffer(), " b c");
}

#[test]
fn repeats_the_last_change_with_dot() {
    check(&[
        ("|a b c d", "dw.", "|c d"),
        ("|a b c d e f", "dw3.", "|e f"),
        ("|a b c d e", "2dw.", "|e"),
        ("|one two", "ciwX\x1bw.", "X |X"),
        ("|a\nb", "Ax\x1bj.", "ax\nb|x"),
        ("|a\nb\nc", "dd.", "|c"),
        ("|a\nb", "oz\x1b.", "a\nz\n|z\nb"),
        ("|abc", "x.", "|c"),
        ("|a b", "ywP.", "aa|  a b"),
    ]);
}

#[test]
fn leaves_insert_on_the_last_character_typed() {
    let (landed, mut fake) = after("|", "iselect\x1b");
    assert_eq!(landed, "selec|t");

    keys(&mut fake, "I\x1b");
    assert_eq!(with_cursor(&fake), "|select");
}

#[test]
fn never_sits_past_the_last_character_in_normal_mode() {
    check(&[("|select", "A\x1bl", "selec|t")]);
}

#[test]
fn deletes_a_word_or_the_line_so_far_in_insert_mode() {
    let mut fake = vim_on("|");
    keys(&mut fake, "iselect name\x17");
    assert_eq!(fake.app.query.editor.buffer(), "select ");

    keys(&mut fake, "from\x15");
    assert_eq!(fake.app.query.editor.buffer(), "");
}

#[test]
fn selects_characters_shows_visual_in_the_title_and_deletes_them() {
    let mut fake = vim_on("|select name");
    keys(&mut fake, "v");
    assert_eq!(fake.app.query.vim.mode, VimMode::Visual);
    assert!(fake.screen().contains("[3] SQL · VISUAL"));

    keys(&mut fake, "e");
    assert!(fake.highlighted().contains(&"select".to_string()));

    keys(&mut fake, "d");
    assert_eq!(with_cursor(&fake), "| name");
    assert_eq!(fake.app.query.vim.mode, VimMode::Normal);
}

#[test]
fn selects_lines_and_deletes_them() {
    let mut fake = vim_on("a\n|b\nc\nd");
    keys(&mut fake, "V");
    assert!(fake.screen().contains("[3] SQL · VISUAL LINE"));

    keys(&mut fake, "jd");
    assert_eq!(with_cursor(&fake), "a\n|d");
}

#[test]
fn selects_with_a_text_object_and_yanks() {
    let (landed, fake) = after("count(i|d)", "vi(y");
    assert_eq!(fake.app.query.vim.register, "id");
    assert_eq!(landed, "count(|id)");
}

#[test]
fn swaps_the_ends_of_a_selection_with_o() {
    check(&[("sel|ect name", "veobd", "| name")]);
}

#[test]
fn acts_on_the_selection() {
    check(&[
        ("|a\nb\nc", "Vj>", "  |a\n  b\nc"),
        ("  |a\n  b", "Vj<", "|a\nb"),
        ("|select", "vll~", "|SELect"),
        ("|a\nb\nc", "VjJ", "a| b\nc"),
        ("|select", "vlx", "|lect"),
        ("|select name", "vec", "| name"),
    ]);
}

#[test]
fn switches_between_v_and_capital_v_and_leaves() {
    let mut fake = vim_on("|select");
    keys(&mut fake, "vV");
    assert_eq!(fake.app.query.vim.mode, VimMode::VisualLine);
    keys(&mut fake, "v");
    assert_eq!(fake.app.query.vim.mode, VimMode::Visual);
    keys(&mut fake, "v");
    assert_eq!(fake.app.query.vim.mode, VimMode::Normal);

    keys(&mut fake, "V\x1b");
    assert_eq!(fake.app.query.vim.mode, VimMode::Normal);
    assert_eq!(fake.app.mode, Mode::Query);
}

#[test]
fn runs_just_the_selection_with_colon_r() {
    let mut fake = vim_on("select name from fruit\n|select name from fruit where qty > 8");
    keys(&mut fake, "V:r\n");

    assert!(fake.app.query.results);
    assert_eq!(fake.app.rows, [["apple"]]);
    assert_eq!(fake.app.query.vim.mode, VimMode::Normal);
    assert!(fake.app.query.editor.buffer().contains('\n'));
}

#[test]
fn colon_r_runs_the_query_from_the_sql_editor() {
    let mut fake = vim_on("|select * from fruit where qty > 8");
    keys(&mut fake, ":r");
    assert!(fake.screen().contains("runs the query"));

    keys(&mut fake, "\n");
    assert!(fake.app.query.results);
    assert_eq!(fake.app.raw.len(), 1);
    assert_eq!(fake.app.mode, Mode::Query);
}

#[test]
fn colon_run_runs_the_query_too() {
    let (_, fake) = after("|select * from fruit where qty > 8", ":run\n");
    assert_eq!(fake.app.raw.len(), 1);
}

#[test]
fn colon_r_still_reloads_away_from_the_sql_editor() {
    let mut fake = vim_on("|select * from fruit where qty > 8");
    keys(&mut fake, "\x1b:r");
    assert!(fake.screen().contains("reloads"));

    keys(&mut fake, "\n");
    assert!(!fake.app.query.results);
}

#[test]
fn offers_colon_r_in_the_hotkey_bar_for_each_mode() {
    let mut fake = vim_on("|select 1");
    let screen = fake.screen();
    assert!(
        screen.contains(":r Run") && screen.contains("i Insert"),
        "{screen}"
    );
    assert!(!screen.contains("ctrl+r"));
    assert!(screen.contains("i types · :r runs it · esc returns"));

    keys(&mut fake, "i");
    assert!(fake.screen().contains("tab Indent"));

    keys(&mut fake, "\x1bv");
    assert!(fake.screen().contains(":r Run selection"));
}

#[test]
fn keeps_the_indentation_on_a_new_line() {
    check(&[
        ("  |where a = 1", "A\nand", "  where a = 1\n  and|"),
        ("  |where a = 1", "oand", "  where a = 1\n  and|"),
        ("  |where a = 1", "Oand", "  and|\n  where a = 1"),
        ("|where a = 1", "oand", "where a = 1\nand|"),
    ]);
}

#[test]
fn runs_the_statement_under_the_cursor_with_colon_r() {
    let mut fake = vim_on("select * from fruit where qty > 8;\nselect * from |fruit");
    keys(&mut fake, ":r\n");
    assert_eq!(fake.app.raw.len(), 3);
    assert!(fake.app.status().contains("statement 2 of 2"));

    keys(&mut fake, "gg:r\n");
    assert_eq!(fake.app.raw.len(), 1);
    assert!(fake.app.status().contains("statement 1 of 2"));
}

#[test]
fn types_at_the_terminal_cursor_in_insert_mode_and_paints_it_otherwise() {
    let mut fake = vim_on("|select");
    assert!(!fake.app.inserts_at_terminal_cursor());
    assert!(fake.highlighted().contains(&"s".to_string()));

    keys(&mut fake, "i");
    assert!(fake.app.inserts_at_terminal_cursor());
    assert!(fake.screen().contains("[3] SQL · INSERT"));
    assert!(!fake.highlighted().contains(&"s".to_string()));

    keys(&mut fake, "\x1b");
    assert!(!fake.app.inserts_at_terminal_cursor());
}

#[test]
fn completes_names_in_insert_mode_and_dot_repeats_the_completed_word() {
    let mut fake = vim_on("|");
    keys(&mut fake, "iselect * from fr");
    assert_eq!(
        fake.app.query.completion.as_ref().unwrap().items[0].label,
        "fruit"
    );

    keys(&mut fake, "\t");
    assert_eq!(fake.app.query.editor.buffer(), "select * from fruit");
    assert_eq!(fake.app.query.vim.mode, VimMode::Insert);

    keys(&mut fake, " where na\x1b");
    assert!(fake.app.query.completion.is_none());
    assert_eq!(fake.app.query.vim.mode, VimMode::Insert);

    keys(&mut fake, "\x1b");
    assert_eq!(fake.app.query.vim.mode, VimMode::Normal);
    keys(&mut fake, "u");
    assert_eq!(fake.app.query.editor.buffer(), "");
}

#[test]
fn escape_leaves_the_editor_and_tab_moves_to_the_next_pane_in_normal_mode() {
    let mut fake = vim_on("|select 1");
    keys(&mut fake, "\t");
    assert_eq!(fake.app.mode, Mode::Browse);

    keys(&mut fake, "s");
    assert_eq!(fake.app.query.vim.mode, VimMode::Normal);
    keys(&mut fake, "\x1b");
    assert_eq!(fake.app.mode, Mode::Browse);
}

#[test]
fn reopening_the_editor_starts_in_normal_mode() {
    let mut fake = vim_on("|select 1");
    keys(&mut fake, "i");
    fake.press("\x1b");
    fake.press("\x1b");
    assert_eq!(fake.app.mode, Mode::Browse);

    keys(&mut fake, "s");
    assert_eq!(fake.app.query.vim.mode, VimMode::Normal);
    assert!(fake.screen().contains("[3] SQL · NORMAL"));
}

#[test]
fn insert_mode_asks_for_a_bar_cursor() {
    use zerorust_tui::{CursorShape, Tui};

    let mut fake = opened("cursor-shape", Recorded::default());
    assert_eq!(fake.app.cursor_shape(), CursorShape::Default);

    fake.press("i");
    assert_eq!(fake.app.cursor_shape(), CursorShape::Bar);

    fake.press("\x1b");
    assert_eq!(fake.app.cursor_shape(), CursorShape::Default);
}
