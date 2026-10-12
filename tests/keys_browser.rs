use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use app::driver::Driver;
use app::keystore::{Change, Contents, KeyKind, KeyStore, Reply, Scanned};
use app::store::Connection;
use app::terminal::{Fake, Tui};
use app::tui::KeyBrowser;

#[derive(Debug, Clone, PartialEq)]
enum Value {
    Text(Vec<u8>),
    Hash(Vec<(String, String)>),
    List(Vec<String>),
    Set(Vec<String>),
    Zset(Vec<(String, String)>),
}

#[derive(Default)]
struct World {
    databases: BTreeMap<u32, BTreeMap<String, Value>>,
    patterns: Vec<String>,
    written: Vec<Vec<Change>>,
    ran: Vec<String>,
    expired: Vec<(String, Option<i64>)>,
    renamed: Vec<(String, String)>,
    field_expiry: Vec<(String, String, Option<i64>)>,
    refuse_write: Option<String>,
}

#[derive(Clone)]
struct Fake2 {
    world: Arc<Mutex<World>>,
    database: u32,
}

impl Fake2 {
    fn with(keys: Vec<(&str, Value)>) -> Fake2 {
        let mut world = World::default();
        world.databases.insert(
            0,
            keys.into_iter()
                .map(|(key, value)| (key.to_string(), value))
                .collect(),
        );
        world.databases.insert(
            1,
            BTreeMap::from([("other".to_string(), Value::Text(b"x".to_vec()))]),
        );
        Fake2 {
            world: Arc::new(Mutex::new(world)),
            database: 0,
        }
    }

    fn keys(&self) -> Vec<String> {
        self.world.lock().unwrap().databases[&self.database]
            .keys()
            .cloned()
            .collect()
    }
}

fn glob(pattern: &str, key: &str) -> bool {
    let core = pattern.trim_matches('*');
    match (pattern.starts_with('*'), pattern.ends_with('*')) {
        _ if pattern.is_empty() || pattern == "*" => true,
        (true, true) => key.contains(core),
        (false, true) => key.starts_with(core),
        (true, false) => key.ends_with(core),
        (false, false) => key == pattern,
    }
}

impl KeyStore for Fake2 {
    fn scan(&mut self, cursor: u64, pattern: &str, count: usize) -> anyhow::Result<Scanned> {
        let mut world = self.world.lock().unwrap();
        if cursor == 0 {
            world.patterns.push(pattern.to_string());
        }
        let matching: Vec<String> = world.databases[&self.database]
            .keys()
            .filter(|key| glob(pattern, key))
            .cloned()
            .collect();
        let start = cursor as usize;
        let end = (start + count).min(matching.len());
        Ok(Scanned {
            cursor: if end >= matching.len() { 0 } else { end as u64 },
            keys: matching[start..end].to_vec(),
        })
    }

    fn size(&mut self) -> anyhow::Result<u64> {
        Ok(self.world.lock().unwrap().databases[&self.database].len() as u64)
    }

    fn kinds(&mut self, keys: &[String]) -> anyhow::Result<Vec<KeyKind>> {
        let world = self.world.lock().unwrap();
        Ok(keys
            .iter()
            .map(|key| match world.databases[&self.database].get(key) {
                Some(Value::Text(_)) => KeyKind::String,
                Some(Value::Hash(_)) => KeyKind::Hash,
                Some(Value::List(_)) => KeyKind::List,
                Some(Value::Set(_)) => KeyKind::Set,
                Some(Value::Zset(_)) => KeyKind::Zset,
                None => KeyKind::Other("none".into()),
            })
            .collect())
    }

    fn ttl(&mut self, _: &str) -> anyhow::Result<Option<i64>> {
        Ok(None)
    }

    fn read(
        &mut self,
        key: &str,
        _: &KeyKind,
        offset: usize,
        limit: usize,
    ) -> anyhow::Result<Contents> {
        let world = self.world.lock().unwrap();
        let rows = |headers: &[&str], rows: Vec<Vec<String>>| {
            let more = rows.len() > offset + limit;
            let rows: Vec<Vec<String>> = rows.into_iter().skip(offset).take(limit).collect();
            Contents::Rows {
                headers: headers.iter().map(|header| header.to_string()).collect(),
                binary: rows
                    .iter()
                    .map(|row| row.iter().any(|cell| cell.starts_with("0x")))
                    .collect(),
                rows,
                more,
            }
        };
        Ok(match world.databases[&self.database].get(key) {
            None => Contents::Missing,
            Some(Value::Text(bytes)) => Contents::Text(bytes.clone()),
            Some(Value::Hash(pairs)) => rows(
                &["field", "value"],
                pairs
                    .iter()
                    .map(|(field, value)| vec![field.clone(), value.clone()])
                    .collect(),
            ),
            Some(Value::List(items)) => rows(
                &["index", "value"],
                items
                    .iter()
                    .enumerate()
                    .map(|(at, item)| vec![at.to_string(), item.clone()])
                    .collect(),
            ),
            Some(Value::Set(members)) => rows(
                &["member"],
                members.iter().map(|member| vec![member.clone()]).collect(),
            ),
            Some(Value::Zset(pairs)) => rows(
                &["member", "score"],
                pairs
                    .iter()
                    .map(|(member, score)| vec![member.clone(), score.clone()])
                    .collect(),
            ),
        })
    }

