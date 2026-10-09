pub const VERSION: &str = "0.7.1";

pub struct Setting {
    pub section: &'static str,
    pub key: &'static str,
    pub default: &'static str,
    pub since: &'static str,
    pub comment: &'static [&'static str],
}

const fn setting(
    section: &'static str,
    key: &'static str,
    default: &'static str,
    since: &'static str,
    comment: &'static [&'static str],
) -> Setting {
    Setting {
        section,
        key,
        default,
        since,
        comment,
    }
}

pub const SETTINGS: &[Setting] = &[
    setting(
        "ui",
        "sql_position",
        "\"top\"",
        "0.3.0",
        &["Where the SQL editor sits: \"top\" or \"bottom\""],
    ),
    setting(
        "ui",
        "sql_always",
        "false",
        "0.3.0",
        &["Keep it on screen instead of only after pressing s"],
    ),
    setting(
        "ui",
        "sql_height",
        "0",
        "0.3.0",
        &["Rows it takes, 0 picks a third of the frame"],
    ),
    setting(
        "ui",
        "sql_editor",
        "\"simple\"",
        "0.6.0",
        &["How the SQL editor takes keys: \"simple\" (ctrl+r runs) or \"vim\" (:r runs)"],
    ),
    setting(
        "ui",
        "sql_complete",
        "true",
        "0.6.2",
        &["Offer tables, columns and keywords as you type, tab takes one"],
    ),
    setting(
        "ui",
        "row_style",
        "\"marker\"",
        "0.3.0",
        &["The current row: \"marker\", \"dim-others\", \"bold\", \"inverse\", \"underline\""],
    ),
    setting(
        "ui",
        "top_margin",
        "0",
        "0.3.0",
        &["Blank rows above the frame"],
    ),
    setting(
        "ui",
        "sidebar_width",
        "24",
        "0.3.0",
        &["Width of the table list"],
    ),
    setting(
        "ui",
        "time_zone",
        "\"UTC\"",
        "0.4.0",
        &["The zone ctrl+t and now() write in, such as America/Toronto"],
    ),
    setting(
        "ui",
        "modal_ring",
        "true",
        "0.4.0",
        &["Ring a modal with a border as well as the box itself"],
    ),
    setting(
        "ui",
        "inspect_related",
        "10",
        "0.3.0",
        &["Related rows to load into the inspector, 0 turns it off"],
    ),
    setting(
        "ui",
        "export_path",
        "\"\"",
        "0.3.0",
        &["Where exports go, empty uses the last folder you saved one in"],
    ),
    setting(
        "ui",
        "mouse",
        "true",
        "0.3.0",
        &["Click, drag and scroll inside tql"],
    ),
    setting(
        "ui",
        "hotkey_bar",
        "true",
        "0.6.1",
        &["The row of keys under the frame, false gives the row to the frame"],
    ),
    setting(
        "ui",
        "status_seconds",
        "4",
        "0.5.0",
        &["Seconds a status message stays before it fades, 0 keeps it"],
    ),
    setting(
        "ui",
        "double_click_ms",
        "400",
        "0.3.0",
        &["How close two clicks must be to open the editor"],
    ),
    setting(
        "ui",
        "mouse_row_offset",
        "0",
        "0.3.0",
        &["Subtract from reported mouse rows, 1 inside a multiplexer"],
    ),
    setting(
        "ui",
        "mouse_column_offset",
        "0",
        "0.3.0",
        &["Subtract from reported mouse columns"],
    ),
    setting(
        "theme",
        "border",
        "\"dim\"",
        "0.3.0",
        &["A pane border that is not focused"],
    ),
    setting(
        "theme",
        "focus_border",
        "\"cyan\"",
        "0.3.0",
        &["The border of the pane you are in"],
    ),
    setting("theme", "focus_title", "\"cyan\"", "0.3.0", &["Its title"]),
    setting(
        "theme",
        "cursor",
        "\"default\"",
        "0.3.0",
        &["The block you are on, \"default\" swaps the terminal's own colors"],
    ),
    setting(
        "theme",
        "selection",
        "\"default\"",
        "0.3.0",
        &["Highlighted but not where you are: the selected table, a marked row"],
    ),
    setting(
        "theme",
        "edited",
        "\"yellow\"",
        "0.3.0",
        &["A row you have changed, before :w"],
    ),
    setting(
        "theme",
        "added",
        "\"green\"",
        "0.4.0",
        &["A row you have added, before :w"],
    ),
    setting(
        "theme",
        "deleted",
        "\"red\"",
        "0.3.0",
        &["A row marked for deletion, before :w"],
    ),
    setting(
        "theme",
        "modal_border",
        "\"gray\"",
        "0.3.0",
        &["A modal border, and the ring around it"],
    ),
    setting(
        "theme",
        "modal_focus_border",
        "\"cyan\"",
        "0.3.0",
        &["The same, focused"],
    ),
    setting(
        "theme",
        "modal_title",
        "\"white\"",
        "0.3.0",
        &["A modal title"],
    ),
    setting(
        "theme",
        "modal_focus_title",
        "\"cyan\"",
        "0.3.0",
        &["The same, focused"],
    ),
    setting(
        "updates",
        "check",
        "true",
        "0.7.1",
        &["Look for a new release once a day"],
    ),
    setting(
        "updates",
        "automatic",
        "true",
        "0.7.1",
        &[
            "Install it by itself, checked against its checksum, for the next start; never for Homebrew or a package",
        ],
    ),
    setting(
        "ai",
        "provider",
        "\"auto\"",
        "0.3.0",
        &["\"auto\" takes the first provider you have a key for"],
    ),
    setting(
        "ai",
        "model",
        "\"\"",
        "0.3.0",
        &["Empty picks a default for that provider"],
    ),
    setting(
        "ai",
        "timeout",
        "60",
        "0.3.0",
        &["Seconds to wait for an answer"],
    ),
    setting(
        "ai",
        "url",
        "\"\"",
        "0.3.0",
        &["An OpenAI-compatible endpoint instead, such as LM Studio on http://localhost:1234/v1"],
    ),
    setting(
        "ai",
        "key",
        "\"\"",
        "0.3.0",
        &["Bearer token for that endpoint, if it wants one"],
    ),
    setting("icons", "mysql", "\"\u{e704}\"", "0.3.0", &[]),
    setting("icons", "pgsql", "\"\u{e76e}\"", "0.3.0", &[]),
    setting("icons", "sqlite", "\"\u{e7c4}\"", "0.3.0", &[]),
    setting("icons", "sqlsrv", "\"\u{f1c0}\"", "0.3.0", &[]),
    setting(
        "icons",
        "default",
        "\"\u{f1c0}\"",
        "0.3.0",
        &["Anything else"],
    ),
];

