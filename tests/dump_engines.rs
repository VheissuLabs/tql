use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use app::dump::{DumpOptions, Engine, Job, LoadOptions};
use app::encrypter::Encrypter;
use app::store::Connection;

fn encrypter() -> Encrypter {
    let key = std::env::temp_dir().join(format!("tql-dump-engines-{}.key", std::process::id()));
    Encrypter::ensure(&key).unwrap()
}

fn server(encrypter: &Encrypter, driver: &str) -> Connection {
    Connection {
        name: format!("{driver}-server"),
        driver: driver.into(),
        host: Some("db.example.test".into()),
        port: Some(if driver == "mysql" { 3306 } else { 5432 }),
        database: Some("notarydash".into()),
        username: Some("app".into()),
        password: Some(encrypter.encrypt_string("p@ss:w\"rd\\1").unwrap()),
        ..Connection::default()
    }
}

fn defaults_file(job: &Job) -> PathBuf {
    let flag = job
        .command
        .iter()
        .find_map(|part| part.strip_prefix("--defaults-file="))
        .unwrap();
    PathBuf::from(flag)
}

fn read(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap()
}

#[test]
fn keeps_the_mysql_password_off_the_command_line_and_in_a_private_file() {
    let encrypter = encrypter();
    let options = DumpOptions {
        tables: vec!["orders".into(), "users".into()],
        threads: 4,
        ..DumpOptions::default()
    };

    let job = Engine::MySql
        .dump(
            &server(&encrypter, "mysql"),
            Path::new("/tmp/out"),
            &options,
            Some(&encrypter),
        )
        .unwrap();
    let defaults = defaults_file(&job);

    assert!(!job.describe().contains("p@ss"));
    assert!(job.command.contains(&"--database=notarydash".to_string()));
    assert!(
        job.command
            .contains(&"--tables-list=notarydash.orders,notarydash.users".to_string())
    );
    assert_eq!(
        read(&defaults),
        "[client]\nhost=\"db.example.test\"\nport=\"3306\"\nuser=\"app\"\npassword=\"p@ss:w\\\"rd\\\\1\"\n"
    );
    assert_eq!(
        std::fs::metadata(&defaults).unwrap().permissions().mode() & 0o777,
        0o600
    );
    assert_eq!(
        std::fs::metadata(defaults.parent().unwrap())
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o700
    );
}

#[test]
fn builds_the_whole_mydumper_command() {
    let encrypter = encrypter();
    let options = DumpOptions {
        tables: vec![],
        threads: 2,
        data_only: true,
        no_lock: true,
    };

    let job = Engine::MySql
        .dump(
            &server(&encrypter, "mysql"),
            Path::new("/tmp/my out"),
            &options,
            Some(&encrypter),
        )
        .unwrap();

    assert_eq!(
        job.describe(),
        format!(
            "mydumper --defaults-file={} --database=notarydash '--outputdir=/tmp/my out' --threads=2 --triggers --routines --skip-definer --verbose=3 --no-schemas --sync-thread-lock-mode=NO_LOCK",
            defaults_file(&job).display()
        )
    );
}

#[test]
fn replaces_tables_on_a_mysql_load_only_when_asked() {
    let encrypter = encrypter();
    let connection = server(&encrypter, "mysql");
    let load = |drop| {
        Engine::MySql
            .load(
                &connection,
                Path::new("/tmp/in"),
                &LoadOptions { drop, threads: 4 },
                Some(&encrypter),
            )
            .unwrap()
            .command
    };

    assert!(!load(false).contains(&"--drop-table=DROP".to_string()));
    assert!(load(true).contains(&"--drop-table=DROP".to_string()));
    assert_eq!(
        &load(false)[2..],
        [
            "--directory=/tmp/in",
            "--database=notarydash",
            "--threads=4",
            "--verbose=3"
        ]
    );
}

