use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::atomic::{AtomicUsize, Ordering};

use app::database::{self, Database, Page, Sort};
use app::store::Connection;
use app::tui::Browser;
use zerorust_tui::Fake;

struct Counted {
    inner: Box<dyn Database>,
    queries: Rc<RefCell<Vec<String>>>,
}

impl Counted {
    fn log(&self, query: String) {
        self.queries.borrow_mut().push(query);
    }
}

impl Database for Counted {
    fn tables(&mut self) -> anyhow::Result<Vec<String>> {
        self.log("tables".into());
        self.inner.tables()
    }

    fn rows(
        &mut self,
        table: &str,
        limit: usize,
        offset: usize,
        sort: Option<&Sort>,
    ) -> anyhow::Result<Page> {
        self.log(format!("rows {table}"));
        self.inner.rows(table, limit, offset, sort)
    }

    fn primary_keys(&mut self) -> anyhow::Result<HashMap<String, String>> {
        self.log("primary keys".into());
        self.inner.primary_keys()
    }
}

fn counted() -> (Fake<Browser>, Rc<RefCell<Vec<String>>>) {
    static FIXTURES: AtomicUsize = AtomicUsize::new(0);
    let path = std::env::temp_dir().join(format!(
        "tql-rust-{}-trips-{}.sqlite",
        std::process::id(),
        FIXTURES.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = std::fs::remove_file(&path);
    let writer = rusqlite::Connection::open(&path).unwrap();
    writer
        .execute_batch(
            "create table a (id integer primary key); create table b (id integer primary key);
             create table c (id integer primary key); create table d (id integer primary key);
             create table e (code text, region text, primary key (code, region));",
        )
        .unwrap();

    let connection = Connection::sqlite("trips", path.to_str().unwrap());
    let queries = Rc::new(RefCell::new(Vec::new()));
    let database = Box::new(Counted {
        inner: database::connect(&connection, None).unwrap(),
        queries: queries.clone(),
    });
    (
        Fake::new(Browser::new(connection, database).unwrap()),
        queries,
    )
}

fn take(queries: &Rc<RefCell<Vec<String>>>) -> Vec<String> {
    std::mem::take(&mut *queries.borrow_mut())
}

#[test]
fn opening_costs_three_queries_like_tqls_prefetch() {
    let (_fake, queries) = counted();

    assert_eq!(take(&queries), ["tables", "primary keys", "rows a"]);
}

#[test]
fn switching_tables_costs_one_query() {
    let (mut fake, queries) = counted();
    take(&queries);

    fake.press("j");
    assert_eq!(take(&queries), ["rows b"]);
    assert_eq!(
        fake.app.sort.as_ref().map(|sort| sort.column.as_str()),
        Some("id")
    );
}

#[test]
fn holding_j_loads_only_where_you_stop() {
    let (mut fake, queries) = counted();
    take(&queries);

    fake.burst("jjjj");
    assert_eq!(take(&queries), ["rows e"]);
    assert_eq!(fake.app.current_table().as_deref(), Some("e"));
}

#[test]
fn composite_keys_are_not_a_default_sort_like_in_tql() {
    let (mut fake, _) = counted();

    fake.press("jjjj");
    assert!(fake.app.sort.is_none());
}

#[test]
fn the_grid_says_loading_until_the_rows_arrive() {
    let (mut fake, _) = counted();

    fake.key(zerorust_tui::Key::Char('j'));
    assert!(fake.screen().contains("loading…"));
    assert!(fake.screen().contains("[2] b"));

    fake.settle();
    assert!(!fake.screen().contains("loading…"));
}
