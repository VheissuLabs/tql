use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use app::database::{Column, Database, Filters, ForeignKey, Index, Outcome, Page, Sort, Table};
use app::driver::Driver;
use app::paths;
use app::store::Connection;
use app::store_writer::StoreWriter;
use app::terminal::Fake;
use app::tui::Browser;

#[derive(Clone, Default)]
struct Server {
    current: Arc<Mutex<String>>,
    links: HashMap<Table, Vec<ForeignKey>>,
}

struct Schemas {
    server: Server,
}

fn tables_of(database: &str) -> Vec<Table> {
    match database {
        "two" => vec![Table::bare("a"), Table::bare("b")],
        _ => vec![
            Table::bare("customers"),
            Table::bare("orders"),
            Table::in_schema("cp", "authorization"),
            Table::in_schema("ev", "evaluation"),
            Table::in_schema("ev", "orders"),
        ],
    }
}

impl Database for Schemas {
    fn tables(&mut self) -> anyhow::Result<Vec<Table>> {
        Ok(tables_of(&self.server.current.lock().unwrap()))
    }

    fn default_schema(&mut self) -> Option<String> {
        Some("public".to_string())
    }

    fn filtered_rows(
        &mut self,
        table: &Table,
        _: usize,
        _: usize,
        _: Option<&Sort>,
        _: Option<&Filters>,
    ) -> anyhow::Result<Page> {
        if *table == Table::in_schema("ev", "evaluation") {
            return Ok(Page {
                headers: vec!["id".to_string(), "user_id".to_string()],
                rows: vec![vec![Some("7".to_string()), Some("1".to_string())]],
                ..Page::default()
            });
        }
        Ok(Page {
            headers: vec!["id".to_string(), "shown".to_string()],
            rows: vec![vec![Some("1".to_string()), Some(table.to_string())]],
            ..Page::default()
        })
    }

    fn primary_keys(&mut self) -> anyhow::Result<HashMap<Table, String>> {
        Ok(HashMap::new())
    }

    fn select(&mut self, statement: &str, _: &[String]) -> anyhow::Result<Page> {
        if statement.contains("from \"ev\".\"evaluation\"") && statement.ends_with("offset 0") {
            return Ok(Page {
                headers: vec!["id".to_string()],
                rows: vec![vec![Some("7".to_string())]],
                ..Page::default()
            });
        }
        Ok(Page::default())
    }

    fn columns(&mut self, _: &Table) -> anyhow::Result<Vec<Column>> {
        Ok(Vec::new())
    }

    fn indexes(&mut self, _: &Table) -> anyhow::Result<Vec<Index>> {
        Ok(Vec::new())
    }

    fn foreign_keys(&mut self) -> anyhow::Result<HashMap<Table, Vec<ForeignKey>>> {
        Ok(self.server.links.clone())
    }

    fn run(&mut self, _: &str) -> anyhow::Result<Outcome> {
        Ok(Outcome::default())
    }

    fn databases(&mut self) -> anyhow::Result<Vec<String>> {
        Ok(vec!["one".to_string(), "two".to_string()])
    }

    fn use_database(&mut self, name: &str) -> anyhow::Result<()> {
        *self.server.current.lock().unwrap() = name.to_string();
        Ok(())
    }
}

fn connection() -> Connection {
    Connection {
        driver: Driver::Pgsql,
        database: Some("one".to_string()),
        ..Connection::sqlite("schemas", "")
    }
}

fn browser_on(server: Server) -> Fake<Browser> {
    *server.current.lock().unwrap() = "one".to_string();
    Fake::sized(
        Browser::new(connection(), Box::new(Schemas { server })).unwrap(),
        100,
        24,
    )
}

fn browser() -> Fake<Browser> {
    browser_on(Server::default())
}

#[test]
fn one_schema_has_no_headers() {
    let server = Server::default();
    *server.current.lock().unwrap() = "two".to_string();
    let mut fake = Fake::sized(
        Browser::new(
            Connection {
                database: Some("two".to_string()),
                ..connection()
            },
            Box::new(Schemas { server }),
        )
        .unwrap(),
        100,
        24,
    );
    let screen = fake.screen();
    assert!(!screen.contains('▾'), "{screen}");
    assert!(screen.contains(" a "), "{screen}");
}

#[test]
fn every_schema_is_a_header_with_the_default_open_and_the_others_folded() {
    let mut fake = browser();
    let screen = fake.screen();
    assert!(screen.contains("▾ public"), "{screen}");
    assert!(screen.contains("▸ cp"), "{screen}");
    assert!(screen.contains("▸ ev"), "{screen}");
    assert!(screen.contains("customers"), "{screen}");
    assert!(!screen.contains("evaluation"), "{screen}");
    assert!(screen.find("▸ cp").unwrap() < screen.find("▸ ev").unwrap());
}

#[test]
fn starts_on_the_first_table_not_its_header() {
    let fake = browser();
    assert_eq!(fake.app.current_table(), Some(Table::bare("customers")));
}

#[test]
fn enter_on_a_header_unfolds_it_and_its_tables_open_in_their_schema() {
    let mut fake = browser();
    fake.press("jjj\n");
    assert!(fake.screen().contains("▾ ev"), "{}", fake.screen());
    assert!(fake.screen().contains("evaluation"), "{}", fake.screen());

    fake.press("jj");
    assert_eq!(
        fake.app.current_table(),
        Some(Table::in_schema("ev", "orders"))
    );
    fake.settle();
    assert!(fake.screen().contains("ev.orders"), "{}", fake.screen());
}