#[test]
fn maps_the_ssl_mode_and_certificates_into_the_client_file() {
    let encrypter = encrypter();
    let connection = Connection {
        ssl_mode: Some("verify-full".into()),
        ssl_ca: Some("/certs/ca.pem".into()),
        password: None,
        ..server(&encrypter, "mysql")
    };

    let job = Engine::MySql
        .dump(
            &connection,
            Path::new("/tmp/out"),
            &DumpOptions::default(),
            Some(&encrypter),
        )
        .unwrap();

    assert_eq!(
        read(&defaults_file(&job)),
        "[client]\nhost=\"db.example.test\"\nport=\"3306\"\nuser=\"app\"\nssl-mode=\"VERIFY_IDENTITY\"\nssl-ca=\"/certs/ca.pem\"\n"
    );
}

#[test]
fn hands_postgres_its_password_through_a_password_file() {
    let encrypter = encrypter();
    let connection = Connection {
        ssl_mode: Some("require".into()),
        ..server(&encrypter, "pgsql")
    };

    let job = Engine::Postgres
        .load(
            &connection,
            Path::new("/tmp/in"),
            &LoadOptions {
                drop: true,
                threads: 4,
            },
            Some(&encrypter),
        )
        .unwrap();
    let environment = |key: &str| {
        job.environment
            .iter()
            .find(|(name, _)| name == key)
            .map(|(_, value)| value.clone())
    };

    assert!(!job.describe().contains("p@ss"));
    assert_eq!(
        job.describe(),
        "pg_restore --host=db.example.test --port=5432 --username=app --dbname=notarydash --no-password --format=directory --jobs=4 --no-owner --no-privileges --exit-on-error --verbose --clean --if-exists /tmp/in"
    );
    assert_eq!(environment("PGSSLMODE").as_deref(), Some("require"));
    assert_eq!(environment("PGSSLCERT"), None);
    assert_eq!(
        read(Path::new(&environment("PGPASSFILE").unwrap())),
        "*:*:*:*:p@ss\\:w\"rd\\\\1\n"
    );
}

#[test]
fn builds_the_whole_pg_dump_command() {
    let encrypter = encrypter();
    let options = DumpOptions {
        tables: vec!["orders".into(), "users".into()],
        threads: 3,
        data_only: true,
        no_lock: false,
    };

    let job = Engine::Postgres
        .dump(
            &server(&encrypter, "pgsql"),
            Path::new("/tmp/out"),
            &options,
            Some(&encrypter),
        )
        .unwrap();

    assert_eq!(
        job.describe(),
        "pg_dump --host=db.example.test --port=5432 --username=app --dbname=notarydash --no-password --format=directory --jobs=3 --file=/tmp/out --verbose --table=orders --table=users --data-only"
    );
}

#[test]
fn gives_every_job_its_own_connection_file_so_a_dump_and_a_load_prepared_together_each_reach_their_own_server()
 {
    let encrypter = encrypter();
    let source = Connection {
        host: Some("prod.example.test".into()),
        ..server(&encrypter, "mysql")
    };
    let target = Connection {
        host: Some("127.0.0.1".into()),
        ..server(&encrypter, "mysql")
    };

    let dump = Engine::MySql
        .dump(
            &source,
            Path::new("/tmp/out"),
            &DumpOptions::default(),
            Some(&encrypter),
        )
        .unwrap();
    let load = Engine::MySql
        .load(
            &target,
            Path::new("/tmp/out"),
            &LoadOptions {
                drop: true,
                threads: 4,
            },
            Some(&encrypter),
        )
        .unwrap();

    assert!(read(&defaults_file(&dump)).contains("host=\"prod.example.test\""));
    assert!(read(&defaults_file(&load)).contains("host=\"127.0.0.1\""));
}

#[test]
fn recognises_each_engines_dump_by_what_it_leaves_behind() {
    let folder = std::env::temp_dir().join(format!("tql-dump-recognise-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&folder);
    std::fs::create_dir_all(&folder).unwrap();

    assert_eq!(Engine::recognise(&folder), None);
    std::fs::write(folder.join("toc.dat"), "").unwrap();
    assert_eq!(Engine::recognise(&folder), Some(Engine::Postgres));
    std::fs::write(folder.join("metadata"), "").unwrap();
    assert_eq!(Engine::recognise(&folder), Some(Engine::MySql));
    std::fs::write(folder.join("tql.json"), "{\"engine\": \"sqlite\"}").unwrap();
    assert_eq!(Engine::recognise(&folder), Some(Engine::Sqlite));

    let _ = std::fs::remove_dir_all(&folder);
}
