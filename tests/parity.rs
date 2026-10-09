use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use app::database::{self, Exporter};
use app::settings::{self, Settings};
use app::store::Store;
use app::terminal::Fake;
use app::tui::{Browser, Picker, Places};

const ROOT: &str = env!("CARGO_MANIFEST_DIR");

const EMPTY_TABLE_HEADERS: &str = "an empty table shows its column headers; tql shows 'no columns' because it reads headers from the rows";

const SQL_SERVER_INSERT: &str = "SQL Server adds the row and tql-rust says so; tql sends the insert through a select, and Microsoft's driver then refuses to read rows from it, so tql reports 'The active result for the query contains no fields' for a row it did add";

const DELIBERATE: &[(&str, &str)] = &[
    (
        "conn-edit-read-only-turned-off",
        "a connection that is read only turns it off with one press; tql's first press leaves it on because it holds the saved value as '1' rather than 'yes'",
    ),
    (
        "conn-edit-over-ssh-type-a-path",
        "typing into an empty field shows what is typed; tql shows the field's placeholder over it until enter",
    ),
    ("empty-table", EMPTY_TABLE_HEADERS),
    ("inspect-empty-table", EMPTY_TABLE_HEADERS),
    ("inspect-fold-record", TITLES_ARE_NOT_STOPS),
    ("inspect-fold-related", TITLES_ARE_NOT_STOPS),
    ("mysql-inspect-fold-record", TITLES_ARE_NOT_STOPS),
    ("mysql-inspect-fold-related", TITLES_ARE_NOT_STOPS),
    ("mysql-inspect-empty-table", EMPTY_TABLE_HEADERS),
    ("yank-empty-table", EMPTY_TABLE_HEADERS),
    ("export-empty-table", EMPTY_TABLE_HEADERS),
    (
        "export-results",
        "query results are exported as the text the grid holds, so numbers are quoted; tql writes them bare. Both re-import to the same rows",
    ),
    (
        "export-narrow",
        "a long status cuts the line by what is visible; tql counts colour codes too and cuts it early",
    ),
    (
        "sql-error",
        "the database's own words without PDO's 'SQLSTATE[HY000]: General error: 1' in front",
    ),
    (
        "sql-error-syntax",
        "the database's own words without PDO's 'SQLSTATE[HY000]: General error: 1' in front",
    ),
    (
        "sql-write",
        "a write says how many rows it changed; tql runs everything as a select and says '0 rows'",
    ),
    (
        "sql-sort-complex",
        "a long status cuts the line by what is visible; tql counts colour codes too and cuts it early",
    ),
    (
        "sql-sort-results",
        "sorting query results keeps the cursor on the sorted column, as sorting a table does; tql sends it back to the first column",
    ),
    (
        "sql-sort-results-desc",
        "sorting query results keeps the cursor on the sorted column, as sorting a table does; tql sends it back to the first column",
    ),
    (
        "sql-sort-results-cleared",
        "sorting query results keeps the cursor on the sorted column, as sorting a table does; tql sends it back to the first column",
    ),
    (
        "sql-small",
        "a long status cuts the line by what is visible; tql counts colour codes too and cuts it early",
    ),
    (
        "rows-filter-nothing-matches",
        "a filter that matches nothing keeps the column headers, so f can still reopen the filter; tql shows 'no columns' and then refuses to open it",
    ),
    (
        "mysql-sql-error",
        "errors show the database's own words without PDO's SQLSTATE prefix",
    ),
    (
        "mysql-sql-sort-results",
        "sorting query results keeps the cursor on the sorted column, and the status is cut by visible width",
    ),
    ("sqlsrv-inspect-empty-table", EMPTY_TABLE_HEADERS),
    (
        "sqlsrv-filter-quote",
        "a filter that matches nothing keeps the column headers; the escaped where clause in the SQL pane matches tql exactly",
    ),
    (
        "sqlsrv-sql-error",
        "errors show the database's own words without PDO's SQLSTATE and ODBC driver prefix",
    ),
    (
        "sqlsrv-sql-sort-results",
        "sorting query results keeps the cursor on the sorted column, and the status is cut by visible width",
    ),
    (
        "sqlsrv-values",
        "a decimal below one keeps its leading zero (0.15); Microsoft's ODBC driver hands tql .15",
    ),
    ("sqlsrv-new-row-written", SQL_SERVER_INSERT),
    ("sqlsrv-new-row-no-key-written", SQL_SERVER_INSERT),
    (
        "mysql-filter-quote",
        "a filter that matches nothing keeps the column headers; the escaped where clause in the SQL pane matches tql exactly",
    ),
];