    fn write(&mut self, changes: &[Change]) -> anyhow::Result<()> {
        let mut world = self.world.lock().unwrap();
        if let Some(refusal) = world.refuse_write.clone() {
            anyhow::bail!(refusal);
        }
        world.written.push(changes.to_vec());
        let database = self.database;
        let keys = world.databases.get_mut(&database).unwrap();
        for change in changes {
            match change {
                Change::SetString { key, value } => {
                    keys.insert(key.clone(), Value::Text(value.as_bytes().to_vec()));
                }
                Change::DeleteKey { key } => {
                    keys.remove(key);
                }
                Change::NewKey { key, first, .. } => {
                    keys.insert(key.clone(), Value::Text(first[0].as_bytes().to_vec()));
                }
                Change::SetField { key, field, value } => {
                    if let Some(Value::Hash(pairs)) = keys.get_mut(key) {
                        match pairs.iter_mut().find(|(name, _)| name == field) {
                            Some(pair) => pair.1 = value.clone(),
                            None => pairs.push((field.clone(), value.clone())),
                        }
                    }
                }
                _ => {}
            }
        }
        Ok(())
    }

    fn expire(&mut self, key: &str, seconds: Option<i64>) -> anyhow::Result<()> {
        self.world
            .lock()
            .unwrap()
            .expired
            .push((key.to_string(), seconds));
        Ok(())
    }

    fn rename(&mut self, from: &str, to: &str) -> anyhow::Result<()> {
        let mut world = self.world.lock().unwrap();
        world.renamed.push((from.to_string(), to.to_string()));
        let database = self.database;
        let keys = world.databases.get_mut(&database).unwrap();
        if let Some(value) = keys.remove(from) {
            keys.insert(to.to_string(), value);
        }
        Ok(())
    }

    fn databases(&mut self) -> anyhow::Result<Vec<(u32, u64)>> {
        let world = self.world.lock().unwrap();
        Ok(world
            .databases
            .iter()
            .map(|(number, keys)| (*number, keys.len() as u64))
            .collect())
    }

    fn select(&mut self, database: u32) -> anyhow::Result<()> {
        self.database = database;
        Ok(())
    }

    fn run(&mut self, line: &str) -> anyhow::Result<Reply> {
        self.world.lock().unwrap().ran.push(line.to_string());
        Ok(
            match line
                .split_whitespace()
                .next()
                .unwrap_or_default()
                .to_lowercase()
                .as_str()
            {
                "hgetall" => Reply::Map(vec![
                    (Reply::Text("name".into()), Reply::Text("Ada".into())),
                    (
                        Reply::Text("email".into()),
                        Reply::Text("ada@example.com".into()),
                    ),
                ]),
                "oops" => Reply::Error("ERR unknown command 'oops'".into()),
                _ => Reply::Text("OK".into()),
            },
        )
    }

    fn reads_only(&mut self, command: &str) -> anyhow::Result<bool> {
        Ok(matches!(
            command.to_lowercase().as_str(),
            "get" | "hgetall" | "keys" | "scan"
        ))
    }

    fn server(&mut self) -> String {
        "Redis 7.4.0".to_string()
    }

    fn fork(&mut self) -> anyhow::Result<Box<dyn KeyStore>> {
        Ok(Box::new(self.clone()))
    }

    fn supports(&mut self, command: &str) -> bool {
        matches!(command, "hexpire" | "httl" | "commandlog")
    }

    fn expire_field(&mut self, key: &str, field: &str, seconds: Option<i64>) -> anyhow::Result<()> {
        self.world
            .lock()
            .unwrap()
            .field_expiry
            .push((key.to_string(), field.to_string(), seconds));
        Ok(())
    }

