use std::path::PathBuf;

use app::database;
use app::settings::{self, Settings};
use app::store::Connection;
use app::terminal::Fake;
use app::tui::{Browser, Exit, Focus, Mode};

const RUN: &str = "\x12";
const ESCAPE: &str = "\x1b";
const UNDO: &str = "\x1a";
const REDO: &str = "\x19";
const PALETTE: &str = "\x0b";

fn temp(name: &str, kind: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!(
        "tql-rust-sql-{}-{name}-{kind}.sqlite",
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
             create table orders (id integer primary key, fruit_id integer);
             insert into fruit values (1, 'apple', 3), (2, 'banana', 1), (3, 'cherry', 9);
             insert into orders values (1, 3);",
        )
        .unwrap();
    path
}

fn store(name: &str) -> PathBuf {
    let path = temp(name, "store");
    rusqlite::Connection::open(&path)
        .unwrap()
        .execute_batch(
            "create table connections (id integer primary key, name text, driver text, host text, port integer,
                database text, username text, password text, read_only integer, tag text, last_used_at text);
             create table query_executions (id integer primary key autoincrement, connection_id integer,
                statement text, source text, succeeded integer, error text, row_count integer,
                duration_ms integer, created_at text, updated_at text);
             insert into connections (id, name, driver, database) values
                (1, 'fruit', 'sqlite', '/tmp/fruit.sqlite'), (2, 'elsewhere', 'sqlite', '/tmp/elsewhere.sqlite');",
        )
        .unwrap();
    path
}

fn opened(name: &str, configure: impl FnOnce(&mut Connection)) -> Fake<Browser> {
    settings::install(Settings::default());
    let mut connection = Connection::sqlite("fruit", fixture(name).to_str().unwrap());
    connection.id = 1;
    configure(&mut connection);
    let database = database::connect(&connection, None).unwrap();
    Fake::sized(
        Browser::new(connection, database)
            .unwrap()
            .with_store(store(name)),
        100,
        30,
    )
}

fn browser(name: &str) -> Fake<Browser> {
    opened(name, |_| {})
}

fn sql_pane(name: &str) -> Fake<Browser> {
    let mut fake = browser(name);
    fake.press("s");
    for _ in 0..fake.app.query.editor.buffer().chars().count() {
        fake.press("\x7f");
    }
    fake
}

fn names(fake: &Fake<Browser>) -> Vec<String> {
    fake.app.rows.iter().map(|row| row[1].clone()).collect()
}

#[test]
fn s_opens_the_editor_on_the_statement_behind_the_grid() {
    let mut fake = browser("opens");

    fake.press("s");

    assert_eq!(fake.app.mode, Mode::Query);
    assert_eq!(
        fake.app.query.editor.buffer(),
        "select * from \"fruit\" order by \"id\" asc limit 100 offset 0"
    );
    let screen = fake.screen();
    assert!(screen.contains("[3] SQL"), "{screen}");
    assert!(screen.contains("ctrl+r runs it · ↵ adds a line · esc returns"));
    assert_eq!(fake.highlighted().last().map(String::as_str), Some(""));
}

#[test]
fn enter_adds_a_line_and_ctrl_r_runs_the_query() {
    let mut fake = sql_pane("runs");

    fake.press("select name from fruit\nwhere qty > 1");
    assert_eq!(
        fake.app.query.editor.buffer(),
        "select name from fruit\nwhere qty > 1"
    );
    assert!(!fake.app.query.results);

    fake.press(RUN);
    assert!(fake.app.query.results);
    assert_eq!(fake.app.headers, ["name"]);
    assert_eq!(fake.app.rows, [["apple"], ["cherry"]]);
    assert!(fake.app.status().starts_with("2 rows · "));

    let screen = fake.screen();
    assert!(screen.contains("[2] fruit"), "{screen}");
    assert!(screen.contains("esc goes back to fruit"));
}

#[test]
fn results_from_no_table_are_titled_results() {
    let mut fake = sql_pane("results-title");

    fake.press("select 1 + 1 as two");
    fake.press(RUN);

    assert!(fake.screen().contains("[2] RESULTS"));
    assert_eq!(fake.app.rows, [["2"]]);
}

#[test]
fn runs_the_statement_under_the_cursor_and_numbers_it_only_when_there_are_several() {
    let mut fake = sql_pane("statements");

    fake.press("select * from fruit where qty > 8;\nselect * from fruit");
    fake.press("\x1b[A");
    fake.press(RUN);
    assert_eq!(fake.app.rows.len(), 1);
    assert!(fake.app.status().ends_with("statement 1 of 2"));

    let mut lone = sql_pane("lone-statement");
    lone.press("select * from fruit;");
    lone.press(RUN);
    assert_eq!(lone.app.rows.len(), 3);
    assert!(!lone.app.status().contains("statement"));
}

