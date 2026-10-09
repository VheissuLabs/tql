use std::collections::HashMap;

use app::database::{Column, Database, Filters, ForeignKey, Index, Page, Sort};
use app::store::Connection;
use app::terminal::Fake;
use app::tui::Browser;

struct Server {
    current: Option<String>,
    databases: Vec<(&'static str, Vec<&'static str>)>,
}

impl Database for Server {
    fn tables(&mut self) -> anyhow::Result<Vec<String>> {
        Ok(self
            .databases
            .iter()
            .find(|(name, _)| Some(*name) == self.current.as_deref())
            .map(|(_, tables)| tables.iter().map(|table| table.to_string()).collect())
            .unwrap_or_default())
    }

    fn filtered_rows(
        &mut self,
        table: &str,
        _: usize,
        _: usize,
        _: Option<&Sort>,
        _: Option<&Filters>,
    ) -> anyhow::Result<Page> {
        Ok(Page {
            headers: vec!["id".into(), "name".into()],
            rows: vec![vec![Some("1".into()), Some(format!("first {table}"))]],
            ..Page::default()
        })
    }

    fn primary_keys(&mut self) -> anyhow::Result<HashMap<String, String>> {
        Ok(self
            .tables()?
            .into_iter()
            .map(|table| (table, "id".to_string()))
            .collect())
    }

    fn databases(&mut self) -> anyhow::Result<Vec<String>> {
        Ok(self
            .databases
            .iter()
            .map(|(name, _)| name.to_string())
            .collect())
    }

    fn use_database(&mut self, name: &str) -> anyhow::Result<()> {
        self.current = Some(name.to_string());
        Ok(())
    }

    fn quote(&self, identifier: &str) -> String {
        format!("`{identifier}`")
    }

    fn select(&mut self, _: &str, _: &[String]) -> anyhow::Result<Page> {
        Ok(Page::default())
    }

    fn columns(&mut self, _: &str) -> anyhow::Result<Vec<Column>> {
        Ok(Vec::new())
    }

    fn indexes(&mut self, _: &str) -> anyhow::Result<Vec<Index>> {
        Ok(Vec::new())
    }

    fn foreign_keys(&mut self) -> anyhow::Result<HashMap<String, Vec<ForeignKey>>> {
        Ok(HashMap::new())
    }
}

fn server(database: Option<&str>) -> Fake<Browser> {
    let connection = Connection {
        name: "Cloud - quill".into(),
        driver: "mysql".into(),
        database: database.map(String::from),
        tag: Some("production".into()),
        ..Connection::default()
    };
    let server = Server {
        current: database.map(String::from),
        databases: vec![
            ("information_schema", vec!["COLUMNS", "TABLES"]),
            ("mysql", vec!["user"]),
            ("production", vec!["activity_log", "users"]),
            ("scratch", vec![]),
        ],
    };
    Fake::sized(Browser::new(connection, Box::new(server)).unwrap(), 100, 30)
}

#[test]
fn a_connection_without_a_database_asks_for_one() {
    let mut fake = server(None);

    let screen = fake.screen();
    assert!(screen.contains("DATABASE"), "{screen}");
    assert!(screen.contains("information_schema"));
    assert!(screen.contains("[1] TABLES"));
    assert!(screen.contains("nothing matches"));
    assert!(screen.contains("↵ Choose"));
    assert_eq!(fake.highlighted()[1], "information_schema");
}

#[test]
fn choosing_a_database_opens_its_tables() {
    let mut fake = server(None);

    fake.press("prod\n");
    assert!(fake.app.databases.is_none());
    assert_eq!(fake.app.active_database.as_deref(), Some("production"));
    assert_eq!(fake.app.tables, ["activity_log", "users"]);
    assert_eq!(fake.app.status(), "using production");

    let screen = fake.screen();
    assert!(screen.contains("[1] production"));
    assert!(screen.contains("[2] activity_log"));
    assert!(screen.contains("first activity_log"));
}

#[test]
fn an_empty_database_says_so() {
    let mut fake = server(None);

    fake.press("scr\n");
    assert_eq!(fake.app.status(), "scratch has no tables");
}

#[test]
fn b_switches_database_starting_from_the_one_in_use() {
    let mut fake = server(Some("production"));
    assert_eq!(fake.app.tables, ["activity_log", "users"]);

    fake.press("b");
    assert_eq!(
        fake.app
            .databases
            .as_ref()
            .and_then(|choice| choice.selected())
            .as_deref(),
        Some("production")
    );

    fake.press("\x1b[B\x1b[B\n");
    assert_eq!(
        fake.app.active_database.as_deref(),
        Some("information_schema")
    );
    assert_eq!(fake.app.tables, ["COLUMNS", "TABLES"]);
}

#[test]
fn escape_closes_the_picker_without_switching() {
    let mut fake = server(Some("production"));

    fake.press("b\x1b");
    assert!(fake.app.databases.is_none());
    assert_eq!(fake.app.active_database.as_deref(), Some("production"));
}

#[test]
fn a_new_name_offers_to_create_it() {
    let mut fake = server(None);

    fake.press("analytics");
    assert!(fake.screen().contains("+ create analytics"));

    fake.press("\n");
    assert_eq!(fake.app.problem_title, "THE DATABASE SAID NO");
    assert!(fake.app.databases.is_none());
}

#[test]
fn the_sidebar_offers_b_for_servers() {
    let mut fake = server(Some("production"));

    assert!(fake.screen().contains("b Database"));
}
