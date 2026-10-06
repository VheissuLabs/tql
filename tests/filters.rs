use std::path::PathBuf;

use app::database::{self, Filter, Filters, Joiner, Sort};
use app::store::Connection;
use app::tui::{Browser, Cell, Focus};
use zerorust_tui::Fake;

const ESCAPE: &str = "\x1b";
const SHIFT_TAB: &str = "\x1b[Z";
const RIGHT: &str = "\x1b[C";
const DOWN: &str = "\x1b[B";
const SAVE: &str = "\x13";

fn fixture(name: &str, sql: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!(
        "tql-rust-filters-{}-{name}.sqlite",
        std::process::id()
    ));
    let _ = std::fs::remove_file(&path);
    rusqlite::Connection::open(&path)
        .unwrap()
        .execute_batch(sql)
        .unwrap();
    path
}

fn people(name: &str) -> Fake<Browser> {
    let path = fixture(
        name,
        "create table people (id integer primary key, name text, age integer, city text);
         create table widgets (id integer primary key, label text);
         insert into widgets (label) values ('one');
         insert into people (name, age, city) values
             ('Karl', 41, 'Toronto'), ('Karla', 29, 'Montreal'), ('Bo', 17, 'Toronto'), ('Ada', 36, null);",
    );
    let mut fake = open(path);
    fake.press("\n");
    fake
}

fn open(path: PathBuf) -> Fake<Browser> {
    let connection = Connection::sqlite("rows", path.to_str().unwrap());
    let database = database::connect(&connection, None).unwrap();
    Fake::sized(Browser::new(connection, database).unwrap(), 120, 30)
}

fn names(fake: &Fake<Browser>) -> Vec<String> {
    let column = fake
        .app
        .headers
        .iter()
        .position(|header| header == "name")
        .unwrap();
    fake.app
        .rows
        .iter()
        .map(|row| row[column].clone())
        .collect()
}

fn apply(fake: &mut Fake<Browser>, conditions: Vec<Filter>, joiner: Joiner) {
    fake.press("f").press(ESCAPE);
    let form = fake.app.filter_form.as_mut().unwrap();
    form.conditions = conditions;
    form.joiner = joiner;
    fake.press(SAVE);
}

fn form_cell(fake: &Fake<Browser>) -> Cell {
    fake.app.filter_form.as_ref().unwrap().cell
}

fn current(fake: &Fake<Browser>) -> Filter {
    fake.app.filter_form.as_ref().unwrap().current().clone()
}

#[test]
fn opens_ready_to_type_a_value_on_the_column_you_were_on() {
    let mut fake = people("ready");
    fake.press("l").press("f");

    assert_eq!(current(&fake), Filter::new("name", "contains", ""));
    assert_eq!(form_cell(&fake), Cell::Value);

    fake.press("Karl").press(SAVE);

    assert_eq!(names(&fake), ["Karl", "Karla"]);
    assert!(fake.app.filter_form.is_none());
}

#[test]
fn applies_with_a_second_enter() {
    let mut fake = people("enter-enter");
    fake.press("l").press("f").press("Bo").press("\n");

    assert_eq!(current(&fake).value, "Bo");
    assert_eq!(names(&fake).len(), 4);

    fake.press("\n");

    assert_eq!(names(&fake), ["Bo"]);
}

#[test]
fn combines_conditions_with_and() {
    let mut fake = people("and");
    apply(
        &mut fake,
        vec![
            Filter::new("city", "is", "Toronto"),
            Filter::new("age", "is at least", "18"),
        ],
        Joiner::And,
    );

    assert_eq!(names(&fake), ["Karl"]);
}

#[test]
fn combines_conditions_with_or() {
    let mut fake = people("or");
    fake.press("f").press(ESCAPE).press("o");
    fake.app.filter_form.as_mut().unwrap().conditions = vec![
        Filter::new("name", "is", "Bo"),
        Filter::new("name", "is", "Ada"),
    ];
    fake.press(SAVE);

    assert_eq!(names(&fake), ["Bo", "Ada"]);
}

#[test]
fn finds_empty_values() {
    let mut fake = people("empty");
    apply(
        &mut fake,
        vec![Filter::new("city", "is empty", "")],
        Joiner::And,
    );

    assert_eq!(names(&fake), ["Ada"]);
}

#[test]
fn shows_the_where_clause_with_its_values_in_the_sql_pane() {
    let mut fake = people("statement");
    apply(
        &mut fake,
        vec![
            Filter::new("name", "contains", "Karl"),
            Filter::new("age", "is one of", "41, 29"),
        ],
        Joiner::And,
    );

    assert_eq!(
        fake.app.last_statement.as_deref(),
        Some(
            "select * from \"people\" where \"name\" like '%Karl%' and \"age\" in (41, 29) order by \"id\" asc limit 100 offset 0"
        )
    );
}