pub const NOT_YET: &[&str] = &["ai.provider", "ai.model", "ai.timeout", "ai.url", "ai.key"];

const NOTES: &[(&str, &str)] = &[
    (
        "theme",
        "Colors: dim, default, black, red, green, yellow, blue, magenta, cyan, white, gray",
    ),
    (
        "updates",
        "Once a day tql asks GitHub for the latest release. Nothing about you or your databases is sent.",
    ),
    (
        "ai",
        "What answers when you press a. Only table and column names are sent, never rows.",
    ),
    (
        "icons",
        "The glyph beside a connection name, by driver. Nerd Font devicons.",
    ),
];

pub fn sections() -> Vec<&'static str> {
    let mut sections: Vec<&str> = Vec::new();
    for setting in SETTINGS {
        if !sections.contains(&setting.section) {
            sections.push(setting.section);
        }
    }
    sections
}

pub fn render() -> String {
    let mut out = format!(
        "# tql configuration {VERSION}\n\
         # Every setting below is its default. Change what you like, delete what you do not.\n\
         # All of them, said properly:\n\
         # https://github.com/VheissuLabs/tql/blob/main/docs/configuration.md\n"
    );
    for section in sections() {
        out.push_str(&format!("\n[{section}]\n"));
        if let Some((_, note)) = NOTES.iter().find(|(noted, _)| *noted == section) {
            out.push_str(&format!("# {note}\n"));
        }
        for setting in SETTINGS.iter().filter(|setting| setting.section == section) {
            out.push('\n');
            out.push_str(&block(setting));
        }
    }
    out
}

pub fn block(setting: &Setting) -> String {
    let comments: String = setting
        .comment
        .iter()
        .map(|line| format!("# {line}\n"))
        .collect();
    format!("{comments}{} = {}\n", setting.key, setting.default)
}

pub fn section_of(key: &str) -> Option<&'static str> {
    SETTINGS
        .iter()
        .find(|setting| setting.key == key)
        .map(|setting| setting.section)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::Settings;

    #[test]
    fn writes_defaults_that_parse_as_valid_toml() {
        let table: toml::Table = toml::from_str(&render()).unwrap();

        assert_eq!(table["ui"]["sql_position"].as_str(), Some("top"));
        assert_eq!(table["icons"]["default"].as_str(), Some("\u{f1c0}"));
        assert_eq!(sections(), ["ui", "theme", "updates", "ai", "icons"]);
    }

    #[test]
    fn every_setting_is_read_or_listed_as_not_yet_ported() {
        let table: toml::Table = toml::from_str(&render()).unwrap();
        for setting in SETTINGS {
            let name = format!("{}.{}", setting.section, setting.key);
            let read = Settings::default().set(&name, &table[setting.section][setting.key]);
            assert_ne!(read, NOT_YET.contains(&name.as_str()), "{name}");
        }
    }

    #[test]
    fn the_defaults_are_the_ones_tql_rust_starts_with() {
        let written = Settings::from_table(&toml::from_str(&render()).unwrap());

        assert_eq!(written, Settings::default());
    }
}
