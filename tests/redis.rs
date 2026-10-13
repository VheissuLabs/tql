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
        let mut hash: Vec<Vec<String>> =
            rows(redis.read(&keys[1], &KeyKind::Hash, 0, 100).unwrap())
                .into_iter()
                .map(|row| row[..2].to_vec())
                .collect();
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
        assert!(redis.reads_only("get", None).unwrap());
        assert!(!redis.reads_only("set", None).unwrap());
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

fn newer_servers() -> Vec<(&'static str, String)> {
    ["TQL_TEST_REDIS", "TQL_TEST_VALKEY", "TQL_TEST_VALKEY9"]
        .into_iter()
        .filter_map(|name| std::env::var(name).ok().map(|place| (name, place)))
        .collect()
}

#[test]
#[ignore = "needs TQL_TEST_REDIS, TQL_TEST_VALKEY or TQL_TEST_VALKEY9 as host:port"]
fn shows_and_sets_the_ttl_of_a_hash_field_where_the_server_has_them() {
    for (name, place) in newer_servers() {
        let (mut redis, p) = seeded(&place);
        let key = format!("{p}:user:1");
        if !redis.supports("httl") {
            assert!(
                matches!(
                    redis.read(&key, &KeyKind::Hash, 0, 100).unwrap(),
                    Contents::Rows { ref headers, .. } if headers.len() == 2
                ),
                "{name}"
            );
            cleanup(&mut redis, &p);
            continue;
        }
        redis.expire_field(&key, "email", Some(300)).unwrap();
        let Contents::Rows { headers, rows, .. } =
            redis.read(&key, &KeyKind::Hash, 0, 100).unwrap()
        else {
            panic!("{name}: not rows");
        };
        assert_eq!(headers, ["field", "value", "ttl"], "{name}");
        let email = rows.iter().find(|row| row[0] == "email").unwrap();
        assert!(
            email[2].trim_end_matches('s').parse::<i64>().unwrap() > 290,
            "{name}: {email:?}"
        );
        let other = rows.iter().find(|row| row[0] == "name").unwrap();
        assert_eq!(other[2], "none", "{name}");

        redis.expire_field(&key, "email", None).unwrap();
        let rows = rows_of(redis.read(&key, &KeyKind::Hash, 0, 100).unwrap());
        assert!(rows.iter().all(|row| row[2] == "none"), "{name}: {rows:?}");

        cleanup(&mut redis, &p);
    }
}

#[test]
#[ignore = "needs TQL_TEST_REDIS, TQL_TEST_VALKEY or TQL_TEST_VALKEY9 as host:port"]
fn reads_the_command_log_or_the_slow_log() {
    for (name, place) in newer_servers() {
        let mut redis = store(&place, 0);
        redis.run("CONFIG SET slowlog-log-slower-than 0").unwrap();
        if redis.supports("commandlog") {
            redis
                .run("CONFIG SET commandlog-execution-slower-than 0")
                .unwrap();
        }
        redis.run("PING").unwrap();
        let Contents::Rows { headers, rows, .. } = redis.log().unwrap() else {
            panic!("{name}: not rows");
        };
        assert_eq!(headers, ["when", "took", "command", "client"], "{name}");
        assert!(!rows.is_empty(), "{name}");
        redis
            .run("CONFIG SET slowlog-log-slower-than 10000")
            .unwrap();
        if redis.supports("commandlog") {
            redis
                .run("CONFIG SET commandlog-execution-slower-than 10000")
                .unwrap();
        }
    }
}

fn rows_of(contents: Contents) -> Vec<Vec<String>> {
    rows(contents)
}

#[test]
#[ignore = "needs TQL_TEST_REDIS and/or TQL_TEST_VALKEY as host:port"]
fn removes_and_changes_several_items_of_one_list_by_their_first_places() {
    for (name, place) in servers() {
        let (mut redis, p) = seeded(&place);
        let queue = format!("{p}:queue");

        redis
            .write(&[
                Change::RemoveItem {
                    key: queue.clone(),
                    index: 1,
                    was: "b".into(),
                },
                Change::SetItem {
                    key: queue.clone(),
                    index: 4,
                    was: "e".into(),
                    value: "E".into(),
                },
                Change::RemoveItem {
                    key: queue.clone(),
                    index: 3,
                    was: "d".into(),
                },
            ])
            .unwrap();

        assert_eq!(
            rows(redis.read(&queue, &KeyKind::List, 0, 100).unwrap()),
            [["0", "a"], ["1", "c"], ["2", "E"]],
            "{name}"
        );
        cleanup(&mut redis, &p);
    }
}

#[test]
#[ignore = "needs TQL_TEST_REDIS and/or TQL_TEST_VALKEY as host:port"]
fn editing_a_string_keeps_its_ttl() {
    for (name, place) in servers() {
        let (mut redis, p) = seeded(&place);
        let key = format!("{p}:greeting");
        redis.expire(&key, Some(500)).unwrap();

        redis
            .write(&[Change::SetString {
                key: key.clone(),
                value: "changed".into(),
            }])
            .unwrap();

        assert!(
            redis.ttl(&key).unwrap().is_some_and(|ttl| ttl > 400),
            "{name}"
        );
        cleanup(&mut redis, &p);
    }
}