#[test]
fn binds_values_so_quotes_are_text_rather_than_sql() {
    let mut fake = people("bound");
    apply(
        &mut fake,
        vec![Filter::new("name", "is", "x' or '1' = '1")],
        Joiner::And,
    );

    assert!(names(&fake).is_empty());
    assert!(fake.app.filters.is_some());

    fake.press(ESCAPE);
    apply(
        &mut fake,
        vec![Filter::new("name", "is not", "O'Brien")],
        Joiner::And,
    );

    assert_eq!(names(&fake).len(), 4);
}

#[test]
fn says_the_table_is_filtered_and_offers_to_go_back() {
    let mut fake = people("title");
    assert!(!fake.screen().contains("filtered"));
    assert!(!fake.screen().contains("esc Back"));

    apply(
        &mut fake,
        vec![Filter::new("name", "is", "Karl")],
        Joiner::And,
    );

    let screen = fake.screen();
    assert!(screen.contains("[2] people ·  filtered"), "{screen}");
    assert!(screen.contains("f Filter"), "{screen}");
    assert!(screen.contains("esc Back"), "{screen}");
}

#[test]
fn offers_the_form_keys_while_it_is_open() {
    let mut fake = people("offers");
    fake.press("f");

    let screen = fake.screen();
    assert!(
        screen.contains("↑↓ Move   ctrl+s Apply   esc Cancel"),
        "{screen}"
    );
    assert!(screen.contains("FILTER"), "{screen}");
    assert!(
        screen.contains("↵ keeps it    ↵↵ applies    esc goes back to the form"),
        "{screen}"
    );
}

#[test]
fn clears_the_filter_when_the_reopened_form_is_cancelled() {
    let mut fake = people("cancel");
    apply(
        &mut fake,
        vec![Filter::new("name", "is", "Karl")],
        Joiner::And,
    );
    assert_eq!(names(&fake), ["Karl"]);

    fake.press("f").press(ESCAPE);

    assert!(fake.app.filters.is_none());
    assert!(fake.app.filter_form.is_none());
    assert_eq!(names(&fake).len(), 4);
    assert_eq!(fake.app.status(), "filter cleared");
}

#[test]
fn reopens_with_the_conditions_you_had() {
    let mut fake = people("reopen");
    apply(
        &mut fake,
        vec![Filter::new("name", "is", "Karl")],
        Joiner::Or,
    );

    fake.press("f");

    let form = fake.app.filter_form.as_ref().unwrap();
    assert_eq!(form.conditions, [Filter::new("name", "is", "Karl")]);
    assert_eq!(form.joiner, Joiner::Or);
    assert!(form.editor.is_none());
}

#[test]
fn cycles_the_column_and_the_operator_with_the_arrows() {
    let mut fake = people("cycle");
    fake.press("f")
        .press(ESCAPE)
        .press(SHIFT_TAB)
        .press(SHIFT_TAB);

    assert_eq!(form_cell(&fake), Cell::Column);
    assert_eq!(current(&fake).column, "id");

    fake.press(RIGHT);
    assert_eq!(current(&fake).column, "name");

    fake.press("\t").press(RIGHT);
    assert_eq!(form_cell(&fake), Cell::Operator);
    assert_eq!(current(&fake).operator, "starts with");
}

#[test]
fn adds_and_removes_conditions_but_keeps_one() {
    let mut fake = people("add-remove");
    fake.press("f").press(ESCAPE).press("+");
    assert_eq!(fake.app.filter_form.as_ref().unwrap().conditions.len(), 2);

    fake.press("k");
    assert_eq!(fake.app.filter_form.as_ref().unwrap().row, 0);
    fake.press("j");
    assert_eq!(fake.app.filter_form.as_ref().unwrap().row, 1);

    fake.press("-").press("-").press("d");
    assert_eq!(fake.app.filter_form.as_ref().unwrap().conditions.len(), 1);
}

#[test]
fn forgets_the_filter_when_you_change_table() {
    let mut fake = people("switch");
    apply(
        &mut fake,
        vec![Filter::new("name", "is", "Karl")],
        Joiner::And,
    );

    fake.app.focus = Focus::Sidebar;
    fake.press("j");

    assert_eq!(fake.app.current_table().as_deref(), Some("widgets"));
    assert!(fake.app.filters.is_none());
    assert_eq!(fake.app.rows.len(), 1);
}

