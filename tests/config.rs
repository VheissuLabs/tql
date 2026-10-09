use std::fs;
use std::path::PathBuf;

use app::config_file::{self, template};
use app::database;
use app::paths;
use app::store::Connection;
use app::terminal::Fake;
use app::tui::Browser;

fn directory(name: &str) -> PathBuf {
    let directory =
        std::env::temp_dir().join(format!("tql-rust-config-{}-{name}", std::process::id()));
    let _ = fs::remove_dir_all(&directory);
    paths::use_config_directory(directory.join("tql"));
    directory.join("tql")
}

fn config(arguments: &[&str]) -> (i32, String) {
    let result = app::application().run_with(
        std::iter::once("config").chain(arguments.iter().copied()),
        "",
    );
    (result.code, result.stdout + &result.stderr)
}

#[test]
fn prints_the_path_to_the_file() {
    let directory = directory("path");

    let (code, output) = config(&["--path"]);

    assert_eq!(code, 0);
    assert_eq!(
        output,
        format!("{}\n", directory.join("config.toml").display())
    );
}

#[test]
fn writes_the_file_first_as_every_tql_command_does() {
    let directory = directory("first");

    let (code, output) = config(&[]);

    let path = directory.join("config.toml");
    assert_eq!(code, 0);
    assert_eq!(fs::read_to_string(&path).unwrap(), template::render());
    assert_eq!(
        output,
        format!(
            "  {}\n  written for {}, template is {}\n  in order\n",
            path.display(),
            template::VERSION,
            template::VERSION
        )
    );
}

#[test]
fn says_what_is_out_of_order_and_which_keys_clash() {
    let directory = directory("report");
    fs::create_dir_all(&directory).unwrap();
    let path = directory.join("config.toml");
    let file = "# tql configuration 0.7.1\n\n[keys]\nfilter_rows = \"r\"\n\n[ui]\nsidebar_width = 40\nsql_position = \"bottom\"\n";
    fs::write(&path, file).unwrap();

    let (code, output) = config(&[]);

    assert_eq!(code, 0);
    assert_eq!(
        output,
        format!(
            "  {}\n  written for 0.7.1, template is {}\n  r is both filter_rows and reload\n  out of order: sidebar_width, sql_position\n  tql config --tidy sorts it, keeping your values.\n",
            path.display(),
            template::VERSION
        )
    );
    assert_eq!(fs::read_to_string(&path).unwrap(), file);
}

#[test]
fn tidies_the_file_keeping_a_backup_and_the_values() {
    let directory = directory("tidy");
    fs::create_dir_all(&directory).unwrap();
    let path = directory.join("config.toml");
    let scrambled = format!(
        "# tql configuration {}\n\n[ai]\n\n# mine\nkey = \"secret\"\nprovider = \"auto\"\n\n[ui]\nsidebar_width = 40\nsql_position = \"bottom\"\nnot_yet_in_tql = 3\n",
        template::VERSION
    );
    fs::write(&path, &scrambled).unwrap();

    let (code, output) = config(&["--tidy"]);

    assert_eq!(code, 0);
    assert_eq!(
        output,
        format!(
            "  Tidied {0}\n  4 settings moved: sidebar_width, sql_position, key, provider\n  The file as it was: {0}.bak\n",
            path.display()
        )
    );
    assert_eq!(
        fs::read_to_string(directory.join("config.toml.bak")).unwrap(),
        scrambled
    );
    let tidied = fs::read_to_string(&path).unwrap();
    assert_eq!(
        tidied,
        format!(
            "# tql configuration {}\n\n[ui]\n\nsql_position = \"bottom\"\n\nsidebar_width = 40\n\nnot_yet_in_tql = 3\n\n[ai]\n\nprovider = \"auto\"\n\n# mine\nkey = \"secret\"\n",
            template::VERSION
        )
    );

    let (_, again) = config(&["--tidy"]);
    assert_eq!(again, format!("  Already in order: {}\n", path.display()));
}

#[test]
fn the_browser_says_what_happened_to_the_config_file() {
    let directory = directory("status");
    let loaded = config_file::prepare(&directory.join("config.toml"));

    let path = directory.join("shop.sqlite");
    rusqlite::Connection::open(&path)
        .unwrap()
        .execute_batch("create table things (id integer primary key);")
        .unwrap();
    let connection = Connection::sqlite("shop", path.to_str().unwrap());
    let database = database::connect(&connection, None).unwrap();
    let mut fake = Fake::sized(
        Browser::new(connection, database)
            .unwrap()
            .with_status(loaded.message()),
        160,
        20,
    );

    let wrote = format!("wrote {}", directory.join("config.toml").display());
    assert_eq!(fake.app.status(), wrote);
    assert!(fake.screen().contains("wrote "), "{}", fake.screen());
}

#[test]
fn starting_never_tidies_and_only_tql_config_tidy_does() {
    let directory = directory("opt-in");
    fs::create_dir_all(&directory).unwrap();
    let path = directory.join("config.toml");
    let scrambled = format!(
        "# tql configuration {}\n\n\n[ai]\nkey = \"secret\"\nprovider = \"auto\"\n[ui]\nsidebar_width = 40\nsql_position = \"bottom\"\n",
        template::VERSION
    );
    fs::write(&path, &scrambled).unwrap();

    config_file::prepare(&path);
    config(&[]);
    config(&["--path"]);

    assert_eq!(fs::read_to_string(&path).unwrap(), scrambled);
    assert!(!directory.join("config.toml.bak").exists());

    config(&["--tidy"]);
    assert_ne!(fs::read_to_string(&path).unwrap(), scrambled);
}