const TITLES_ARE_NOT_STOPS: &str = "the inspector cursor never stops on a box title (there is nothing to do there), so a whole box cannot be folded from its title";

const HOTKEY_BAR_REASON: &str =
    "the hotkey bar only offers what tql-rust can do; tql also offers features not ported yet";

struct Scenario {
    name: String,
    connection: Option<String>,
    settings: Settings,
    hotkey_bar: bool,
    picker: bool,
    size: (u16, u16),
    keys: Vec<String>,
    groups: Vec<(String, String)>,
}

const EXPORTS: &str = "$EXPORTS";

fn export_stamps() -> regex::Regex {
    regex::Regex::new(r"\d{8}-\d{6}\.sql").unwrap()
}

fn scenario_json(exports: &Path) -> Vec<serde_json::Value> {
    let mut files: Vec<_> = fs::read_dir(Path::new(ROOT).join("parity/scenarios"))
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| {
            path.extension()
                .is_some_and(|extension| extension == "json")
        })
        .collect();
    files.sort_by_key(|file| {
        let name = file.file_name().unwrap().to_string_lossy().into_owned();
        (
            name != "sql-editor.json",
            name.starts_with("mysql-") || name.starts_with("sqlsrv-"),
            name,
        )
    });

    files
        .iter()
        .flat_map(|file| {
            let source = fs::read_to_string(file)
                .unwrap()
                .replace(EXPORTS, &exports.display().to_string());
            let json: serde_json::Value = serde_json::from_str(&source)
                .unwrap_or_else(|error| panic!("{} is not valid JSON: {error}", file.display()));
            json.as_array().unwrap().clone()
        })
        .collect()
}

fn scenarios(exports: &Path) -> Vec<Scenario> {
    scenario_json(exports)
        .iter()
        .map(|scenario| Scenario {
            name: scenario["name"].as_str().unwrap().to_string(),
            connection: scenario["connection"].as_str().map(String::from),
            groups: scenario["groups"]
                .as_object()
                .map(|groups| {
                    groups
                        .iter()
                        .map(|(name, group)| {
                            (name.clone(), group.as_str().unwrap_or_default().to_string())
                        })
                        .collect()
                })
                .unwrap_or_default(),
            settings: {
                let mut settings = Settings::default();
                settings.set("ui.top_margin", &toml::Value::Integer(0));
                if let Some(overrides) = scenario["config"].as_object() {
                    for (key, value) in overrides {
                        let value: toml::Value = serde_json::from_value(value.clone()).unwrap();
                        settings.set(key, &value);
                    }
                }
                settings
            },
            hotkey_bar: scenario["config"]["ui.hotkey_bar"].as_bool() != Some(false),
            picker: scenario["screen"].as_str() == Some("picker"),
            size: (
                scenario["size"][0].as_u64().unwrap() as u16,
                scenario["size"][1].as_u64().unwrap() as u16,
            ),
            keys: scenario["keys"]
                .as_array()
                .unwrap()
                .iter()
                .map(|key| key.as_str().unwrap().to_string())
                .collect(),
        })
        .collect()
}

fn timing_padding() -> regex::Regex {
    regex::Regex::new(r" {2,}").unwrap()
}

fn normalize(frame: &str) -> Vec<String> {
    let stamps = export_stamps();
    let mut lines: Vec<String> = frame
        .lines()
        .map(|line| stamps.replace_all(line, "YYYYMMDD-HHMMSS.sql").into_owned())
        .map(|line| {
            let mut normalized = String::new();
            let mut digits = String::new();
            let mut rest = line.as_str().trim_end();
            while let Some(character) = rest.chars().next() {
                rest = &rest[character.len_utf8()..];
                if character.is_ascii_digit() {
                    digits.push(character);
                    continue;
                }
                if character == 'm' && rest.starts_with('s') && !digits.is_empty() {
                    normalized.push_str("Nm");
                } else if character == 's' && rest.starts_with(" ago") && !digits.is_empty() {
                    normalized.push_str("Ns");
                } else {
                    normalized.push_str(&digits);
                    normalized.push(character);
                }
                digits.clear();
            }
            normalized + &digits
        })
        .map(|line| match line.contains("Nms") {
            true => timing_padding().replace_all(&line, "  ").into_owned(),
            false => line,
        })
        .collect();
    while lines.last().is_some_and(String::is_empty) {
        lines.pop();
    }
    lines
}