#[test]
fn says_so_when_there_is_nothing_to_run() {
    let mut fake = sql_pane("nothing");

    fake.press("  ");
    fake.press(RUN);

    assert_eq!(fake.app.status(), "nothing to run");
}

#[test]
fn tab_indents_shift_tab_outdents_and_both_stay_in_the_pane() {
    let mut fake = sql_pane("indent");

    fake.press("select *\n\tfrom fruit\nwhere qty > 1");
    assert_eq!(
        fake.app.query.editor.buffer(),
        "select *\n  from fruit\n  where qty > 1"
    );

    fake.press("\x1b[Z");
    assert_eq!(fake.app.mode, Mode::Query);
    assert_eq!(
        fake.app.query.editor.buffer(),
        "select *\n  from fruit\nwhere qty > 1"
    );
}

#[test]
fn undoes_a_word_at_a_time_and_redoes() {
    let mut fake = sql_pane("undo");

    fake.press("select name");
    fake.press(UNDO);
    assert_eq!(fake.app.query.editor.buffer(), "select");

    fake.press(UNDO);
    fake.press(UNDO);
    fake.press(UNDO);
    assert!(fake.app.status().contains("oldest"));

    fake.press(REDO);
    fake.press(REDO);
    fake.press(REDO);
    fake.press(REDO);
    fake.press(REDO);
    assert_eq!(fake.app.query.editor.buffer(), "select name");
    assert!(fake.app.status().contains("newest"));
}

#[test]
fn undoes_a_run_of_deletes_and_a_new_line_each_as_one_step() {
    let mut fake = sql_pane("undo-deletes");

    fake.press("abc\x7f\x7f");
    fake.press(UNDO);
    assert_eq!(fake.app.query.editor.buffer(), "abc");

    fake.press("\nfrom");
    fake.press(UNDO);
    assert_eq!(fake.app.query.editor.buffer(), "abc\n");
    fake.press(UNDO);
    assert_eq!(fake.app.query.editor.buffer(), "abc");
}

#[test]
fn escape_returns_to_the_grid_and_again_back_to_the_table() {
    let mut fake = browser("escape");
    fake.press("\ts");
    fake.press("\x7f".repeat(80).as_str());
    fake.press("select name from fruit where id = 2");
    fake.press(RUN);

    fake.press(ESCAPE);
    assert_eq!(fake.app.mode, Mode::Browse);
    assert_eq!(fake.app.focus, Focus::Grid);
    assert!(fake.app.query.results);

    fake.press(ESCAPE);
    assert!(!fake.app.query.results);
    assert_eq!(fake.app.headers, ["id", "name", "qty"]);
    assert_eq!(fake.app.status(), "back to fruit");
}

#[test]
fn sorting_query_results_rewrites_the_statement() {
    let mut fake = browser("sort-results");
    fake.press("\ts");
    fake.press("\x7f".repeat(80).as_str());
    fake.press("select id, name from fruit where qty > 0 limit 10");
    fake.press(RUN);
    fake.press(ESCAPE);
    fake.press("lo");

    assert_eq!(
        fake.app.query.editor.buffer(),
        "select id, name from fruit where qty > 0 order by \"name\" asc limit 10"
    );
    assert!(fake.screen().contains("name ▲"));

    fake.press("o");
    assert_eq!(names(&fake), ["cherry", "banana", "apple"]);
    assert_eq!(fake.app.column_index, 1);

    fake.press("o");
    assert_eq!(
        fake.app.query.editor.buffer(),
        "select id, name from fruit where qty > 0 limit 10"
    );
}

#[test]
fn a_query_too_complex_to_rewrite_is_left_alone() {
    let mut fake = browser("sort-complex");
    fake.press("\ts");
    fake.press("\x7f".repeat(80).as_str());
    fake.press("select id from fruit union select id from orders");
    fake.press(RUN);
    fake.press(ESCAPE);
    fake.press("o");

    assert!(
        fake.app
            .status()
            .starts_with("this query is too complex to sort")
    );
}

#[test]
fn a_failing_statement_opens_the_error_and_any_key_closes_it() {
    let mut fake = sql_pane("error");

    fake.press("select * from nowhere");
    fake.press(RUN);

    let screen = fake.screen();
    assert!(screen.contains("THE DATABASE SAID NO"), "{screen}");
    assert!(screen.contains("no such table: nowhere"));
    assert!(screen.contains("y copies it    esc closes"));

    fake.press("j");
    assert_eq!(fake.app.problem_offset, 1);
    fake.press("k");
    fake.press("k");
    assert_eq!(fake.app.problem_offset, 0);

    fake.press("d");
    assert!(fake.app.problem.is_none());
    assert!(!fake.screen().contains("THE DATABASE SAID NO"));
    assert_eq!(fake.app.mode, Mode::Query);
}

