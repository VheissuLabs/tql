use std::fs;
use std::path::PathBuf;

use app::console::RunResult;
use app::paths;
use serde_json::{Value, json};

struct Scratch {
    root: PathBuf,
}

impl Scratch {
    fn new(label: &str) -> Self {
        let root = std::env::temp_dir().join(format!("tql-agent-{}-{label}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        paths::use_config_directory(root.join("tql"));
        Scratch { root }
    }

    fn shop(label: &str) -> Self {
        let scratch = Scratch::new(label);
        let shop = scratch.root.join("shop.sqlite");
        rusqlite::Connection::open(&shop)
            .unwrap()
            .execute_batch(
                "create table users (id integer primary key, name text not null collate nocase, email text, \
                 credit real default 0, initial text generated always as (substr(name, 1, 1)) virtual);
                 create table empty_t (a text);
                 insert into users (name, email, credit) values ('Ann', 'ann@x.test', 1.5), ('Bob', null, 2), ('Cy', 'cy@x.test', 0);",
            )
            .unwrap();
        tql(&["open", &shop.display().to_string(), "--name=shop"]);
        scratch
    }

    fn executions(&self) -> Vec<(String, String, bool)> {
        let store = rusqlite::Connection::open(paths::store()).unwrap();
        let mut statement = store
            .prepare("select statement, source, succeeded from query_executions order by id")
            .unwrap();
        statement
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap()
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn tql(arguments: &[&str]) -> RunResult {
    tql_with(arguments, "")
}

fn tql_with(arguments: &[&str], stdin: &str) -> RunResult {
    app::application().run_with(arguments.iter().copied(), stdin)
}

fn json_of(result: &RunResult) -> Value {
    assert_eq!(result.code, 0, "{result:?}");
    serde_json::from_str(&result.stdout).unwrap_or_else(|_| panic!("{result:?}"))
}

#[test]
fn lists_the_saved_connections_as_json_when_piped() {
    let _scratch = Scratch::shop("connections");

    let listed = json_of(&tql(&["connections"]));

    assert_eq!(listed[0]["name"], "shop");
    assert_eq!(listed[0]["driver"], "sqlite");
    assert!(listed[0]["target"].as_str().unwrap().starts_with("sqlite:"));
    assert_eq!(listed[0]["read_only"], false);
    assert_eq!(listed[0]["last_used_at"], Value::Null);
}

#[test]
fn lists_no_connections_as_an_empty_array() {
    let _scratch = Scratch::new("no-connections");

    let listed = tql(&["connections"]);

    assert_eq!(listed.stdout, "[]\n");
    assert_eq!(listed.code, 0);
}

#[test]
fn json_is_indented_by_four_spaces_and_leaves_slashes_alone() {
    let _scratch = Scratch::shop("pretty");

    let listed = tql(&["connections"]);

    assert!(
        listed
            .stdout
            .starts_with("[\n    {\n        \"name\": \"shop\"")
    );
    assert!(!listed.stdout.contains("\\/"));
}

#[test]
fn draws_tables_when_asked_for_them() {
    let _scratch = Scratch::shop("tables-drawn");

    let connections = tql(&["connections", "--table"]);
    assert!(
        connections.stdout.contains("| Name | Driver | Where"),
        "{connections:?}"
    );
    assert!(connections.stdout.contains("| never     |"));

    let tables = tql(&["tables", "shop", "--table"]);
    assert_eq!(tables.stdout, "empty_t\nusers\n");

    let described = tql(&["describe", "shop", "users", "--table"]);
    assert!(!described.stdout.contains('{'));
    assert!(
        described
            .stdout
            .contains("| id      | integer | yes  |         | primary |"),
        "{}",
        described.stdout
    );

    let queried = tql(&[
        "query",
        "shop",
        "select name, email from users",
        "--table",
        "--limit=2",
    ]);
    let (table, footer) = queried
        .stdout
        .split_at(queried.stdout.rfind("3 rows").unwrap());
    assert_eq!(
        table,
        "+------+------------+\n\
         | name | email      |\n\
         +------+------------+\n\
         | Ann  | ann@x.test |\n\
         | Bob  | NULL       |\n\
         +------+------------+\n"
    );
    assert!(
        regex::Regex::new(r"^3 rows · \d+ms · showing the first 2\n$")
            .unwrap()
            .is_match(footer),
        "{footer}"
    );

    let empty = tql(&["query", "shop", "select * from empty_t", "--table"]);
    assert!(
        regex::Regex::new(r"^0 rows · \d+ms\n$")
            .unwrap()
            .is_match(&empty.stdout),
        "{empty:?}"
    );
}

#[test]
fn json_wins_over_table() {
    let _scratch = Scratch::shop("json-wins");

    assert!(
        tql(&["tables", "shop", "--table", "--json"])
            .stdout
            .starts_with('{')
    );
}

#[test]
fn lists_the_tables_of_a_connection() {
    let _scratch = Scratch::shop("tables");

    assert_eq!(
        json_of(&tql(&["tables", "shop"])),
        json!({ "connection": "shop", "tables": ["empty_t", "users"] })
    );
}

#[test]
fn describes_a_table_with_its_primary_key_and_every_column_in_order() {
    let _scratch = Scratch::shop("describe");

    let described = json_of(&tql(&["describe", "shop", "users"]));

    assert_eq!(described["primary_key"], "id");
    let names: Vec<&str> = described["columns"]
        .as_array()
        .unwrap()
        .iter()
        .map(|column| column["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, ["id", "name", "email", "credit", "initial"]);
    let keys: Vec<&String> = described["columns"][0]
        .as_object()
        .unwrap()
        .keys()
        .collect();
    assert_eq!(
        keys,
        [
            "name",
            "type_name",
            "type",
            "collation",
            "nullable",
            "default",
            "auto_increment",
            "comment",
            "generation"
        ]
    );
    assert_eq!(described["columns"][0]["auto_increment"], true);
    assert_eq!(described["columns"][1]["collation"], "nocase");
    assert_eq!(described["columns"][3]["default"], "0");
    assert_eq!(
        described["columns"][4]["generation"],
        json!({ "type": "virtual", "expression": "substr(name, 1, 1)" })
    );
}

#[test]
fn describing_a_table_that_is_not_there_says_so() {
    let _scratch = Scratch::shop("describe-ghost");

    let described = tql(&["describe", "shop", "ghost"]);

    assert_eq!(described.code, 1);
    assert_eq!(
        described.stderr,
        "Table [ghost] has no columns, or does not exist.\n"
    );
    assert_eq!(described.stdout, "");
}

#[test]
fn runs_a_query_and_gives_the_values_their_types() {
    let _scratch = Scratch::shop("query");

    let ran = json_of(&tql(&[
        "query",
        "shop",
        "select id, name, email, credit from users",
    ]));

    assert_eq!(ran["connection"], "shop");
    assert_eq!(ran["rows"], 3);
    assert_eq!(ran["truncated"], false);
    assert!(ran["duration_ms"].is_u64());
    assert_eq!(
        ran["results"][0],
        json!({ "id": 1, "name": "Ann", "email": "ann@x.test", "credit": 1.5 })
    );
    assert_eq!(ran["results"][1]["email"], Value::Null);
}

#[test]
fn a_limit_keeps_the_count_and_says_it_cut_the_rows() {
    let _scratch = Scratch::shop("limit");

    let ran = json_of(&tql(&["query", "shop", "select * from users", "--limit=2"]));
    assert_eq!(ran["rows"], 3);
    assert_eq!(ran["results"].as_array().unwrap().len(), 2);
    assert_eq!(ran["truncated"], true);

    let none = json_of(&tql(&["query", "shop", "select * from users", "--limit=0"]));
    assert_eq!(none["results"].as_array().unwrap().len(), 1);
}

#[test]
fn the_last_of_two_columns_with_one_name_wins() {
    let _scratch = Scratch::shop("duplicate-columns");

    let ran = json_of(&tql(&[
        "query",
        "shop",
        "select 1 as a, 2 as b, 'three' as a",
    ]));

    assert_eq!(
        serde_json::to_string(&ran["results"][0]).unwrap(),
        r#"{"a":"three","b":2}"#
    );
}

#[test]
fn a_write_hidden_in_a_with_is_refused_by_the_database_itself() {
    let _scratch = Scratch::shop("hidden-write");

    let refused = tql(&["query", "shop", "with c as (select 1) delete from users"]);

    assert_eq!(refused.code, 1, "{refused:?}");
    assert!(refused.stderr.contains("readonly database"), "{refused:?}");
    assert_eq!(
        json_of(&tql(&["query", "shop", "select * from users"]))["rows"],
        3
    );
}

#[test]
fn refuses_a_write_and_leaves_the_data_alone() {
    let scratch = Scratch::shop("write");

    for statement in ["delete from users", "select 1; delete from users"] {
        let refused = tql(&["query", "shop", statement]);
        assert_eq!(refused.code, 1);
        assert!(refused.stderr.contains("read-only"), "{refused:?}");
    }

    assert_eq!(
        json_of(&tql(&["query", "shop", "select * from users"]))["rows"],
        3
    );
    assert_eq!(scratch.executions().len(), 1);
}

#[test]
fn an_unknown_connection_names_the_known_ones() {
    let _scratch = Scratch::shop("unknown");

    let refused = tql(&["tables", "nope"]);

    assert_eq!(refused.code, 1);
    assert_eq!(
        refused.stderr,
        "There is no tql connection named [nope]. Known connections: shop\n"
    );
}

#[test]
fn a_bad_query_gives_the_databases_own_words() {
    let _scratch = Scratch::shop("bad-query");

    let failed = tql(&["query", "shop", "select nope from users"]);

    assert_eq!(failed.code, 1);
    assert_eq!(failed.stderr, "no such column: nope\n");
}

#[test]
fn a_schema_that_cannot_be_read_says_why() {
    let _scratch = Scratch::shop("unreadable");

    let failed = tql(&["tables", "shop", "--database=/nowhere/nope.sqlite"]);

    assert_eq!(failed.code, 1);
    assert!(
        failed.stderr.starts_with("Could not read the schema: "),
        "{failed:?}"
    );
}

#[test]
fn another_database_on_the_same_server_is_read_with_database() {
    let scratch = Scratch::shop("other-database");
    let other = scratch.root.join("other.sqlite");
    rusqlite::Connection::open(&other)
        .unwrap()
        .execute_batch("create table elsewhere (a text);")
        .unwrap();

    let listed = json_of(&tql(&[
        "tables",
        "shop",
        &format!("--database={}", other.display()),
    ]));

    assert_eq!(listed["tables"], json!(["elsewhere"]));
}

#[test]
fn reads_the_statement_from_stdin() {
    let _scratch = Scratch::shop("stdin");

    let piped = json_of(&tql_with(
        &["query", "shop"],
        "  select count(*) as n from users \n",
    ));
    assert_eq!(piped["results"][0]["n"], 3);

    let dashed = json_of(&tql_with(&["query", "shop", "-"], "select 1 as one"));
    assert_eq!(dashed["results"][0]["one"], 1);

    let nothing = tql_with(&["query", "shop"], "   ");
    assert_eq!(nothing.code, 1);
    assert_eq!(
        nothing.stderr,
        "Give the statement as an argument, or pipe it in: echo \"select 1\" | tql query <connection>\n"
    );
}

#[test]
fn records_what_it_ran_as_from_the_command_line() {
    let scratch = Scratch::shop("history");

    tql(&["query", "shop", "  select 1  "]);
    tql(&["query", "shop", "select nope"]);

    assert_eq!(
        scratch.executions(),
        [
            ("select 1".to_string(), "cli".to_string(), true),
            ("select nope".to_string(), "cli".to_string(), false),
        ]
    );
}

#[test]
fn does_not_mark_the_connection_used() {
    let _scratch = Scratch::shop("untouched");

    tql(&["query", "shop", "select 1"]);
    tql(&["tables", "shop"]);

    assert_eq!(
        json_of(&tql(&["connections"]))[0]["last_used_at"],
        Value::Null
    );
}

#[test]
fn agent_commands_are_never_taken_for_a_file_to_open() {
    for command in ["connections", "tables", "describe", "query", "mcp:start"] {
        assert_eq!(
            app::argv::rewrite(vec!["tql".into(), command.into(), "x".into()]),
            ["tql", command, "x"]
        );
    }
}

#[test]
fn describes_a_view_as_well_as_a_table() {
    let scratch = Scratch::shop("describe-view");
    rusqlite::Connection::open(scratch.root.join("shop.sqlite"))
        .unwrap()
        .execute_batch("create view named as select id, name from users")
        .unwrap();

    let described = json_of(&tql(&["describe", "shop", "named", "--json"]));

    assert_eq!(described["table"], "named");
    assert_eq!(described["columns"].as_array().unwrap().len(), 2);
}