#[test]
fn picks_a_column_from_a_list_you_can_type_into() {
    let mut fake = people("column-picker");
    fake.press("f")
        .press(ESCAPE)
        .press(SHIFT_TAB)
        .press(SHIFT_TAB)
        .press("\n");

    let picker = fake.app.filter_form.as_ref().unwrap().picker.as_ref();
    assert_eq!(picker.unwrap().title, "COLUMN");
    assert_eq!(picker.unwrap().options, ["id", "name", "age", "city"]);
    assert!(fake.screen().contains("COLUMN"));

    fake.press("cit");
    let picker = fake.app.filter_form.as_ref().unwrap().picker.as_ref();
    assert_eq!(picker.unwrap().matches(), ["city"]);

    fake.press("\n");
    assert!(fake.app.filter_form.as_ref().unwrap().picker.is_none());
    assert_eq!(current(&fake).column, "city");
}

#[test]
fn picks_an_operator_and_drops_the_value_cell_when_it_needs_none() {
    let mut fake = people("operator-picker");
    fake.press("f").press(ESCAPE).press(SHIFT_TAB).press("\n");

    assert_eq!(
        fake.app
            .filter_form
            .as_ref()
            .unwrap()
            .picker
            .as_ref()
            .unwrap()
            .title,
        "OPERATOR"
    );

    fake.press("empty");
    let picker = fake.app.filter_form.as_ref().unwrap().picker.as_ref();
    assert_eq!(picker.unwrap().matches(), ["is empty", "is not empty"]);

    fake.press("\n").press("\t");
    assert_eq!(current(&fake).operator, "is empty");
    assert_eq!(form_cell(&fake), Cell::Operator);
}

#[test]
fn leaves_the_cell_alone_when_the_list_is_escaped() {
    let mut fake = people("picker-escape");
    fake.press("f")
        .press(ESCAPE)
        .press(SHIFT_TAB)
        .press(SHIFT_TAB)
        .press("\n")
        .press("city")
        .press(ESCAPE);

    assert!(fake.app.filter_form.as_ref().unwrap().picker.is_none());
    assert_eq!(current(&fake).column, "id");
}

#[test]
fn starts_the_list_again_from_the_top_when_typing_changes_it() {
    let mut fake = people("picker-top");
    fake.press("f")
        .press(ESCAPE)
        .press(SHIFT_TAB)
        .press(SHIFT_TAB)
        .press("\n")
        .press(DOWN)
        .press(DOWN)
        .press("a");

    let picker = fake.app.filter_form.as_ref().unwrap().picker.as_ref();
    assert_eq!(picker.unwrap().index, 0);
    assert_eq!(picker.unwrap().selected().as_deref(), Some("name"));
}

#[test]
fn says_when_nothing_in_the_list_matches() {
    let mut fake = people("picker-nothing");
    fake.press("f")
        .press(ESCAPE)
        .press(SHIFT_TAB)
        .press(SHIFT_TAB)
        .press("\n")
        .press("zzz");

    assert!(fake.screen().contains("nothing matches"));
}

#[test]
fn steps_back_into_the_form_on_escape_rather_than_closing() {
    let mut fake = people("step-back");
    fake.press("f").press("Kar").press(ESCAPE);

    let form = fake.app.filter_form.as_ref().unwrap();
    assert!(form.editor.is_none());
    assert_eq!(form.current().value, "");

    fake.press(ESCAPE);
    assert!(fake.app.filter_form.is_none());
}

#[test]
fn gets_back_to_the_column_after_typing_a_value() {
    let mut fake = people("back-to-column");
    fake.press("f").press("Karl").press("\n");
    assert_eq!(current(&fake).value, "Karl");

    fake.press(SHIFT_TAB).press(SHIFT_TAB).press(RIGHT);

    assert_eq!(form_cell(&fake), Cell::Column);
    assert_eq!(current(&fake), Filter::new("name", "contains", "Karl"));
}

#[test]
fn moves_with_h_and_l_once_out_of_the_value_but_types_other_letters() {
    let mut fake = people("form-keys");
    fake.press("f").press(ESCAPE).press("h");

    assert_eq!(form_cell(&fake), Cell::Operator);
    assert!(fake.app.filter_form.as_ref().unwrap().editor.is_none());

    fake.press("l");
    assert_eq!(current(&fake).operator, "starts with");

    fake.press("\t").press("K");
    let form = fake.app.filter_form.as_ref().unwrap();
    assert_eq!(
        form.editor.as_ref().map(|editor| editor.text()).as_deref(),
        Some("K")
    );
}