#[test]
fn a_write_says_how_many_rows_it_changed() {
    let mut fake = sql_pane("write");

    fake.press("update fruit set qty = 0 where qty > 2");
    fake.press(RUN);

    assert!(fake.app.status().starts_with("2 rows affected · "));
}

#[test]
fn a_read_only_connection_refuses_to_write() {
    let mut fake = opened("read-only", |connection| connection.read_only = true);
    fake.press("s");
    fake.press("\x7f".repeat(80).as_str());

    fake.press("delete from fruit");
    fake.press(RUN);

    assert!(fake.screen().contains("READ-ONLY"));
    fake.press(ESCAPE);
    fake.press("\x7f".repeat(80).as_str());
    fake.press("select count(*) from fruit");
    fake.press(RUN);
    assert_eq!(fake.app.rows, [["3"]]);
}

#[test]
fn history_records_what_the_editor_ran_and_puts_it_back_without_running_it() {
    let mut fake = sql_pane("history");

    fake.press("select name\n  from fruit");
    fake.press(RUN);
    fake.press(ESCAPE);
    fake.press(ESCAPE);
    fake.press("H");

    let screen = fake.screen();
    assert!(screen.contains("HISTORY"), "{screen}");
    assert!(screen.contains("select name from fruit"));
    assert!(screen.contains("3 rows · "));

    fake.press("name\n");
    assert!(fake.app.palette.is_none());
    assert_eq!(fake.app.mode, Mode::Query);
    assert_eq!(fake.app.query.editor.buffer(), "select name\n  from fruit");
    assert!(!fake.app.query.results);
}

#[test]
fn history_opens_from_the_editor_with_alt_h_and_from_the_command_line() {
    let mut fake = sql_pane("history-keys");
    fake.press("select qty from fruit");
    fake.press(RUN);

    fake.press("\x1bh");
    assert!(fake.app.palette.is_some());
    fake.press(ESCAPE);
    fake.press(ESCAPE);

    fake.press(":history\n");
    assert!(fake.screen().contains("HISTORY"));
}

#[test]
fn history_marks_a_failure_and_says_so_when_it_is_empty() {
    let mut fake = browser("history-empty");

    fake.press("H");
    assert!(fake.app.palette.is_none());
    assert!(fake.app.status().starts_with("no history yet"));

    fake.press("s");
    fake.press("\x7f".repeat(80).as_str());
    fake.press("select nope from fruit");
    fake.press(RUN);
    fake.press(ESCAPE);
    fake.press(ESCAPE);
    fake.press("H");
    assert!(fake.screen().contains("failed · "));
}

#[test]
fn the_palette_lists_actions_with_their_keys_and_runs_them() {
    let mut fake = browser("palette");

    fake.press(PALETTE);
    let screen = fake.screen();
    assert!(screen.contains("COMMANDS"), "{screen}");

    fake.press("filter tables");
    assert!(fake.screen().contains("Filter tables"));
    fake.press("\n");
    assert!(fake.app.palette.is_none());
    assert!(fake.app.filtering);
}

#[test]
fn the_palette_types_j_and_k_and_moves_with_the_arrows() {
    let mut fake = browser("palette-moves");

    fake.press(PALETTE);
    fake.press("jk");
    assert_eq!(fake.app.palette.as_ref().unwrap().query.buffer(), "jk");

    fake.press("\x7f\x7f\x1b[B\x0e");
    assert_eq!(fake.app.palette.as_ref().unwrap().index, 2);
    fake.press("\x10");
    assert_eq!(fake.app.palette.as_ref().unwrap().index, 1);
}

#[test]
fn the_palette_goes_to_a_table_runs_commands_and_asks_in_english() {
    let mut fake = browser("palette-targets");

    fake.press(PALETTE);
    fake.press("orders\n");
    assert_eq!(fake.app.current_table().as_deref(), Some("orders"));
    assert_eq!(fake.app.focus, Focus::Grid);

    fake.press(PALETTE);
    fake.press("write the pending\n");
    assert_eq!(fake.app.status(), "nothing to write");

    fake.press(PALETTE);
    fake.press("ask in english\n");
    assert!(fake.app.question.is_some());
}

#[test]
fn the_palette_opens_another_connection() {
    let mut fake = browser("palette-connection");

    fake.press(PALETTE);
    fake.press("elsewhere\n");

    assert_eq!(fake.exit(), Some(&Exit::Open(2)));
}

#[test]
fn the_palette_opens_from_the_editor_and_leaves_it_for_anything_but_sql() {
    let mut fake = browser("palette-editor");
    fake.press("s");

    fake.press(PALETTE);
    assert!(fake.app.palette.is_some());
    fake.press("go to the rows\n");
    assert_eq!(fake.app.mode, Mode::Browse);
    assert_eq!(fake.app.focus, Focus::Grid);
}