fn herd(sql: &str) {
    let mut child = Command::new("mysql")
        .args(["-uroot", "-h127.0.0.1", "-P3306"])
        .stdin(std::process::Stdio::piped())
        .spawn()
        .expect("the mysql client is installed");
    std::io::Write::write_all(child.stdin.as_mut().unwrap(), sql.as_bytes()).unwrap();
    assert!(
        child.wait().unwrap().success(),
        "mysql rejected the fixture"
    );
}

fn mysql_data() -> String {
    fs::read_to_string(Path::new(ROOT).join("parity/fixture-mysql-data.sql")).unwrap()
}

fn mysql_reset() -> String {
    format!(
        "{}use tql_rust_scratch;\n{}",
        fs::read_to_string(Path::new(ROOT).join("parity/reset-mysql.sql")).unwrap(),
        mysql_data()
    )
}

const SQL_SERVER: &str = "mssql";

fn sql_server_place() -> Option<String> {
    std::env::var("TQL_PARITY_SQLSRV").ok()
}

fn sql_server(database: &str) -> Box<dyn database::Database> {
    let place = sql_server_place().expect("TQL_PARITY_SQLSRV is host:port");
    let (host, port) = place
        .rsplit_once(':')
        .expect("TQL_PARITY_SQLSRV is host:port");
    let encrypter = app::encrypter::Encrypter::throwaway().unwrap();
    let password = std::env::var("TQL_PARITY_SQLSRV_PASSWORD").unwrap_or_default();
    let connection = app::store::Connection {
        name: SQL_SERVER.into(),
        driver: "sqlsrv".into(),
        host: Some(host.into()),
        port: Some(port.parse().unwrap()),
        database: Some(database.into()),
        username: Some("sa".into()),
        password: Some(encrypter.encrypt_string(&password).unwrap()),
        ..app::store::Connection::sqlite(SQL_SERVER, "")
    };
    database::connect(&connection, Some(&encrypter)).unwrap()
}

fn sql_server_reset() {
    sql_server("tql_rust_scratch")
        .run(&fs::read_to_string(Path::new(ROOT).join("parity/fixture-sqlsrv.sql")).unwrap())
        .unwrap();
}

// tql counts colour codes when it cuts a long line, so a dimmed line ends a
// few characters early. Two lines cut short that agree as far as the shorter
// one goes are the same line.
fn cut_alike(php: Option<&str>, rust: Option<&str>) -> bool {
    let (Some(php), Some(rust)) = (php, rust) else {
        return false;
    };
    if !php.ends_with('…') || !rust.ends_with('…') {
        return false;
    }
    let php = php.trim_end_matches('…').trim_end();
    let rust = rust.trim_end_matches('…').trim_end();
    php.starts_with(rust) || rust.starts_with(php)
}

fn diff(expected: &[String], actual: &[String]) -> Vec<String> {
    let mut report = Vec::new();
    for number in 0..expected.len().max(actual.len()) {
        let php = expected.get(number).map(String::as_str);
        let rust = actual.get(number).map(String::as_str);
        if php != rust && !cut_alike(php, rust) {
            report.push(format!(
                "{:>3} php  │{}",
                number + 1,
                php.unwrap_or("⟨none⟩")
            ));
            report.push(format!(
                "{:>3} rust │{}",
                number + 1,
                rust.unwrap_or("⟨none⟩")
            ));
        }
    }
    report
}