#[test]
fn clears_the_filter_with_escape_from_the_rows() {
    let mut fake = people("escape-rows");
    apply(
        &mut fake,
        vec![Filter::new("name", "contains", "Karl")],
        Joiner::And,
    );
    assert_eq!(names(&fake).len(), 2);

    fake.press(ESCAPE);

    assert!(fake.app.filters.is_none());
    assert_eq!(names(&fake).len(), 4);
    assert_eq!(fake.app.status(), "filter cleared");
}

#[test]
fn does_nothing_on_escape_when_nothing_is_filtered() {
    let mut fake = people("escape-nothing");
    fake.press(ESCAPE);

    assert!(fake.app.filters.is_none());
    assert_ne!(fake.app.status(), "filter cleared");
}

#[test]
fn applies_nothing_when_no_condition_is_finished() {
    let mut fake = people("unfinished");
    fake.press("f").press(SAVE);

    assert!(fake.app.filters.is_none());
    assert!(!fake.screen().contains("filtered"));
}

#[test]
fn asks_for_a_table_before_filtering() {
    let mut fake = open(fixture("no-tables", "pragma user_version = 1;"));
    fake.press("f");

    assert!(fake.app.filter_form.is_none());
    assert_eq!(fake.app.status(), "open a table first");
}

fn customers(name: &str) -> Fake<Browser> {
    let inserts: Vec<String> = (1..=250)
        .map(|id| format!("({id}, 'Customer {id:03}')"))
        .collect();
    let path = fixture(
        name,
        &format!(
            "create table customers (id integer primary key, name text);
             insert into customers values {};",
            inserts.join(", ")
        ),
    );
    let mut fake = open(path);
    fake.press("\n");
    fake
}

#[test]
fn keeps_the_filter_across_pages() {
    let mut fake = customers("pages");
    apply(
        &mut fake,
        vec![Filter::new("name", "contains", "1")],
        Joiner::And,
    );
    assert!(fake.app.has_more);

    fake.press("n");

    assert_eq!(fake.app.offset, 100);
    assert_eq!(fake.app.rows.len(), 33);
    assert!(names(&fake).iter().all(|name| name.contains('1')));
    assert!(fake.screen().contains("filtered"));
}

#[test]
fn keeps_the_filter_when_sorting_and_starts_again_from_the_first_page() {
    let mut fake = customers("sorting");
    apply(
        &mut fake,
        vec![Filter::new("name", "contains", "1")],
        Joiner::And,
    );
    fake.press("n").press("l").press("o").press("o");

    assert_eq!(fake.app.offset, 0);
    assert_eq!(
        fake.app.sort,
        Some(Sort {
            column: "name".to_string(),
            direction: database::Direction::Desc,
        })
    );
    assert_eq!(
        names(&fake).first().map(String::as_str),
        Some("Customer 241")
    );
    assert!(names(&fake).iter().all(|name| name.contains('1')));
}

#[test]
fn starts_again_from_the_first_page_when_a_filter_is_applied() {
    let mut fake = customers("first-page");
    fake.press("n");
    assert_eq!(fake.app.offset, 100);

    apply(
        &mut fake,
        vec![Filter::new("name", "ends with", "5")],
        Joiner::And,
    );

    assert_eq!(fake.app.offset, 0);
    assert_eq!(
        names(&fake).first().map(String::as_str),
        Some("Customer 005")
    );
    assert_eq!(fake.app.rows.len(), 25);
}

#[test]
fn filters_through_the_database_with_bound_values() {
    let path = fixture(
        "database",
        "create table people (id integer primary key, name text);
         insert into people (name) values ('O''Brien'), ('Karl');",
    );
    let connection = Connection::sqlite("rows", path.to_str().unwrap());
    let mut database = database::connect(&connection, None).unwrap();

    let page = database
        .filtered_rows(
            "people",
            10,
            0,
            None,
            Some(&Filters::new(
                vec![Filter::new("name", "is", "O'Brien")],
                Joiner::And,
            )),
        )
        .unwrap();

    assert_eq!(
        page.rows,
        [[Some("1".to_string()), Some("O'Brien".to_string())]]
    );
    assert_eq!(
        page.statement,
        "select * from \"people\" where \"name\" = 'O''Brien' limit 10 offset 0"
    );
}

#[test]
fn ignores_the_mouse_while_the_form_is_open() {
    let mut fake = people("mouse");
    fake.press("f").click(4, 3);

    assert!(fake.app.filter_form.is_some());
    assert_eq!(fake.app.current_table().as_deref(), Some("people"));
    assert_eq!(fake.app.focus, Focus::Grid);
}