#[test]
fn completion_offers_names_and_tab_takes_one() {
    let mut fake = sql_pane("complete");

    fake.press("select * from fr");
    let completion = fake.app.query.completion.as_ref().unwrap();
    assert_eq!(completion.items[0].label, "fruit");
    assert!(fake.screen().contains("fruit table"));

    fake.press("\t");
    assert_eq!(fake.app.query.editor.buffer(), "select * from fruit");
    assert!(fake.app.query.completion.is_none());

    fake.press(" where na");
    assert_eq!(
        fake.app.query.completion.as_ref().unwrap().items[0].label,
        "name"
    );
    fake.press(ESCAPE);
    assert!(fake.app.query.completion.is_none());
    assert_eq!(fake.app.mode, Mode::Query);
}

#[test]
fn completion_can_be_turned_off() {
    let mut fake = sql_pane("complete-off");
    let mut quiet = Settings::default();
    quiet.set("ui.sql_complete", &toml::Value::Boolean(false));
    settings::install(quiet);

    fake.press("select * from fr");

    assert!(fake.app.query.completion.is_none());
}

#[test]
fn clicking_the_sql_pane_puts_the_cursor_where_you_clicked() {
    let mut fake = browser("click");
    let mut always = Settings::default();
    always.set("ui.sql_always", &toml::Value::Boolean(true));
    settings::install(always);
    fake.screen();

    fake.click(31, 2);
    assert_eq!(fake.app.mode, Mode::Query);
    assert_eq!(fake.app.query.editor.cursor(), 2);

    fake.click(60, 20);
    assert_eq!(fake.app.mode, Mode::Browse);
}

#[test]
fn tab_cycles_into_the_sql_pane_when_it_is_shown() {
    let mut fake = browser("tab-cycle");
    let mut always = Settings::default();
    always.set("ui.sql_always", &toml::Value::Boolean(true));
    settings::install(always);

    fake.press("\t\t");
    assert_eq!(fake.app.mode, Mode::Query);

    fake.press("\x1b2");
    assert_eq!(fake.app.mode, Mode::Browse);
    assert_eq!(fake.app.focus, Focus::Grid);

    fake.press("\x1b3");
    assert_eq!(fake.app.mode, Mode::Query);
}

#[test]
fn a_filtered_table_opens_the_editor_on_its_filtered_statement_and_escape_clears_the_filter_first()
{
    let mut fake = browser("filtered");
    fake.press("\tl").press("f").press("an").press("\x13");
    assert_eq!(names(&fake), ["banana"]);

    fake.press("s");
    assert!(fake.app.query.editor.buffer().contains("like"));
    fake.press(RUN);
    assert!(fake.app.query.results);
    assert_eq!(names(&fake), ["banana"]);

    fake.press(ESCAPE);
    fake.press(ESCAPE);
    assert!(fake.app.filters.is_none());
    assert!(!fake.app.query.results);
    assert_eq!(names(&fake), ["apple", "banana", "cherry"]);
}

#[test]
fn a_change_made_in_the_sql_editor_is_seen_without_reloading() {
    let mut fake = browser("schema-after-write");

    fake.press("\tt");
    assert!(!fake.screen().contains("ripe"));
    fake.press("\x1b");
    fake.press("s")
        .press("\x05")
        .press("; alter table fruit add column ripe integer")
        .press("\x12")
        .press("\x1b")
        .press("\x1b");
    fake.press("t");

    let screen = fake.screen();
    let structure: String = screen
        .lines()
        .skip_while(|line| !line.contains("STRUCTURE"))
        .collect();
    assert!(structure.contains("ripe"), "{screen}");
}

#[test]
fn query_results_offer_yanking_on_the_hotkey_bar() {
    let mut fake = browser("results-bar");

    fake.press("s")
        .press("\x05")
        .press("; select name from fruit")
        .press("\x12")
        .press("\x1b")
        .press("\t");

    let screen = fake.screen();
    assert!(screen.contains("y Yank   Y Row   s Edit query"), "{screen}");
}

#[test]
fn writes_from_the_grid_go_into_the_history_as_tql_s_own() {
    let mut fake = browser("tui-history");

    fake.settle().press("\tle").press("-ripe\x04").press(":w\n");

    let store = std::env::temp_dir().join(format!(
        "tql-rust-sql-{}-tui-history-store.sqlite",
        std::process::id()
    ));
    let recorded: Vec<(String, String)> = rusqlite::Connection::open(store)
        .unwrap()
        .prepare("select source, statement from query_executions order by id")
        .unwrap()
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    assert!(
        recorded.contains(&(
            "tui".to_string(),
            "update \"fruit\" set \"name\" = ? where \"id\" = ?".to_string()
        )),
        "{recorded:?}"
    );
}
