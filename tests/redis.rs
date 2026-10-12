use std::sync::atomic::{AtomicUsize, Ordering};

use app::driver::Driver;
use app::keystore::{self, Change, Contents, KeyKind, KeyStore, Reply};
use app::store::Connection;

fn servers() -> Vec<(&'static str, String)> {
    ["TQL_TEST_REDIS", "TQL_TEST_VALKEY"]
        .into_iter()
        .filter_map(|name| std::env::var(name).ok().map(|place| (name, place)))
        .collect()
}

fn store(place: &str, database: u32) -> Box<dyn KeyStore> {
    let (host, port) = place.rsplit_once(':').expect("host:port");
    keystore::connect(
        &Connection {
            name: "redis".into(),
            driver: Driver::Redis,
            host: Some(host.into()),
            port: Some(port.parse().unwrap()),
            database: Some(database.to_string()),
            ..Connection::sqlite("redis", "")
        },
        None,
    )
    .unwrap()
}

fn prefix() -> String {
    static MADE: AtomicUsize = AtomicUsize::new(0);
    format!(
        "tql{}x{}",
        std::process::id(),
        MADE.fetch_add(1, Ordering::Relaxed)
    )
}

fn seeded(place: &str) -> (Box<dyn KeyStore>, String) {
    let mut redis = store(place, 0);
    let p = prefix();
    for line in [
        format!("SET {p}:greeting hello"),
        format!("HSET {p}:user:1 name Ada email ada@example.com"),
        format!("RPUSH {p}:queue a b c d e"),
        format!("SADD {p}:tags red green"),
        format!("ZADD {p}:leaders 30 carol 10 alice 20 bob"),
        format!("XADD {p}:events 1-1 kind signup who ada"),
    ] {
        redis.run(&line).unwrap();
    }
    redis.run(&format!("SET {p}:bytes \"\\xff\\x00\"")).unwrap();
    (redis, p)
}

fn rows(contents: Contents) -> Vec<Vec<String>> {
    match contents {
        Contents::Rows { rows, .. } => rows,
        other => panic!("not rows: {other:?}"),
    }
}

fn cleanup(redis: &mut Box<dyn KeyStore>, p: &str) {
    let mut cursor = 0;
    loop {
        let scanned = redis.scan(cursor, &format!("{p}*"), 1000).unwrap();
        for key in scanned.keys {
            redis.run(&format!("DEL {key}")).unwrap();
        }
        cursor = scanned.cursor;
        if cursor == 0 {
            break;
        }
    }
}