    fn log(&mut self) -> anyhow::Result<Contents> {
        Ok(Contents::Rows {
            headers: ["when", "took", "command", "client"]
                .iter()
                .map(|header| header.to_string())
                .collect(),
            binary: vec![false],
            rows: vec![vec![
                "2026-10-12 09:14:00".into(),
                "1200µs".into(),
                "KEYS *".into(),
                "127.0.0.1:5000".into(),
            ]],
            more: false,
        })
    }
}

fn connection(read_only: bool) -> Connection {
    Connection {
        name: "cache".into(),
        driver: Driver::Redis,
        read_only,
        ..Connection::sqlite("cache", "")
    }
}

fn opened(store: &Fake2, read_only: bool) -> Fake<KeyBrowser> {
    let mut fake = Fake::sized(
        KeyBrowser::new(connection(read_only), Box::new(store.clone())).unwrap(),
        120,
        30,
    );
    settled(&mut fake);
    fake
}

fn settled(fake: &mut Fake<KeyBrowser>) {
    let started = Instant::now();
    while started.elapsed() < Duration::from_secs(5) {
        fake.app.idle();
        if !fake.app.scanning() {
            break;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    fake.app.idle();
}

fn small() -> Fake2 {
    Fake2::with(vec![
        ("config", Value::Text(b"{\"debug\":true}".to_vec())),
        ("blob", Value::Text(vec![0xff, 0x00, 0x10])),
        (
            "user:1",
            Value::Hash(vec![
                ("name".into(), "Ada".into()),
                ("email".into(), "ada@example.com".into()),
            ]),
        ),
        ("user:2", Value::Hash(vec![("name".into(), "Grace".into())])),
        (
            "user:sessions",
            Value::Zset(vec![
                ("alice".into(), "10".into()),
                ("bob".into(), "20".into()),
            ]),
        ),
        (
            "queue:emails",
            Value::List(vec!["a@x".into(), "b@x".into()]),
        ),
        ("tags", Value::Set(vec!["red".into(), "green".into()])),
    ])
}

#[test]
fn groups_keys_into_namespaces_open_when_there_are_few() {
    let mut fake = opened(&small(), false);
    let screen = fake.screen();
    assert!(screen.contains("▾ user"), "{screen}");
    assert!(screen.contains("(3)"), "{screen}");
    assert!(screen.contains("sessions"), "{screen}");
    assert!(screen.contains("config"), "{screen}");
    assert!(screen.contains("7 keys"), "{screen}");
}

#[test]
fn namespaces_start_folded_over_two_hundred_keys_and_fold_with_enter() {
    let mut keys: Vec<(String, Value)> = (0..250)
        .map(|n| (format!("cache:{n:03}"), Value::Text(b"x".to_vec())))
        .collect();
    keys.push(("solo".into(), Value::Text(b"y".to_vec())));
    let store = Fake2::with(
        keys.iter()
            .map(|(key, value)| (key.as_str(), value.clone()))
            .collect(),
    );
    let mut fake = opened(&store, false);
    let screen = fake.screen();
    assert!(screen.contains("▸ cache"), "{screen}");
    assert!(screen.contains("(250)"), "{screen}");
    assert!(!screen.contains("cache:000"), "{screen}");
    assert!(!screen.contains(" 000"), "{screen}");

    fake.press("\n");
    assert!(fake.screen().contains("▾ cache"), "{}", fake.screen());
}

#[test]
fn stops_scanning_at_a_hundred_thousand_keys_and_says_so() {
    let keys: Vec<(String, Value)> = (0..150_000)
        .map(|n| (format!("k:{n}"), Value::Text(Vec::new())))
        .collect();
    let store = Fake2::with(
        keys.iter()
            .map(|(key, value)| (key.as_str(), value.clone()))
            .collect(),
    );
    let mut fake = opened(&store, false);
    assert_eq!(fake.app.key_count(), 100_000);
    assert!(
        fake.screen().contains("first 100000 of 150000"),
        "{}",
        fake.screen()
    );
}

#[test]
fn shows_each_type_in_the_value_pane() {
    let mut fake = opened(&small(), false);

    fake.app.open_key("user:1");
    let screen = fake.screen();
    assert!(screen.contains("user:1 · hash · 2 fields"), "{screen}");
    assert!(screen.contains("ada@example.com"), "{screen}");

    fake.app.open_key("user:sessions");
    assert!(fake.screen().contains("zset"), "{}", fake.screen());
    assert!(fake.screen().contains("alice"), "{}", fake.screen());

    fake.app.open_key("config");
    assert!(
        fake.screen().contains("    \"debug\": true"),
        "{}",
        fake.screen()
    );

    fake.app.open_key("blob");
    assert!(fake.screen().contains("0xff0010"), "{}", fake.screen());
    assert!(fake.screen().contains("3 bytes"), "{}", fake.screen());
}

#[test]
fn says_a_key_has_gone_when_it_vanishes() {
    let store = small();
    let mut fake = opened(&store, false);
    store
        .world
        .lock()
        .unwrap()
        .databases
        .get_mut(&0)
        .unwrap()
        .remove("tags");
    fake.app.open_key("tags");
    assert!(fake.screen().contains("tags is gone"), "{}", fake.screen());
}

#[test]
fn a_pattern_narrows_the_scan_on_the_server() {
    let store = small();
    let mut fake = opened(&store, false);
    fake.press("/user\n");
    settled(&mut fake);
    assert_eq!(
        store.world.lock().unwrap().patterns.last().unwrap(),
        "*user*"
    );
    assert_eq!(fake.app.key_count(), 3);

    fake.press("/user:*\n");
    settled(&mut fake);
    assert_eq!(
        store.world.lock().unwrap().patterns.last().unwrap(),
        "user:*"
    );
}

#[test]
fn switches_database_with_b_and_rescans() {
    let mut fake = opened(&small(), false);
    fake.press("b");
    assert!(fake.screen().contains("db1"), "{}", fake.screen());
    fake.press("db1\n");
    settled(&mut fake);
    assert_eq!(fake.app.key_count(), 1);
    assert!(fake.screen().contains("other"), "{}", fake.screen());
}

#[test]
fn edits_wait_for_w_and_go_in_one_write() {
    let store = small();
    let mut fake = opened(&store, false);

    fake.app.open_key("config");
    fake.press("e");
    fake.press("\x7f".repeat(40).as_str());
    fake.press("{}\n");
    assert!(store.world.lock().unwrap().written.is_empty());

    fake.app.open_key("user:1");
    fake.press("N");
    fake.press("plan\n");
    fake.press("pro\n");

    fake.app.select_key("tags");
    fake.press("d");

    fake.press(":w\n");
    let written = store.world.lock().unwrap().written.clone();
    assert_eq!(written.len(), 1, "{written:?}");
    assert_eq!(
        written[0],
        [
            Change::SetString {
                key: "config".into(),
                value: "{}".into()
            },
            Change::SetField {
                key: "user:1".into(),
                field: "plan".into(),
                value: "pro".into()
            },
            Change::DeleteKey { key: "tags".into() },
        ]
    );
    assert!(!store.keys().contains(&"tags".to_string()));
}

#[test]
fn u_drops_the_pending_changes() {
    let store = small();
    let mut fake = opened(&store, false);
    fake.app.select_key("tags");
    fake.press("d");
    assert_eq!(fake.app.pending().len(), 1);
    fake.press("u");
    assert!(fake.app.pending().is_empty());
}

#[test]
fn a_failed_write_keeps_the_changes_and_says_why() {
    let store = small();
    store.world.lock().unwrap().refuse_write =
        Some("a list item moved before it was written, so nothing was written".into());
    let mut fake = opened(&store, false);
    fake.app.select_key("tags");
    fake.press("d");
    fake.press(":w\n");
    assert_eq!(fake.app.pending().len(), 1);
    assert!(fake.screen().contains("moved"), "{}", fake.screen());
}

#[test]
fn sets_a_ttl_and_renames() {
    let store = small();
    let mut fake = opened(&store, false);
    fake.app.select_key("config");
    fake.press("T");
    fake.press("60\n");
    fake.press("T");
    fake.press("\n");
    assert_eq!(
        store.world.lock().unwrap().expired,
        [
            ("config".to_string(), Some(60)),
            ("config".to_string(), None)
        ]
    );

    fake.press("R");
    fake.press("\x7f".repeat(10).as_str());
    fake.press("settings\n");
    settled(&mut fake);
    assert_eq!(
        store.world.lock().unwrap().renamed,
        [("config".to_string(), "settings".to_string())]
    );
}

#[test]
fn adds_a_new_key() {
    let store = small();
    let mut fake = opened(&store, false);
    fake.app.select_key("config");
    fake.press("N");
    fake.press("greeting\n");
    fake.press("string\n");
    fake.press("hello\n");
    fake.press(":w\n");
    let written = store.world.lock().unwrap().written.clone();
    assert_eq!(
        written[0],
        [Change::NewKey {
            key: "greeting".into(),
            kind: KeyKind::String,
            first: vec!["hello".into()]
        }]
    );
}

#[test]
fn a_read_only_connection_refuses_every_edit() {
    let store = small();
    let mut fake = opened(&store, true);
    fake.app.select_key("tags");
    for key in ["d", "N", "T", "R"] {
        fake.press(key);
        assert!(
            fake.screen().contains("read-only"),
            "after {key}: {}",
            fake.screen()
        );
        fake.press("\x1b");
    }
    fake.app.open_key("config");
    fake.press("e");
    assert!(fake.screen().contains("read-only"), "{}", fake.screen());
    assert!(fake.app.pending().is_empty());
}

#[test]
fn the_command_pane_runs_a_command_and_shows_the_reply() {
    let store = small();
    let mut fake = opened(&store, false);
    fake.press("s");
    fake.press("HGETALL user:1");
    fake.press("\x12");
    let screen = fake.screen();
    assert!(screen.contains("ada@example.com"), "{screen}");
    assert_eq!(store.world.lock().unwrap().ran, ["HGETALL user:1"]);

    fake.press("\x7f".repeat(20).as_str());
    fake.press("oops");
    fake.press("\x12");
    assert!(
        fake.screen().contains("unknown command"),
        "{}",
        fake.screen()
    );
}

#[test]
fn the_command_pane_refuses_writes_on_a_read_only_connection_and_blocking_commands() {
    let store = small();
    let mut fake = opened(&store, true);
    fake.press("s");
    fake.press("SET a b");
    fake.press("\x12");
    assert!(fake.screen().contains("only reads"), "{}", fake.screen());

    fake.press("\x7f".repeat(20).as_str());
    fake.press("BLPOP q 0");
    fake.press("\x12");
    assert!(fake.screen().contains("waits"), "{}", fake.screen());
    assert!(store.world.lock().unwrap().ran.is_empty());
}

#[test]
fn t_on_a_hash_field_sets_its_ttl() {
    let store = small();
    let mut fake = opened(&store, false);
    fake.app.open_key("user:1");
    fake.press("j");
    fake.press("T");
    fake.press("90\n");
    fake.press("T");
    fake.press("\x7f\n");
    assert_eq!(
        store.world.lock().unwrap().field_expiry,
        [
            ("user:1".to_string(), "email".to_string(), Some(90)),
            ("user:1".to_string(), "email".to_string(), None)
        ]
    );
}

#[test]
fn colon_log_shows_the_command_log() {
    let mut fake = opened(&small(), false);
    fake.press(":log\n");
    let screen = fake.screen();
    assert!(screen.contains("command log"), "{screen}");
    assert!(screen.contains("KEYS *"), "{screen}");
    assert!(screen.contains("1200µs"), "{screen}");
}

#[test]
fn a_binary_field_is_shown_but_not_edited_or_removed() {
    let store = Fake2::with(vec![(
        "bin",
        Value::Hash(vec![
            ("0xff00".into(), "v".into()),
            ("plain".into(), "w".into()),
        ]),
    )]);
    let mut fake = opened(&store, false);
    fake.app.open_key("bin");
    fake.press("e");
    assert!(fake.screen().contains("binary"), "{}", fake.screen());
    fake.press("d");
    assert!(fake.screen().contains("binary"), "{}", fake.screen());
    assert!(fake.app.pending().is_empty());

    fake.press("j");
    fake.press("d");
    assert_eq!(fake.app.pending().len(), 1);
}

#[test]
fn the_command_pane_refuses_commands_that_change_the_connection_itself() {
    let store = small();
    let mut fake = opened(&store, false);
    fake.press("s");
    fake.press("SELECT 3");
    fake.press("\x12");
    assert!(
        fake.screen().contains("b switches database"),
        "{}",
        fake.screen()
    );

    for typed in ["MULTI", "watch k", "RESET", "CLIENT REPLY OFF", "HELLO 3"] {
        fake.press("\x7f".repeat(30).as_str());
        fake.press(typed);
        fake.press("\x12");
        assert!(
            fake.screen().contains("tql does not run"),
            "{typed}: {}",
            fake.screen()
        );
    }
    assert!(store.world.lock().unwrap().ran.is_empty());
}

#[test]
fn the_command_pane_cursor_sits_right_after_what_was_typed() {
    let mut fake = opened(&small(), false);
    fake.press("s");
    fake.press("GET ab");
    let (x, y) = fake.cursor();
    let row = fake.screen_row(y);
    assert_eq!(row[x as usize - 1], "b", "{}", row.concat());
    assert_eq!(row[x as usize - 2], "a", "{}", row.concat());
}