#[test]
fn h_folds_the_schema_you_are_in_and_l_unfolds_it() {
    let mut fake = browser();
    fake.press("h");
    assert!(fake.screen().contains("▸ public"), "{}", fake.screen());
    assert!(fake.app.is_folded("public"));
    fake.press("l");
    assert!(fake.screen().contains("▾ public"), "{}", fake.screen());
}

#[test]
fn filtering_opens_the_schemas_that_match_and_hides_the_rest() {
    let mut fake = browser();
    fake.press("/eval\n");
    let screen = fake.screen();
    assert!(screen.contains("▾ ev"), "{screen}");
    assert!(screen.contains("evaluation"), "{screen}");
    assert!(!screen.contains("cp"), "{screen}");
    assert!(!screen.contains("customers"), "{screen}");
}

#[test]
fn folding_is_remembered_per_connection_and_database() {
    let root = std::env::temp_dir().join(format!("tql-schemas-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("tql")).unwrap();
    paths::use_config_directory(root.join("tql"));
    StoreWriter::prepare(&paths::store()).unwrap();

    let opened = |server: Server| {
        *server.current.lock().unwrap() = "one".to_string();
        Fake::sized(
            Browser::new(connection(), Box::new(Schemas { server }))
                .unwrap()
                .with_store(paths::store()),
            100,
            24,
        )
    };
    let mut first = opened(Server::default());
    first.press("jjj\n");
    assert!(first.screen().contains("▾ ev"), "{}", first.screen());

    let mut second = opened(Server::default());
    assert!(second.screen().contains("▾ ev"), "{}", second.screen());
    assert!(second.screen().contains("▸ cp"), "{}", second.screen());

    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn switching_database_reads_its_own_schemas_and_folds() {
    let mut fake = browser();
    fake.press("jjj\n");
    assert!(fake.screen().contains("▾ ev"), "{}", fake.screen());

    fake.press("b");
    fake.press("two\n");
    fake.settle();
    let screen = fake.screen();
    assert!(!screen.contains('▾'), "{screen}");
    assert!(screen.contains(" a "), "{screen}");

    fake.press("b");
    fake.press("one\n");
    fake.settle();
    let screen = fake.screen();
    assert!(screen.contains("▾ public"), "{screen}");
    assert!(screen.contains("▸ ev"), "{screen}");
}

#[test]
fn the_palette_finds_a_table_in_another_schema_and_opens_it() {
    let mut fake = browser();
    fake.press("\x0bev.eval\n");
    assert_eq!(
        fake.app.current_table(),
        Some(Table::in_schema("ev", "evaluation"))
    );
    assert!(fake.screen().contains("▾ ev"), "{}", fake.screen());
}

#[test]
fn the_palette_finds_it_by_its_own_name_too() {
    let mut fake = browser();
    fake.press("\x0bauthorization\n");
    assert_eq!(
        fake.app.current_table(),
        Some(Table::in_schema("cp", "authorization"))
    );
}

#[test]
fn following_a_link_opens_the_table_in_the_schema_it_points_to() {
    let server = Server {
        links: HashMap::from([(
            Table::in_schema("ev", "evaluation"),
            vec![ForeignKey {
                columns: vec!["user_id".to_string()],
                table: Table::bare("customers"),
                references: vec!["id".to_string()],
            }],
        )]),
        ..Server::default()
    };
    let mut fake = browser_on(server);
    fake.press("\x0bev.eval\n");
    fake.settle();
    fake.app.focus = app::tui::Focus::Grid;
    fake.press("l");
    fake.press("L");
    fake.settle();
    assert_eq!(fake.app.current_table(), Some(Table::bare("customers")));
}

#[test]
fn an_export_names_and_quotes_a_table_in_another_schema() {
    let root = std::env::temp_dir().join(format!("tql-schemas-export-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("tql")).unwrap();
    paths::use_config_directory(root.join("tql"));

    let exporter = app::database::Exporter::new(&connection(), None);
    let named = exporter
        .filename(Some(&Table::in_schema("ev", "evaluation")))
        .unwrap();
    let file_name = named.file_name().unwrap().to_string_lossy().into_owned();
    assert!(file_name.contains("-ev-evaluation-"), "{file_name}");

    let mut database: Box<dyn Database> = Box::new(Schemas {
        server: Server::default(),
    });
    let path = root.join("both.sql");
    exporter
        .tables(
            database.as_mut(),
            &[Table::bare("orders"), Table::in_schema("ev", "evaluation")],
            Some(path.clone()),
            None,
        )
        .unwrap();
    let written = std::fs::read_to_string(&path).unwrap();
    assert!(written.contains("-- orders\n"), "{written}");
    assert!(written.contains("-- ev.evaluation\n"), "{written}");
    assert!(
        written.contains("insert into \"ev\".\"evaluation\""),
        "{written}"
    );

    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn the_palette_hints_a_tables_schema() {
    let mut fake = browser();
    fake.press("\x0beval");
    let screen = fake.screen();
    let line = screen
        .lines()
        .find(|line| line.contains("ev.evaluation"))
        .unwrap_or_else(|| panic!("{screen}"));
    let hint = line
        .trim_end_matches(['│', ' '])
        .rsplit(' ')
        .next()
        .unwrap();
    assert_eq!(hint, "ev", "{line}");
}
