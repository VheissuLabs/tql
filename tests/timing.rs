use std::time::Instant;

use app::database;
use app::store::Connection;
use app::terminal::Fake;
use app::tui::Browser;

#[test]
#[ignore]
fn timing() {
    let path = std::env::temp_dir().join("tql-rust-timing.sqlite");
    if !path.exists() {
        let connection = rusqlite::Connection::open(&path).unwrap();
        connection.execute_batch("create table a (id integer primary key, x text); create table big (id integer primary key, name text, email text, notes text, score real);").unwrap();
        connection.execute_batch("begin; with recursive n(i) as (select 1 union all select i+1 from n where i < 300000) insert into big select i, 'Customer ' || i, 'c' || i || '@example.com', hex(randomblob(40)), random() % 1000 from n; commit;").unwrap();
    }
    let connection = Connection::sqlite("bench", path.to_str().unwrap());

    let started = Instant::now();
    let database = database::connect(&connection, None).unwrap();
    let mut fake = Fake::sized(Browser::new(connection, database).unwrap(), 200, 50);
    println!("open            {:>8.1?}", started.elapsed());

    for (label, keys) in [
        ("render", ""),
        ("switch table", "j"),
        ("move row", "\tj"),
        ("move column", "l"),
        ("next page", "n"),
        ("sort by name", "o"),
        ("sort desc", "o"),
        ("back to key", "o"),
    ] {
        let started = Instant::now();
        fake.press(keys);
        fake.screen();
        println!("{label:<15} {:>8.1?}", started.elapsed());
    }

    let started = Instant::now();
    for _ in 0..50 {
        fake.press("j");
        fake.screen();
    }
    println!("50 × j + render {:>8.1?}", started.elapsed());
}