#[test]
#[ignore = "needs PHP and a tql checkout; run with --ignored"]
fn frames_match_the_php_tql() {
    let work = std::env::temp_dir().join(format!("tql-parity-{}", std::process::id()));
    let _ = fs::remove_dir_all(&work);
    fs::create_dir_all(&work).unwrap();

    let fixture = work.join("shop.sqlite");
    rusqlite::Connection::open(&fixture)
        .unwrap()
        .execute_batch(&fs::read_to_string(Path::new(ROOT).join("parity/fixture.sql")).unwrap())
        .unwrap();
    let pristine = fs::read(&fixture).unwrap();

    let mysql = std::env::var_os("TQL_PARITY_MYSQL").is_some();
    if mysql {
        herd(&format!(
            "drop database if exists tql_rust_scratch;\n{}\n{}",
            fs::read_to_string(Path::new(ROOT).join("parity/fixture-mysql.sql")).unwrap(),
            mysql_data()
        ));
    }

    let sqlsrv = sql_server_place().is_some();
    if sqlsrv {
        sql_server("master")
            .run_server_wide(
                "drop database if exists tql_rust_scratch; create database tql_rust_scratch",
            )
            .unwrap();
        sql_server_reset();
    }

    let home = work.join("home");
    for (path, contents) in [
        (
            ".ssh/id_parity",
            "-----BEGIN OPENSSH PRIVATE KEY-----\nnot a real key\n",
        ),
        (".ssh/id_parity.pub", "ssh-ed25519 AAAA parity"),
        (".ssh/config", "Host *"),
        ("certs/ca.pem", ""),
        ("certs/client.crt", ""),
        ("certs/client.key", ""),
        ("data/notes.sqlite", ""),
    ] {
        let file = home.join(path);
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        fs::write(file, contents).unwrap();
    }
    let places = Places {
        home: home.clone(),
        cwd: home.clone(),
    };

    let scenario_file = work.join("scenarios.json");
    fs::write(
        &scenario_file,
        serde_json::to_string(&scenario_json(&work.join("exports"))).unwrap(),
    )
    .unwrap();

    let php_frames = work.join("php");
    let mut render = Command::new("php");
    render
        .arg(Path::new(ROOT).join("parity/render.php"))
        .arg(&fixture)
        .arg(&scenario_file)
        .arg(&php_frames)
        .env("TQL_PARITY_HOME", &home);
    let openssl_3 = Path::new("/opt/homebrew/opt/openssl@3/lib");
    if sqlsrv && openssl_3.is_dir() {
        render.env("DYLD_LIBRARY_PATH", openssl_3);
    }
    let status = render.status().expect("php is installed");
    assert!(status.success(), "the PHP renderer failed");

    let report_dir = PathBuf::from(ROOT).join("target/parity");
    let _ = fs::remove_dir_all(&report_dir);
    fs::create_dir_all(&report_dir).unwrap();

    let mut matching = Vec::new();
    let mut differing = Vec::new();
    let mut deliberate = Vec::new();

    let saved = Store::open(&php_frames.join("tql.sqlite"))
        .unwrap()
        .connections()
        .unwrap();
    let history = work.join("rust-store.sqlite");
    fs::copy(php_frames.join("tql.sqlite"), &history).unwrap();
    rusqlite::Connection::open(&history)
        .unwrap()
        .execute("delete from query_executions", [])
        .unwrap();

    let encrypter = app::encrypter::Encrypter::from_file(&php_frames.join("key")).unwrap();
    let shop = saved
        .iter()
        .find(|connection| connection.name == "shop")
        .unwrap()
        .clone();

    for scenario in scenarios(&work.join("exports")) {
        fs::write(&fixture, &pristine).unwrap();
        let on_sql_server = scenario.connection.as_deref() == Some(SQL_SERVER);
        let available = if on_sql_server { sqlsrv } else { mysql };
        if scenario.connection.is_some() && !available {
            continue;
        }
        if on_sql_server {
            sql_server_reset();
        } else if scenario.connection.is_some() {
            herd(&mysql_reset());
        }
        settings::install(scenario.settings.clone());
        let screen = if scenario.picker {
            let store = work.join(format!("picker-{}.sqlite", scenario.name));
            fs::copy(php_frames.join("tql.sqlite"), &store).unwrap();
            let grouping = rusqlite::Connection::open(&store).unwrap();
            for (name, group) in &scenario.groups {
                grouping
                    .execute(
                        "update connections set group_name = ?1 where name = ?2",
                        [group, name],
                    )
                    .unwrap();
            }
            let mut fake = Fake::sized(
                Picker::new(Store::open(&store).unwrap().connections().unwrap(), None)
                    .with_store(&store, &php_frames.join("key"))
                    .with_places(places.clone()),
                scenario.size.0,
                scenario.size.1,
            );
            for key in &scenario.keys {
                fake.press(key);
            }
            fake.screen()
        } else {
            let connection = match &scenario.connection {
                Some(name) => saved
                    .iter()
                    .find(|connection| &connection.name == name)
                    .unwrap()
                    .clone(),
                None => shop.clone(),
            };
            let database = database::connect(&connection, Some(&encrypter)).unwrap();
            let mut fake = Fake::sized(
                Browser::new(connection, database)
                    .unwrap()
                    .with_store(history.clone()),
                scenario.size.0,
                scenario.size.1,
            );
            for key in &scenario.keys {
                fake.press(key);
            }
            fake.screen()
        };

        let mut expected = normalize(
            &fs::read_to_string(php_frames.join(format!("{}.txt", scenario.name))).unwrap(),
        );
        let mut actual = normalize(&screen);
        let bar = scenario.size.1 as usize - 3;
        if scenario.hotkey_bar && !scenario.picker && scenario.size.0 >= 60 && scenario.size.1 >= 12
        {
            for frame in [&mut expected, &mut actual] {
                if bar < frame.len() {
                    frame[bar] = format!("⟨hotkey bar: {HOTKEY_BAR_REASON}⟩");
                }
            }
        }
        let lines = diff(&expected, &actual);

        fs::write(
            report_dir.join(format!("{}.php.txt", scenario.name)),
            expected.join("\n"),
        )
        .unwrap();
        fs::write(
            report_dir.join(format!("{}.rust.txt", scenario.name)),
            actual.join("\n"),
        )
        .unwrap();

        if let Some((_, reason)) = DELIBERATE.iter().find(|(name, _)| *name == scenario.name) {
            deliberate.push(format!("{} ({reason})", scenario.name));
        } else if lines.is_empty() {
            matching.push(scenario.name);
        } else {
            fs::write(
                report_dir.join(format!("{}.diff", scenario.name)),
                lines.join("\n"),
            )
            .unwrap();
            differing.push(format!("{} ({} lines)", scenario.name, lines.len() / 2));
        }
    }

    if mysql {
        herd("drop database tql_rust_scratch;");
    }
    if sqlsrv {
        sql_server("master")
            .run_server_wide(
                "alter database tql_rust_scratch set single_user with rollback immediate; drop database tql_rust_scratch",
            )
            .unwrap();
    }

    println!("matching {}: {}", matching.len(), matching.join(", "));
    println!("differing {}: {}", differing.len(), differing.join(", "));
    println!(
        "deliberately different {}: {}",
        deliberate.len(),
        deliberate.join("; ")
    );
    println!("frames and diffs in {}", report_dir.display());
    assert!(
        differing.is_empty(),
        "{} scenarios differ from the PHP tql",
        differing.len()
    );
}

