use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use app::database;
use app::settings::{self, Settings};
use app::store::Store;
use app::tui::{Browser, Picker};
use zerorust_tui::Fake;

const ROOT: &str = env!("CARGO_MANIFEST_DIR");

const DELIBERATE: &[(&str, &str)] = &[
    (
        "empty-table",
        "an empty table shows its column headers; tql shows 'no columns' because it reads headers from the rows",
    ),
    (
        "rows-filter-nothing-matches",
        "a filter that matches nothing keeps the column headers, so f can still reopen the filter; tql shows 'no columns' and then refuses to open it",
    ),
    (
        "mysql-filter-quote",
        "a filter that matches nothing keeps the column headers; the escaped where clause in the SQL pane matches tql exactly",
    ),
];

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
}

fn scenario_json() -> Vec<serde_json::Value> {
    let mut files: Vec<_> = fs::read_dir(Path::new(ROOT).join("parity/scenarios"))
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| {
            path.extension()
                .is_some_and(|extension| extension == "json")
        })
        .collect();
    files.sort();

    files
        .iter()
        .flat_map(|file| {
            let json: serde_json::Value = serde_json::from_str(&fs::read_to_string(file).unwrap())
                .unwrap_or_else(|error| panic!("{} is not valid JSON: {error}", file.display()));
            json.as_array().unwrap().clone()
        })
        .collect()
}

fn scenarios() -> Vec<Scenario> {
    scenario_json()
        .iter()
        .map(|scenario| Scenario {
            name: scenario["name"].as_str().unwrap().to_string(),
            connection: scenario["connection"].as_str().map(String::from),
            settings: {
                let mut settings = Settings::default();
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

fn normalize(frame: &str) -> Vec<String> {
    let mut lines: Vec<String> = frame
        .lines()
        .map(|line| {
            let mut normalized = String::new();
            let mut digits = String::new();
            let mut rest = line.trim_end();
            while let Some(character) = rest.chars().next() {
                rest = &rest[character.len_utf8()..];
                if character.is_ascii_digit() {
                    digits.push(character);
                    continue;
                }
                if character == 'm' && rest.starts_with('s') && !digits.is_empty() {
                    normalized.push_str("Nm");
                } else {
                    normalized.push_str(&digits);
                    normalized.push(character);
                }
                digits.clear();
            }
            normalized + &digits
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

fn diff(expected: &[String], actual: &[String]) -> Vec<String> {
    let mut report = Vec::new();
    for number in 0..expected.len().max(actual.len()) {
        let php = expected.get(number).map(String::as_str);
        let rust = actual.get(number).map(String::as_str);
        if php != rust {
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
            "drop database if exists tql_rust_scratch;\n{}",
            fs::read_to_string(Path::new(ROOT).join("parity/fixture-mysql.sql")).unwrap()
        ));
    }

    let scenario_file = work.join("scenarios.json");
    fs::write(
        &scenario_file,
        serde_json::to_string(&scenario_json()).unwrap(),
    )
    .unwrap();

    let php_frames = work.join("php");
    let status = Command::new("php")
        .arg(Path::new(ROOT).join("parity/render.php"))
        .arg(&fixture)
        .arg(&scenario_file)
        .arg(&php_frames)
        .status()
        .expect("php is installed");
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
    let shop = saved
        .iter()
        .find(|connection| connection.name == "shop")
        .unwrap()
        .clone();

    for scenario in scenarios() {
        fs::write(&fixture, &pristine).unwrap();
        if scenario.connection.is_some() && !mysql {
            continue;
        }
        settings::install(scenario.settings.clone());
        let screen = if scenario.picker {
            let mut fake = Fake::sized(
                Picker::new(saved.clone(), None),
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
            let database = database::connect(&connection, None).unwrap();
            let mut fake = Fake::sized(
                Browser::new(connection, database).unwrap(),
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
        if scenario.hotkey_bar && scenario.size.0 >= 60 && scenario.size.1 >= 12 {
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