#[test]
#[ignore = "needs TQL_TEST_REDIS and/or TQL_TEST_VALKEY as host:port"]
fn scans_and_reads_every_type() {
    for (name, place) in servers() {
        let (mut redis, p) = seeded(&place);

        let mut found = Vec::new();
        let mut cursor = 0;
        loop {
            let scanned = redis.scan(cursor, &format!("{p}:*"), 2).unwrap();
            found.extend(scanned.keys);
            cursor = scanned.cursor;
            if cursor == 0 {
                break;
            }
        }
        found.sort();
        assert_eq!(found.len(), 7, "{name}: {found:?}");

        let keys: Vec<String> = [
            "greeting", "user:1", "queue", "tags", "leaders", "events", "nope",
        ]
        .iter()
        .map(|key| format!("{p}:{key}"))
        .collect();
        assert_eq!(
            redis.kinds(&keys).unwrap(),
            [
                KeyKind::String,
                KeyKind::Hash,
                KeyKind::List,
                KeyKind::Set,
                KeyKind::Zset,
                KeyKind::Stream,
                KeyKind::Other("none".into())
            ],
            "{name}"
        );

        match redis.read(&keys[0], &KeyKind::String, 0, 100).unwrap() {
            Contents::Text(bytes) => assert_eq!(bytes, b"hello"),
            other => panic!("{name}: {other:?}"),
        }
        match redis
            .read(&format!("{p}:bytes"), &KeyKind::String, 0, 100)
            .unwrap()
        {
            Contents::Text(bytes) => assert_eq!(bytes, [0xff, 0x00]),
            other => panic!("{name}: {other:?}"),
        }
        let mut hash = rows(redis.read(&keys[1], &KeyKind::Hash, 0, 100).unwrap());
        hash.sort();
        assert_eq!(
            hash,
            [["email", "ada@example.com"], ["name", "Ada"]],
            "{name}"
        );
        match redis.read(&keys[2], &KeyKind::List, 1, 2).unwrap() {
            Contents::Rows { rows, more, .. } => {
                assert_eq!(rows, [["1", "b"], ["2", "c"]], "{name}");
                assert!(more);
            }
            other => panic!("{name}: {other:?}"),
        }
        let mut set = rows(redis.read(&keys[3], &KeyKind::Set, 0, 100).unwrap());
        set.sort();
        assert_eq!(set, [["green"], ["red"]], "{name}");
        assert_eq!(
            rows(redis.read(&keys[4], &KeyKind::Zset, 0, 100).unwrap()),
            [["alice", "10"], ["bob", "20"], ["carol", "30"]],
            "{name}"
        );
        assert_eq!(
            rows(redis.read(&keys[5], &KeyKind::Stream, 0, 100).unwrap()),
            [["1-1", r#"{"kind":"signup","who":"ada"}"#]],
            "{name}"
        );
        assert!(matches!(
            redis.read(&keys[6], &KeyKind::String, 0, 100).unwrap(),
            Contents::Missing
        ));

        cleanup(&mut redis, &p);
    }
}

#[test]
#[ignore = "needs TQL_TEST_REDIS and/or TQL_TEST_VALKEY as host:port"]
fn writes_every_change_together_or_none_of_them() {
    for (name, place) in servers() {
        let (mut redis, p) = seeded(&place);
        let key = |rest: &str| format!("{p}:{rest}");

        redis
            .write(&[
                Change::SetString {
                    key: key("greeting"),
                    value: "hi".into(),
                },
                Change::SetField {
                    key: key("user:1"),
                    field: "name".into(),
                    value: "Grace".into(),
                },
                Change::SetItem {
                    key: key("queue"),
                    index: 0,
                    was: "a".into(),
                    value: "A".into(),
                },
                Change::RemoveItem {
                    key: key("queue"),
                    index: 4,
                    was: "e".into(),
                },
                Change::ReplaceMember {
                    key: key("tags"),
                    was: "red".into(),
                    value: "blue".into(),
                },
                Change::SetScore {
                    key: key("leaders"),
                    member: "alice".into(),
                    score: "40".into(),
                },
                Change::NewKey {
                    key: key("fresh"),
                    kind: KeyKind::Hash,
                    first: vec!["f".into(), "v".into()],
                },
            ])
            .unwrap();
        assert_eq!(
            redis.run(&format!("GET {}", key("greeting"))).unwrap(),
            Reply::Text("hi".into()),
            "{name}"
        );
        assert_eq!(
            rows(redis.read(&key("queue"), &KeyKind::List, 0, 100).unwrap()),
            [["0", "A"], ["1", "b"], ["2", "c"], ["3", "d"]],
            "{name}"
        );
        assert_eq!(
            rows(redis.read(&key("leaders"), &KeyKind::Zset, 0, 100).unwrap())[2],
            ["alice", "40"],
            "{name}"
        );
        assert_eq!(
            redis.run(&format!("HGET {} f", key("fresh"))).unwrap(),
            Reply::Text("v".into())
        );

        let moved = redis
            .write(&[
                Change::SetString {
                    key: key("greeting"),
                    value: "changed".into(),
                },
                Change::RemoveItem {
                    key: key("queue"),
                    index: 0,
                    was: "not there".into(),
                },
            ])
            .unwrap_err();
        assert!(moved.to_string().contains("moved"), "{name}: {moved}");
        assert_eq!(
            redis.run(&format!("GET {}", key("greeting"))).unwrap(),
            Reply::Text("hi".into())
        );

        cleanup(&mut redis, &p);
    }
}

#[test]
#[ignore = "needs TQL_TEST_REDIS and/or TQL_TEST_VALKEY as host:port"]
fn expires_renames_switches_databases_and_runs_commands() {
    for (name, place) in servers() {
        let (mut redis, p) = seeded(&place);
        let key = |rest: &str| format!("{p}:{rest}");

        redis.expire(&key("greeting"), Some(100)).unwrap();
        assert!(
            redis
                .ttl(&key("greeting"))
                .unwrap()
                .is_some_and(|ttl| ttl > 90)
        );
        redis.expire(&key("greeting"), None).unwrap();
        assert_eq!(redis.ttl(&key("greeting")).unwrap(), None);

        redis.rename(&key("greeting"), &key("salute")).unwrap();
        assert!(
            redis.rename(&key("salute"), &key("queue")).is_err(),
            "{name}"
        );

        let databases = redis.databases().unwrap();
        assert!(databases.len() >= 2, "{name}: {databases:?}");
        assert!(databases[0].1 >= 7);

        match redis.run(&format!("HGETALL {}", key("user:1"))).unwrap() {
            Reply::Map(pairs) => assert_eq!(pairs.len(), 2, "{name}"),
            Reply::List(items) => assert_eq!(items.len(), 4, "{name}"),
            other => panic!("{name}: {other:?}"),
        }
        assert!(matches!(
            redis.run("NOSUCHCOMMAND").unwrap(),
            Reply::Error(_)
        ));
        assert!(redis.reads_only("get").unwrap());
        assert!(!redis.reads_only("set").unwrap());
        assert!(
            redis.server().starts_with("Redis") || redis.server().starts_with("Valkey"),
            "{}",
            redis.server()
        );

        cleanup(&mut redis, &p);
        let mut other = store(&place, 1);
        assert_eq!(other.scan(0, &format!("{p}*"), 100).unwrap().keys.len(), 0);
    }
}