fn without_export_time(sql: &str) -> String {
    sql.lines()
        .map(|line| match line.strip_prefix("-- exported: ") {
            Some(_) => "-- exported: ⟨time⟩",
            None => line,
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
#[ignore = "needs PHP and a tql checkout; run with --ignored"]
fn export_files_match_the_php_tql() {
    let work = std::env::temp_dir().join(format!("tql-parity-export-{}", std::process::id()));
    let _ = fs::remove_dir_all(&work);
    fs::create_dir_all(&work).unwrap();

    let fixture = work.join("shop.sqlite");
    rusqlite::Connection::open(&fixture)
        .unwrap()
        .execute_batch(&fs::read_to_string(Path::new(ROOT).join("parity/fixture.sql")).unwrap())
        .unwrap();

    let php = work.join("php");
    let status = Command::new("php")
        .arg(Path::new(ROOT).join("parity/export.php"))
        .arg(&fixture)
        .arg(&php)
        .status()
        .expect("php is installed");
    assert!(status.success(), "the PHP exporter failed");

    let rust = work.join("rust");
    fs::create_dir_all(&rust).unwrap();
    let mut settings = Settings::default();
    settings.export_path = rust.display().to_string();
    settings::install(settings);
    let connection = app::store::Connection::sqlite("shop", fixture.to_str().unwrap());
    let mut database = database::connect(&connection, None).unwrap();
    let exporter = Exporter::new(&connection, None);
    let tables: Vec<String> =
        serde_json::from_str(&fs::read_to_string(php.join("tables.json")).unwrap()).unwrap();

    for table in &tables {
        exporter
            .table(
                database.as_mut(),
                table,
                Some(rust.join(format!("{table}.sql"))),
                None,
            )
            .unwrap();
    }
    exporter
        .table(
            database.as_mut(),
            "customers",
            Some(rust.join("customers-limited.sql")),
            Some(7),
        )
        .unwrap();
    exporter
        .tables(
            database.as_mut(),
            &tables,
            Some(rust.join("every-table.sql")),
            None,
        )
        .unwrap();
    exporter
        .tables(
            database.as_mut(),
            &tables,
            Some(rust.join("every-table-limited.sql")),
            Some(2),
        )
        .unwrap();
    exporter
        .rows(
            database.as_ref(),
            "results",
            &["id".to_string(), "name".to_string(), "notes".to_string()],
            &[
                vec![Some("1".to_string()), Some("it's".to_string()), None],
                vec![
                    Some("2".to_string()),
                    Some("line\nbreak".to_string()),
                    Some("x".to_string()),
                ],
            ],
            Some(rust.join("results.sql")),
        )
        .unwrap();

    let mut differing = Vec::new();
    let mut compared = 0;
    for entry in fs::read_dir(&php).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().is_none_or(|extension| extension != "sql") {
            continue;
        }
        let name = path.file_name().unwrap().to_owned();
        let expected = without_export_time(&fs::read_to_string(&path).unwrap());
        let actual = without_export_time(&fs::read_to_string(rust.join(&name)).unwrap());
        compared += 1;
        if expected != actual {
            differing.push(name.to_string_lossy().into_owned());
        }
    }

    println!("export files compared {compared}, differing: {differing:?}");
    println!("files in {}", work.display());
    assert!(compared > tables.len());
    assert!(
        differing.is_empty(),
        "{differing:?} differ from the PHP tql"
    );
}

const CONFIG_DELIBERATE: &[(&str, &str)] = &[
    (
        "ensure-current",
        "a file already at the template's version is left exactly as it is; tql rewrites it to drop the blank line under the heading",
    ),
    (
        "ensure-empty",
        "tql-rust stamps until tql would leave the file alone; tql drops one blank line under the heading on each of its next starts",
    ),
    (
        "ensure-newer",
        "a file stamped by a newer tql is left exactly as it is; tql 0.7.1 stamps it back down to 0.7.1",
    ),
];

fn config_cases() -> Vec<serde_json::Value> {
    let template = app::config_file::template::render();
    let pruned: String = template
        .lines()
        .filter(|line| !line.starts_with("sql_height") && !line.starts_with("check"))
        .map(|line| format!("{line}\n"))
        .collect();
    let ensure = |name: &str, contents: Option<&str>| serde_json::json!({"name": format!("ensure-{name}"), "kind": "ensure", "contents": contents});
    let tidy = |name: &str, contents: &str| serde_json::json!({"name": format!("tidy-{name}"), "kind": "tidy", "contents": contents});
    vec![
        serde_json::json!({"name": "template", "kind": "template"}),
        ensure("fresh", None),
        ensure("partial", Some("[ui]\nsql_position = \"bottom\"\n")),
        ensure(
            "user-values",
            Some("[ui]\nsql_position = \"bottom\"\ntop_margin = 7\n"),
        ),
        ensure(
            "own-note",
            Some("# my own note\n[ui]\nsql_position = \"bottom\"\n"),
        ),
        ensure("only-a-comment", Some("# empty config\n")),
        ensure("empty", Some("")),
        ensure(
            "current",
            Some("# tql configuration 0.7.1\n\n[ui]\nsidebar_width = 40\n"),
        ),
        ensure(
            "older",
            Some("# tql configuration 0.6.2\n\n[ui]\nsidebar_width = 40\n"),
        ),
        ensure(
            "older-in-place",
            Some(
                "# tql configuration 0.6.0\n[ui]\n# mine\nsql_editor = \"simple\"\n\nrow_style = \"bold\"\n[theme]\nborder = \"red\"\n",
            ),
        ),
        ensure(
            "older-sections-missing",
            Some("# tql configuration 0.3.0\n[ui]\nmouse = false\n"),
        ),
        ensure("unversioned", Some("[ui]\nsidebar_width = 40\n")),
        ensure(
            "newer",
            Some("# tql configuration 0.9.0\n[ui]\nsidebar_width = 40\n"),
        ),
        ensure(
            "pruned-unversioned",
            Some(&pruned.replace("# tql configuration 0.7.1\n", "")),
        ),
        ensure("pruned", Some(&pruned)),
        ensure(
            "keys",
            Some("# tql configuration 0.7.0\n[keys]\nsql = \"S\"\n"),
        ),
        tidy(
            "scrambled",
            "# tql configuration 0.4.0\n\n[ai]\n\n# mine\nkey = \"secret\"\nprovider = \"auto\"\n\n[ui]\nsidebar_width = 40\n\n# Where the SQL editor sits: \"top\" or \"bottom\"\nsql_position = \"bottom\"",
        ),
        tidy(
            "loose",
            "# tql configuration 0.4.0\nmouse_row_offset = 1\n\n[ui]\nsidebar_width = 40\n",
        ),
        tidy(
            "unknown-section",
            "# tql configuration 0.4.0\n\n[mine]\nwhatever = true\n\n[ui]\nmystery = 1\nsidebar_width = 40\n",
        ),
        tidy("template", &template),
        tidy("pruned", &pruned),
        tidy(
            "no-heading",
            "[theme]\nborder = \"red\"\n[ui]\nmouse = false\n",
        ),
        tidy(
            "keys-first",
            "# tql configuration 0.7.1\n\n[keys]\nsql = \"S\"\n\n[ui]\n# a note\n\nmouse = false\nsidebar_width = 30\n",
        ),
    ]
}

fn rust_config_case(case: &serde_json::Value, work: &Path) -> (String, serde_json::Value) {
    use app::config_file::{file, template, tidy};

    let contents = case["contents"].as_str();
    match case["kind"].as_str().unwrap() {
        "template" => (template::render(), serde_json::Value::Null),
        "tidy" => {
            let (tidied, moved) = tidy::apply(contents.unwrap());
            (tidied, serde_json::json!({ "moved": moved }))
        }
        _ => {
            let path = work.join(format!(
                "{}-home/tql/config.toml",
                case["name"].as_str().unwrap()
            ));
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            if let Some(contents) = contents {
                fs::write(&path, contents).unwrap();
            }
            let ensured = file::ensure(&path).unwrap();
            (
                fs::read_to_string(&path).unwrap(),
                serde_json::json!({ "created": ensured.created, "added": ensured.added }),
            )
        }
    }
}

#[test]
#[ignore = "needs PHP and a tql checkout; run with --ignored"]
fn the_config_file_matches_the_php_tql() {
    let work = std::env::temp_dir().join(format!("tql-parity-config-{}", std::process::id()));
    let _ = fs::remove_dir_all(&work);
    let php = work.join("php");
    let rust = work.join("rust");
    fs::create_dir_all(&rust).unwrap();

    let cases = config_cases();
    let case_file = work.join("cases.json");
    fs::write(&case_file, serde_json::to_string(&cases).unwrap()).unwrap();
    let status = Command::new("php")
        .arg(Path::new(ROOT).join("parity/config.php"))
        .arg(&case_file)
        .arg(&php)
        .status()
        .expect("php is installed");
    assert!(status.success(), "the PHP config renderer failed");

    let blank_lines_under_heading = regex::Regex::new(r"0\.7\.1\n+").unwrap();
    let mut differing = Vec::new();
    let mut deliberate = Vec::new();
    for case in &cases {
        let name = case["name"].as_str().unwrap();
        let (written, result) = rust_config_case(case, &rust);
        // tql-rust starts with no blank rows above the frame; tql starts with one.
        let expected = fs::read_to_string(php.join(format!("{name}.toml")))
            .unwrap()
            .replace("top_margin = 1\n", "top_margin = 0\n");
        let expected_result = fs::read_to_string(php.join(format!("{name}.json")))
            .map(|json| serde_json::from_str(&json).unwrap())
            .unwrap_or(serde_json::Value::Null);
        let same = written == expected && result == expected_result;
        match CONFIG_DELIBERATE
            .iter()
            .find(|(deliberate, _)| *deliberate == name)
        {
            Some((_, reason)) => {
                assert!(
                    !same,
                    "{name} is listed as deliberately different but matches"
                );
                let untouched = Some(written.as_str()) == case["contents"].as_str();
                let settled = written == blank_lines_under_heading.replace(&expected, "0.7.1\n");
                assert!(untouched || settled, "{name}");
                assert_eq!(result, expected_result, "{name}");
                deliberate.push(format!("{name} ({reason})"));
            }
            None if same => {}
            None => differing.push(format!(
                "{name}\n--- php {expected_result}\n{expected}\n--- rust {result}\n{written}"
            )),
        }
    }

    println!(
        "config cases: {} matching, {} deliberately different: {}",
        cases.len() - differing.len() - deliberate.len(),
        deliberate.len(),
        deliberate.join("; ")
    );
    assert!(differing.is_empty(), "{}", differing.join("\n\n"));
}

fn php_config(cases: &[serde_json::Value], work: &Path) -> PathBuf {
    let case_file = work.join("cases.json");
    let out = work.join("php");
    fs::create_dir_all(work).unwrap();
    fs::write(&case_file, serde_json::to_string(cases).unwrap()).unwrap();
    let status = Command::new("php")
        .arg(Path::new(ROOT).join("parity/config.php"))
        .arg(&case_file)
        .arg(&out)
        .status()
        .expect("php is installed");
    assert!(status.success(), "the PHP config renderer failed");
    out
}

fn shared_files() -> Vec<(&'static str, Option<String>)> {
    let template = app::config_file::template::render();
    let pruned: String = template
        .lines()
        .filter(|line| !line.starts_with("sql_height") && !line.starts_with("check"))
        .map(|line| format!("{line}\n"))
        .collect();
    vec![
        ("fresh", None),
        ("template", Some(template.clone())),
        ("partial", Some("[ui]\nsql_position = \"bottom\"\n".into())),
        ("unversioned", Some("[ui]\nsidebar_width = 40\n".into())),
        ("empty", Some(String::new())),
        ("older-blank-line", Some("# tql configuration 0.6.2\n\n[ui]\nsidebar_width = 40\n".into())),
        ("older-blank-lines", Some("# tql configuration 0.6.2\n\n\n\n[ui]\nsidebar_width = 40\n".into())),
        ("older-in-place", Some("# tql configuration 0.6.0\n[ui]\n# mine\nsql_editor = \"vim\"\n\nrow_style = \"bold\"\n[theme]\nborder = \"red\"\n".into())),
        ("older-crlf", Some("# tql configuration 0.6.0\r\n[ui]\r\nmouse = false\r\n".into())),
        ("older-sections-missing", Some("# tql configuration 0.3.0\n[ui]\nmouse = false\n".into())),
        ("current-blank-line", Some("# tql configuration 0.7.1\n\n[ui]\nsidebar_width = 40\n".into())),
        ("pruned", Some(pruned.clone())),
        ("pruned-unversioned", Some(pruned.replace("# tql configuration 0.7.1\n", ""))),
        ("keys", Some("# tql configuration 0.7.0\n[keys]\nsql = \"S\"\nyank_value = [\"y\", \"ctrl+y\"]\n".into())),
        ("newer", Some("# tql configuration 0.9.0\n[ui]\nsidebar_width = 40\n".into())),
    ]
}

fn rust_starts(path: &Path) -> String {
    app::config_file::prepare(path);
    fs::read_to_string(path).unwrap()
}

#[test]
#[ignore = "needs PHP and a tql checkout; run with --ignored"]
fn tql_and_tql_rust_leave_each_others_config_file_alone() {
    let work = std::env::temp_dir().join(format!("tql-parity-shared-{}", std::process::id()));
    let _ = fs::remove_dir_all(&work);
    fs::create_dir_all(&work).unwrap();
    let files = shared_files();
    let ensure_case = |prefix: &str, name: &str, contents: Option<&str>| serde_json::json!({"name": format!("{prefix}-{name}"), "kind": "ensure", "contents": contents});

    let mut first_round: Vec<serde_json::Value> = files
        .iter()
        .map(|(name, contents)| ensure_case("php", name, contents.as_deref()))
        .collect();
    first_round.push(serde_json::json!({"name": "version", "kind": "version"}));
    let php_first = php_config(&first_round, &work.join("first"));

    assert_eq!(
        fs::read_to_string(php_first.join("version.toml")).unwrap(),
        app::config_file::template::VERSION,
        "tql-rust stamps the version of the template it ported"
    );

    let mut problems = Vec::new();
    let mut rust_wrote = Vec::new();
    for (name, contents) in &files {
        let written_by_tql =
            fs::read_to_string(php_first.join(format!("php-{name}.toml"))).unwrap();
        let path = work.join(format!("rust-after-php/{name}/tql/config.toml"));
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, &written_by_tql).unwrap();
        if rust_starts(&path) != written_by_tql {
            problems.push(format!("tql-rust changed the file tql wrote for {name}"));
        }

        let path = work.join(format!("rust-first/{name}/tql/config.toml"));
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        if let Some(contents) = contents {
            fs::write(&path, contents).unwrap();
        }
        let written_by_rust = rust_starts(&path);
        if contents.as_deref() != Some(written_by_rust.as_str()) {
            rust_wrote.push((name.to_string(), written_by_rust));
        }
    }

    let second_round: Vec<serde_json::Value> = rust_wrote
        .iter()
        .map(|(name, written)| ensure_case("rust", name, Some(written)))
        .collect();
    let php_second = php_config(&second_round, &work.join("second"));
    for (name, written_by_rust) in &rust_wrote {
        let after_tql = fs::read_to_string(php_second.join(format!("rust-{name}.toml"))).unwrap();
        if &after_tql != written_by_rust {
            problems.push(format!("tql changed the file tql-rust wrote for {name}"));
        }
    }

    println!(
        "shared config files: {} written by tql left alone by tql-rust, {} written by tql-rust left alone by tql",
        files.len(),
        rust_wrote.len()
    );
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}