fn list(redis: &mut Box<dyn KeyStore>, key: &str) -> Vec<String> {
    rows(redis.read(key, &KeyKind::List, 0, 100).unwrap())
        .into_iter()
        .map(|row| row[1].clone())
        .collect()
}

#[test]
#[ignore = "needs TQL_TEST_REDIS and/or TQL_TEST_VALKEY as host:port"]
fn writes_nothing_when_a_key_is_not_the_type_the_changes_expect() {
    for (name, place) in servers() {
        let (mut redis, p) = seeded(&place);
        let key = |rest: &str| format!("{p}:{rest}");

        for _ in 0..2 {
            let refused = redis
                .write(&[
                    Change::PushItem {
                        key: key("queue"),
                        value: "f".into(),
                    },
                    Change::SetField {
                        key: key("greeting"),
                        field: "f".into(),
                        value: "v".into(),
                    },
                ])
                .unwrap_err();
            assert!(
                refused.to_string().contains("is a string now"),
                "{name}: {refused}"
            );
        }
        assert_eq!(
            list(&mut redis, &key("queue")),
            ["a", "b", "c", "d", "e"],
            "{name}"
        );

        let refused = redis
            .write(&[Change::SetItem {
                key: key("greeting"),
                index: 0,
                was: "h".into(),
                value: "x".into(),
            }])
            .unwrap_err();
        assert!(
            refused.to_string().contains("nothing was written"),
            "{name}: {refused}"
        );
        redis
            .write(&[Change::SetString {
                key: key("greeting"),
                value: "after".into(),
            }])
            .unwrap();
        assert_eq!(
            redis
                .read(&key("greeting"), &KeyKind::String, 0, 1)
                .unwrap(),
            Contents::Text(b"after".to_vec()),
            "{name}: a refused list edit left a WATCH behind"
        );

        cleanup(&mut redis, &p);
    }
}

#[test]
#[ignore = "needs TQL_TEST_REDIS and/or TQL_TEST_VALKEY as host:port"]
fn a_new_key_never_overwrites_one_that_exists() {
    for (name, place) in servers() {
        let (mut redis, p) = seeded(&place);
        let key = format!("{p}:greeting");
        redis.run(&format!("EXPIRE {key} 5000")).unwrap();

        let refused = redis
            .write(&[Change::NewKey {
                key: key.clone(),
                kind: KeyKind::String,
                first: vec!["oops".into()],
            }])
            .unwrap_err();
        assert!(
            refused.to_string().contains("already exists"),
            "{name}: {refused}"
        );
        assert_eq!(
            redis.read(&key, &KeyKind::String, 0, 1).unwrap(),
            Contents::Text(b"hello".to_vec()),
            "{name}"
        );
        assert!(redis.ttl(&key).unwrap().is_some(), "{name}");

        cleanup(&mut redis, &p);
    }
}

#[test]
#[ignore = "needs TQL_TEST_REDIS and/or TQL_TEST_VALKEY as host:port"]
fn keeps_binary_key_names_apart_and_reaches_each() {
    for (name, place) in servers() {
        let mut redis = store(&place, 0);
        let p = prefix();
        redis.run(&format!("SET \"{p}:\\xff\\xfe\" one")).unwrap();
        redis.run(&format!("SET \"{p}:\\xfe\\xff\" two")).unwrap();

        let mut keys = redis.scan(0, &format!("{p}*"), 1000).unwrap().keys;
        keys.sort();
        assert_eq!(
            keys,
            [format!("{p}:\\xfe\\xff"), format!("{p}:\\xff\\xfe")],
            "{name}"
        );
        assert_eq!(
            redis.kinds(&keys).unwrap(),
            [KeyKind::String, KeyKind::String],
            "{name}"
        );
        assert_eq!(
            redis.read(&keys[1], &KeyKind::String, 0, 1).unwrap(),
            Contents::Text(b"one".to_vec()),
            "{name}"
        );
        redis
            .write(&[Change::DeleteKey {
                key: keys[1].clone(),
            }])
            .unwrap();
        assert_eq!(
            redis.scan(0, &format!("{p}*"), 1000).unwrap().keys,
            [keys[0].clone()],
            "{name}"
        );

        redis
            .write(&[Change::DeleteKey {
                key: keys[0].clone(),
            }])
            .unwrap();
    }
}

#[test]
#[ignore = "needs TQL_TEST_REDIS and/or TQL_TEST_VALKEY as host:port"]
fn reads_again_after_the_server_drops_the_connection() {
    for (name, place) in servers() {
        let (mut redis, p) = seeded(&place);
        let id = match redis.run("CLIENT ID").unwrap() {
            Reply::Number(id) => id,
            other => panic!("{name}: {other:?}"),
        };
        store(&place, 0)
            .run(&format!("CLIENT KILL ID {id}"))
            .unwrap();

        assert_eq!(
            redis
                .read(&format!("{p}:greeting"), &KeyKind::String, 0, 1)
                .unwrap(),
            Contents::Text(b"hello".to_vec()),
            "{name}"
        );
        cleanup(&mut redis, &p);
    }
}

