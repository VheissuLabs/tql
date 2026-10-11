use std::fs;
use std::path::PathBuf;

use app::config_file;
use app::paths;
use app::settings;
use app::terminal::Fake;
use app::tui::Picker;

const BACKSPACE: &str = "\x7f";

struct Scratch {
    root: PathBuf,
}

impl Scratch {
    fn new(label: &str) -> Self {
        let root =
            std::env::temp_dir().join(format!("tql-settings-modal-{}-{label}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("tql")).unwrap();
        paths::use_config_directory(root.join("tql"));
        Scratch { root }
    }

    fn with_config(self, contents: &str) -> Self {
        fs::write(self.config(), contents).unwrap();
        self
    }

    fn config(&self) -> PathBuf {
        self.root.join("tql/config.toml")
    }

    fn text(&self) -> String {
        fs::read_to_string(self.config()).unwrap()
    }

    fn opened(&self) -> Fake<Picker> {
        settings::install(config_file::load(&self.config()).settings);
        let mut fake = Fake::sized(Picker::new(Vec::new(), None), 120, 40);
        fake.press(":settings\n");
        fake
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

const MINE: &str = "# my own note\n[ui]\n# how wide the tables are\nsidebar_width = 24\nhotkey_bar = true\n\n[ai]\nkey = \"sk-secret-value\"\n";

#[test]
fn opens_with_every_section_and_says_where_it_saves() {
    let scratch = Scratch::new("open").with_config(MINE);
    let mut fake = scratch.opened();
    let screen = fake.screen();

    let modal = fake.app.settings.as_ref().unwrap();
    let sections: Vec<&str> = modal
        .rows()
        .into_iter()
        .filter_map(|row| match row {
            app::tui::settings_modal::Row::Header(section) => Some(section),
            _ => None,
        })
        .collect();
    assert_eq!(sections, ["ui", "theme", "updates", "ai", "mcp", "icons"]);
    assert!(screen.contains("SETTINGS"), "{screen}");
    assert!(screen.contains("▾ UI"), "{screen}");
    assert!(screen.contains("sidebar_width"), "{screen}");
    assert!(screen.contains("Saved as you go to"), "{screen}");
}

#[test]
fn flips_a_flag_writes_it_and_applies_it_at_once() {
    let scratch = Scratch::new("flag").with_config(MINE);
    let mut fake = scratch.opened();

    fake.press("/hotkey\n\n");

    assert!(scratch.text().contains("hotkey_bar = false"));
    assert!(!settings::with(|settings| settings.hotkey_bar));
    assert!(fake.screen().contains("hotkey_bar is off"));
}

#[test]
fn steps_through_choices_with_the_arrows() {
    let scratch = Scratch::new("choice").with_config(MINE);
    let mut fake = scratch.opened();

    fake.press("/sql_editor\nl");
    assert!(scratch.text().contains("sql_editor = \"vim\""));
    assert!(settings::with(|settings| settings.sql_vim));

    fake.press("h");
    assert!(scratch.text().contains("sql_editor = \"simple\""));
    assert!(!settings::with(|settings| settings.sql_vim));
}

#[test]
fn types_a_number_and_refuses_one_out_of_range() {
    let scratch = Scratch::new("number").with_config(MINE);
    let mut fake = scratch.opened();

    fake.press("/sidebar\n\n")
        .press(BACKSPACE)
        .press(BACKSPACE)
        .press("40\n");
    assert!(scratch.text().contains("sidebar_width = 40"));
    assert_eq!(settings::with(|settings| settings.sidebar_width), 40);

    fake.press("\n")
        .press(BACKSPACE)
        .press(BACKSPACE)
        .press("4\n");
    assert!(fake.screen().contains("takes a whole number from 8 to 500"));
    assert!(scratch.text().contains("sidebar_width = 40"));

    fake.press("\x1b");
    assert!(fake.app.settings.as_ref().unwrap().editing().is_none());
}

#[test]
fn starts_editing_with_the_value_selected_so_typing_replaces_it() {
    let scratch = Scratch::new("replace").with_config(MINE);
    let mut fake = scratch.opened();

    fake.press("/time_zone\n\nAmerica/Toronto\n");
    assert!(scratch.text().contains("time_zone = \"America/Toronto\""));

    fake.press("\n\x1b[D/Montreal\n");
    assert!(
        scratch
            .text()
            .contains("time_zone = \"/MontrealAmerica/Toronto\""),
        "{}",
        scratch.text()
    );

    fake.press("\n").press(BACKSPACE).press("\n");
    assert!(scratch.text().contains("time_zone = \"\""));
}

#[test]
fn keeps_the_comments_and_order_of_the_file() {
    let scratch = Scratch::new("comments").with_config(MINE);
    let mut fake = scratch.opened();

    fake.press("/sidebar\n\n")
        .press(BACKSPACE)
        .press(BACKSPACE)
        .press("30\n");

    assert_eq!(
        scratch.text(),
        MINE.replace("sidebar_width = 24", "sidebar_width = 30")
    );
}

#[test]
fn adds_a_setting_the_file_does_not_have_yet() {
    let scratch = Scratch::new("missing").with_config(MINE);
    let mut fake = scratch.opened();

    fake.press("/left_margin\n\n").press(BACKSPACE).press("3\n");

    let table: toml::Table = toml::from_str(&scratch.text()).unwrap();
    assert_eq!(table["ui"]["left_margin"].as_integer(), Some(3));
    assert_eq!(settings::with(|settings| settings.left_margin), 3);
}

#[test]
fn creates_the_file_when_there_is_none() {
    let scratch = Scratch::new("no-file");
    let mut fake = scratch.opened();

    fake.press("/hotkey\n\n");

    let table: toml::Table = toml::from_str(&scratch.text()).unwrap();
    assert_eq!(table["ui"]["hotkey_bar"].as_bool(), Some(false));
    assert!(scratch.text().contains("# tql configuration"));
}

#[test]
fn x_puts_a_setting_back_to_its_default() {
    let scratch = Scratch::new("default").with_config(MINE);
    let mut fake = scratch.opened();

    fake.press("/sidebar\n\n")
        .press(BACKSPACE)
        .press(BACKSPACE)
        .press("50\n");
    fake.press("x");

    assert!(scratch.text().contains("sidebar_width = 24"));
}

#[test]
fn never_shows_the_ai_key() {
    let scratch = Scratch::new("secret").with_config(MINE);
    let mut fake = scratch.opened();

    fake.press("/key\n");
    let screen = fake.screen();

    assert!(!screen.contains("sk-secret-value"), "{screen}");
    assert!(screen.contains("••••"), "{screen}");
}

#[test]
fn says_when_a_setting_waits_for_the_next_start() {
    let scratch = Scratch::new("later").with_config(MINE);
    let mut fake = scratch.opened();

    fake.press("/mouse\n");
    let row = fake.screen();
    assert!(row.contains("Applies the next time tql starts."), "{row}");

    fake.press("\n");
    assert!(fake.screen().contains("mouse is off · applies next start"));
}

#[test]
fn a_section_folds_and_esc_closes() {
    let scratch = Scratch::new("fold").with_config(MINE);
    let mut fake = scratch.opened();

    fake.press("\n");
    assert!(!fake.screen().contains("sidebar_width"));

    fake.press("\n");
    assert!(fake.screen().contains("sidebar_width"));

    fake.press("\x1b");
    assert!(fake.app.settings.is_none());
}

#[test]
fn the_connection_list_palette_offers_it() {
    let scratch = Scratch::new("palette").with_config(MINE);
    settings::install(config_file::load(&scratch.config()).settings);
    let mut fake = Fake::sized(Picker::new(Vec::new(), None), 120, 40);

    fake.press("\x0bsettings\n");

    assert!(fake.app.settings.is_some());
}

#[test]
fn tql_settings_without_a_terminal_says_where_the_file_is() {
    let scratch = Scratch::new("shell").with_config(MINE);

    let run = app::application().run_with(["settings"], "");

    assert_eq!(run.code, 0, "{run:?}");
    assert_eq!(run.stdout.trim(), scratch.config().display().to_string());
}

#[test]
fn the_browser_opens_it_from_the_colon_line_and_the_palette() {
    let scratch = Scratch::new("browser").with_config(MINE);
    settings::install(config_file::load(&scratch.config()).settings);
    let path = scratch.root.join("shop.sqlite");
    rusqlite::Connection::open(&path)
        .unwrap()
        .execute_batch("create table settings (key text primary key)")
        .unwrap();
    let connection = app::store::Connection::sqlite("shop", path.to_str().unwrap());
    let database = app::database::connect(&connection, None).unwrap();
    let mut fake = Fake::sized(
        app::tui::Browser::new(connection, database).unwrap(),
        120,
        40,
    );

    fake.press(":settings\n");
    assert!(fake.screen().contains("SETTINGS"));
    fake.press("\x1b");
    assert!(fake.app.settings.is_none());

    fake.press("\x0bsettings\n");
    assert!(fake.app.settings.is_some());
}