#[test]
#[ignore = "needs TQL_TEST_REDIS and/or TQL_TEST_VALKEY as host:port"]
fn a_ttl_on_a_key_that_is_not_there_is_an_error() {
    for (name, place) in servers() {
        let mut redis = store(&place, 0);
        let missing = format!("{}:missing", prefix());
        assert!(redis.expire(&missing, Some(60)).is_err(), "{name}");
        assert!(redis.expire(&missing, None).is_err(), "{name}");
    }
}

#[test]
#[ignore = "needs TQL_TEST_REDIS and/or TQL_TEST_VALKEY as host:port"]
fn a_read_only_connection_runs_commands_that_only_look() {
    for (name, place) in servers() {
        let mut redis = store(&place, 0);
        for (command, subcommand) in [
            ("info", None),
            ("ping", None),
            ("time", None),
            ("config", Some("get")),
            ("xinfo", Some("stream")),
            ("object", Some("encoding")),
            ("client", Some("list")),
        ] {
            assert!(
                redis.reads_only(command, subcommand).unwrap(),
                "{name}: {command} {subcommand:?}"
            );
        }
        for (command, subcommand) in [
            ("set", None),
            ("flushall", None),
            ("shutdown", None),
            ("config", Some("set")),
            ("client", Some("kill")),
            ("eval", None),
            ("subscribe", None),
        ] {
            assert!(
                !redis.reads_only(command, subcommand).unwrap(),
                "{name}: {command} {subcommand:?}"
            );
        }
    }
}

#[test]
#[ignore = "needs TQL_TEST_REDIS and/or TQL_TEST_VALKEY as host:port"]
fn shows_a_stream_entry_with_a_repeated_field_and_a_long_string_in_part() {
    for (name, place) in servers() {
        let mut redis = store(&place, 0);
        let p = prefix();
        redis.run(&format!("XADD {p}:s 1-1 a 1 a 2")).unwrap();
        assert_eq!(
            rows(
                redis
                    .read(&format!("{p}:s"), &KeyKind::Stream, 0, 10)
                    .unwrap()
            )[0][1],
            r#"[["a","1"],["a","2"]]"#,
            "{name}"
        );

        redis
            .run(&format!("SET {p}:long {}", "x".repeat(300 * 1024)))
            .unwrap();
        match redis
            .read(&format!("{p}:long"), &KeyKind::String, 0, 1)
            .unwrap()
        {
            Contents::Long { head, size } => {
                assert_eq!(size, 300 * 1024, "{name}");
                assert_eq!(head.len(), 256 * 1024, "{name}");
            }
            other => panic!("{name}: {other:?}"),
        }

        cleanup(&mut redis, &p);
    }
}

#[test]
#[ignore = "needs TQL_TEST_REDIS and/or TQL_TEST_VALKEY as host:port"]
fn a_user_kept_to_some_keys_sees_the_types_of_the_ones_it_may_read() {
    for (name, place) in servers() {
        let (mut admin, p) = seeded(&place);
        let user = format!("{p}user");
        admin
            .run(&format!("ACL SETUSER {user} on >secret ~{p}:user:* +@all"))
            .unwrap();

        let (host, port) = place.rsplit_once(':').unwrap();
        let encrypter = app::encrypter::Encrypter::throwaway().unwrap();
        let mut limited = keystore::connect(
            &Connection {
                name: "redis".into(),
                driver: Driver::Redis,
                host: Some(host.into()),
                port: Some(port.parse().unwrap()),
                username: Some(user.clone()),
                password: Some(encrypter.encrypt_string("secret").unwrap()),
                ..Connection::sqlite("redis", "")
            },
            Some(&encrypter),
        )
        .unwrap();
        let kinds = limited
            .kinds(&[format!("{p}:greeting"), format!("{p}:user:1")])
            .unwrap();
        admin.run(&format!("ACL DELUSER {user}")).unwrap();

        assert_ne!(kinds[0], KeyKind::String, "{name}");
        assert_eq!(kinds[1], KeyKind::Hash, "{name}");
        cleanup(&mut admin, &p);
    }
}

#[test]
#[ignore = "needs TQL_TEST_REDIS and/or TQL_TEST_VALKEY as host:port"]
fn gives_up_on_a_server_that_never_answers() {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let started = std::time::Instant::now();
    let reached = keystore::connect(
        &Connection {
            name: "redis".into(),
            driver: Driver::Redis,
            host: Some("127.0.0.1".into()),
            port: Some(port),
            ..Connection::sqlite("redis", "")
        },
        None,
    );
    assert!(reached.is_err());
    assert!(started.elapsed() < std::time::Duration::from_secs(30));
    drop(listener);
}
