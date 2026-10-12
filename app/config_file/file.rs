use std::fs;
use std::path::Path;
use std::sync::LazyLock;

use regex::Regex;
use toml::{Table, Value};

use super::template::{self, SETTINGS, Setting, VERSION};

const SPACE: &str = r"[\t\n\x0B\x0C\r ]";

static WRITTEN_FOR: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(&format!(
        r"(?m)^#{SPACE}*tql configuration{SPACE}+([0-9]+\.[0-9]+\.[0-9]+)"
    ))
    .unwrap()
});
static STAMP: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(&format!(
        r"(?m)^#{SPACE}*tql configuration({SPACE}+[0-9.]+)?{SPACE}*$"
    ))
    .unwrap()
});

#[derive(Debug, PartialEq, Eq)]
pub struct Ensured {
    pub created: bool,
    pub added: Vec<String>,
}

pub fn ensure(path: &Path) -> std::io::Result<Ensured> {
    if let Some(directory) = path.parent().filter(|directory| !directory.is_dir()) {
        fs::create_dir_all(directory)?;
        restrict(directory, 0o700)?;
    }

    if !path.exists() {
        fs::write(path, template::render())?;
        restrict(path, 0o600)?;
        return Ok(Ensured {
            created: true,
            added: Vec::new(),
        });
    }

    let contents = fs::read_to_string(path)?;
    let (topped, added) = top_up(&contents);
    if topped != contents {
        fs::write(path, topped)?;
    }
    Ok(Ensured {
        created: false,
        added,
    })
}

#[cfg(unix)]
fn restrict(path: &Path, mode: u32) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(mode))
}

#[cfg(not(unix))]
fn restrict(_path: &Path, _mode: u32) -> std::io::Result<()> {
    Ok(())
}

pub fn top_up(contents: &str) -> (String, Vec<String>) {
    let since = written_for(contents);
    if since.as_deref().is_some_and(|since| !newer(VERSION, since)) {
        return (contents.to_string(), Vec::new());
    }
    let mut contents = contents.to_string();
    let mut added = Vec::new();

    for setting in SETTINGS {
        if has(&contents, setting.key) {
            continue;
        }
        if since
            .as_deref()
            .is_some_and(|since| !newer(setting.since, since))
        {
            continue;
        }
        contents = insert(&contents, setting);
        added.push(setting.key.to_string());
    }

    (stamp(&contents), added)
}

pub fn written_for(contents: &str) -> Option<String> {
    WRITTEN_FOR
        .captures(contents)
        .map(|captures| captures[1].to_string())
}

fn newer(version: &str, than: &str) -> bool {
    let parts = |version: &str| -> Vec<u64> {
        version
            .split('.')
            .map(|part| part.parse().unwrap_or(0))
            .collect()
    };
    parts(version) > parts(than)
}

fn stamp(contents: &str) -> String {
    let line = format!("# tql configuration {VERSION}");
    if !STAMP.is_match(contents) {
        return stamp(&format!("{line}\n{contents}"));
    }
    let stamped = STAMP.replace(contents, line.as_str()).into_owned();
    if stamped == contents {
        return stamped;
    }
    stamp(&stamped)
}

fn assigns(key: &str) -> Regex {
    Regex::new(&format!(r"^{SPACE}*{}{SPACE}*=", regex::escape(key))).unwrap()
}

fn has(contents: &str, key: &str) -> bool {
    Regex::new(&format!(r"(?m)^{SPACE}*{}{SPACE}*=", regex::escape(key)))
        .unwrap()
        .is_match(contents)
}

fn insert(contents: &str, setting: &Setting) -> String {
    let mut lines: Vec<String> = contents.split('\n').map(str::to_string).collect();
    let header = format!("[{}]", setting.section);
    let block = template::block(setting);

    let Some(header_at) = lines.iter().position(|line| line.trim() == header) else {
        return format!("{}\n\n{header}\n\n{block}", contents.trim_end());
    };

    let at = after_previous(&lines, setting, header_at)
        .unwrap_or_else(|| after_header(&lines, header_at));
    let inserted = std::iter::once(String::new())
        .chain(block.trim_end_matches('\n').split('\n').map(str::to_string));
    lines.splice(at..at, inserted);
    lines.join("\n")
}

pub fn set(contents: &str, setting: &Setting, literal: &str) -> String {
    let mut lines: Vec<String> = contents.split('\n').map(str::to_string).collect();
    let assignment = assigns(setting.key);
    let header = format!("[{}]", setting.section);
    let first_header = lines
        .iter()
        .position(|line| line.trim_start().starts_with('['))
        .unwrap_or(lines.len());
    let in_section = lines
        .iter()
        .position(|line| line.trim() == header)
        .and_then(|header_at| {
            lines
                .iter()
                .enumerate()
                .skip(header_at + 1)
                .take_while(|(_, line)| !line.trim_start().starts_with('['))
                .find(|(_, line)| assignment.is_match(line))
                .map(|(index, _)| index)
        });
    let at_the_top = || {
        lines[..first_header]
            .iter()
            .position(|line| assignment.is_match(line))
    };
    match in_section.or_else(at_the_top) {
        Some(at) => {
            let indent: String = lines[at]
                .chars()
                .take_while(|character| character.is_whitespace())
                .collect();
            lines[at] = format!("{indent}{} = {literal}", setting.key);
            lines.join("\n")
        }
        None => set(&insert(contents, setting), setting, literal),
    }
}

pub fn write(path: &Path, setting: &Setting, literal: &str) -> std::io::Result<()> {
    if !path.exists() {
        ensure(path)?;
    }
    let contents = fs::read_to_string(path)?;
    fs::write(path, set(&contents, setting, literal))
}

fn after_previous(lines: &[String], setting: &Setting, header_at: usize) -> Option<usize> {
    SETTINGS
        .iter()
        .take_while(|candidate| candidate.key != setting.key)
        .filter(|candidate| candidate.section == setting.section)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .find_map(|candidate| {
            let assignment = assigns(candidate.key);
            lines
                .iter()
                .enumerate()
                .skip(header_at + 1)
                .find(|(_, line)| assignment.is_match(line))
                .map(|(index, _)| index + 1)
        })
}

fn after_header(lines: &[String], header_at: usize) -> usize {
    lines
        .iter()
        .enumerate()
        .skip(header_at + 1)
        .find(|(_, line)| !line.trim().starts_with('#'))
        .map(|(index, _)| index)
        .unwrap_or(lines.len())
}

pub fn hoist(mut user: Table) -> (Table, Vec<String>) {
    let mut moved = Vec::new();
    for setting in SETTINGS {
        let Some(value) = user.remove(setting.key) else {
            continue;
        };
        let section = user
            .entry(setting.section)
            .or_insert_with(|| Value::Table(Table::new()));
        let Value::Table(section) = section else {
            continue;
        };
        if !section.contains_key(setting.key) {
            section.insert(setting.key.to_string(), value);
            moved.push(setting.key.to_string());
        }
    }
    (user, moved)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn decoded(contents: &str) -> Table {
        toml::from_str(contents).unwrap()
    }

    #[test]
    fn adds_only_the_settings_that_are_missing_and_never_changes_a_value() {
        let (topped, added) = top_up("[ui]\nsql_position = \"bottom\"\ntop_margin = 7");

        assert!(added.contains(&"sql_always".to_string()));
        assert!(!added.contains(&"sql_position".to_string()));
        let table = decoded(&topped);
        assert_eq!(table["ui"]["sql_position"].as_str(), Some("bottom"));
        assert_eq!(table["ui"]["top_margin"].as_integer(), Some(7));
        assert!(
            table["ui"]
                .as_table()
                .unwrap()
                .contains_key("sidebar_width")
        );
    }

    #[test]
    fn does_nothing_when_every_setting_is_already_there() {
        let template = template::render();

        assert_eq!(top_up(&template), (template, Vec::new()));
    }

    #[test]
    fn keeps_the_comments_the_user_wrote_and_creates_a_missing_section() {
        let (topped, _) = top_up("# my own note\n");

        assert!(topped.contains("# my own note"));
        assert!(decoded(&topped).contains_key("ui"));
    }

    #[test]
    fn puts_a_new_setting_after_the_one_it_follows_in_the_template() {
        let (topped, added) = top_up(
            "# tql configuration 0.6.0\n\n[ui]\n# mine\nsql_editor = \"simple\"\n\nrow_style = \"bold\"\n",
        );

        assert_eq!(
            added,
            [
                "sql_complete",
                "sql_keyword_case",
                "bottom_margin",
                "left_margin",
                "right_margin",
                "hotkey_bar",
                "confirm_create_on_production",
                "check",
                "automatic",
                "read_screen",
                "read_production"
            ]
        );
        assert!(topped.starts_with(
            "# tql configuration 0.14.0\n[ui]\n# mine\nsql_editor = \"simple\"\n\n# Offer tables"
        ));
    }

    #[test]
    fn only_adds_settings_newer_than_the_file_says_it_is() {
        let file = format!("# tql configuration {VERSION}\n\n[ui]\nsidebar_width = 40\n");

        assert_eq!(top_up(&file), (file, Vec::new()));
    }

    #[test]
    fn offers_a_setting_from_a_newer_version_once() {
        let (topped, added) = top_up("# tql configuration 0.6.2\n\n[ui]\nsidebar_width = 40\n");

        assert_eq!(
            added,
            [
                "sql_keyword_case",
                "bottom_margin",
                "left_margin",
                "right_margin",
                "confirm_create_on_production",
                "check",
                "automatic",
                "read_screen",
                "read_production"
            ]
        );
        assert!(topped.starts_with(&format!("# tql configuration {VERSION}\n[ui]")));

        let deleted = topped.replace("automatic = true", "");
        assert_eq!(top_up(&deleted), (deleted, Vec::new()));
    }

    #[test]
    fn leaves_a_file_at_this_version_or_newer_exactly_as_it_is() {
        for file in [
            "# tql configuration 0.14.0\n\n\n[ui]\nsidebar_width = 40\n",
            "#tql configuration 0.14.0   \r\n[ui]\r\nsidebar_width = 40\r\n",
            "# tql configuration 0.15.0\n[ui]\nsidebar_width = 40\n",
            "# tql configuration 1.0.0\n",
        ] {
            assert_eq!(top_up(file), (file.to_string(), Vec::new()), "{file:?}");
        }
    }

    #[test]
    fn stamps_an_older_file_until_tql_would_leave_it_alone() {
        let (topped, added) = top_up("# tql configuration 0.6.2\n\n\n[ui]\nsidebar_width = 40\n");

        assert_eq!(
            added,
            [
                "sql_keyword_case",
                "bottom_margin",
                "left_margin",
                "right_margin",
                "confirm_create_on_production",
                "check",
                "automatic",
                "read_screen",
                "read_production"
            ]
        );
        assert!(topped.starts_with("# tql configuration 0.14.0\n[ui]\n"));
        assert_eq!(stamp(&topped), topped);
    }

    #[test]
    fn offers_everything_once_to_a_file_with_no_version_in_it() {
        let (topped, added) = top_up("[ui]\nsidebar_width = 40\n");

        assert!(added.contains(&"sql_position".to_string()));
        assert!(topped.starts_with(&format!("# tql configuration {VERSION}\n")));
    }

    #[test]
    fn reads_the_version_a_file_was_written_for() {
        assert_eq!(
            written_for("# tql configuration 0.4.0\n").as_deref(),
            Some("0.4.0")
        );
        assert_eq!(written_for("[ui]\n"), None);
        assert!(newer("0.10.0", "0.9.9") && !newer("0.7.1", "0.7.1"));
    }

    #[test]
    fn reads_settings_written_above_the_first_section_header() {
        let (hoisted, moved) = hoist(decoded(
            "mouse_row_offset = 1\nsidebar_width = 40\n[ui]\nsql_position = \"bottom\"\n",
        ));

        assert_eq!(
            hoisted,
            decoded("[ui]\nsql_position = \"bottom\"\nsidebar_width = 40\nmouse_row_offset = 1\n")
        );
        assert_eq!(moved, ["sidebar_width", "mouse_row_offset"]);
    }

    #[test]
    fn prefers_the_sectioned_value_and_leaves_an_unknown_key_where_it_is() {
        let (hoisted, moved) = hoist(decoded(
            "mouse_row_offset = 1\nsomething_else = 1\n[ui]\nmouse_row_offset = 3\n",
        ));

        assert_eq!(
            hoisted,
            decoded("something_else = 1\n[ui]\nmouse_row_offset = 3\n")
        );
        assert!(moved.is_empty());
    }

    fn named(key: &str) -> &'static Setting {
        SETTINGS.iter().find(|setting| setting.key == key).unwrap()
    }

    #[test]
    fn sets_a_value_in_place_and_keeps_every_comment_and_the_order() {
        let contents = "# mine\n[ui]\n# how much room\n  sidebar_width = 24 \nsql_height = 0\n\n[theme]\nborder = \"dim\"\n";

        let changed = set(contents, named("sidebar_width"), "40");

        assert_eq!(
            changed,
            "# mine\n[ui]\n# how much room\n  sidebar_width = 40\nsql_height = 0\n\n[theme]\nborder = \"dim\"\n"
        );
    }

    #[test]
    fn sets_the_value_where_it_sits_above_the_sections() {
        let contents = "mouse_row_offset = 1\n\n[ui]\nsidebar_width = 24\n";

        assert_eq!(
            set(contents, named("mouse_row_offset"), "0"),
            "mouse_row_offset = 0\n\n[ui]\nsidebar_width = 24\n"
        );
    }

    #[test]
    fn prefers_the_sectioned_value_and_never_touches_a_same_named_key_elsewhere() {
        let contents = "[ui]\nmouse_row_offset = 2\n\n[keys]\nmouse_row_offset = \"x\"\n";

        assert_eq!(
            set(contents, named("mouse_row_offset"), "0"),
            "[ui]\nmouse_row_offset = 0\n\n[keys]\nmouse_row_offset = \"x\"\n"
        );
    }

    #[test]
    fn adds_a_missing_setting_to_its_section_with_its_comment() {
        let changed = set("[ui]\nsidebar_width = 24\n", named("sql_height"), "12");

        assert!(changed.contains("sql_height = 12"), "{changed}");
        assert!(changed.contains("# Rows it takes"), "{changed}");
        let table: Table = toml::from_str(&changed).unwrap();
        assert_eq!(table["ui"]["sql_height"].as_integer(), Some(12));
        assert_eq!(table["ui"]["sidebar_width"].as_integer(), Some(24));
    }

    #[test]
    fn adds_a_missing_section_too() {
        let changed = set("[ui]\nsidebar_width = 24\n", named("read_screen"), "true");

        let table: Table = toml::from_str(&changed).unwrap();
        assert_eq!(table["mcp"]["read_screen"].as_bool(), Some(true));
    }

    #[test]
    fn writes_the_template_on_first_run_and_tops_it_up_after() {
        let directory =
            std::env::temp_dir().join(format!("tql-config-file-{}", std::process::id()));
        let _ = fs::remove_dir_all(&directory);
        let path = directory.join("tql/config.toml");

        assert_eq!(
            ensure(&path).unwrap(),
            Ensured {
                created: true,
                added: Vec::new()
            }
        );
        assert_eq!(fs::read_to_string(&path).unwrap(), template::render());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = fs::metadata(&path).unwrap().permissions().mode();
            assert_eq!(mode & 0o777, 0o600);
        }

        assert_eq!(
            ensure(&path).unwrap(),
            Ensured {
                created: false,
                added: Vec::new()
            }
        );

        fs::write(&path, "[ui]\nsidebar_width = 40\n").unwrap();
        assert!(!ensure(&path).unwrap().added.is_empty());
        assert!(fs::read_to_string(&path).unwrap().contains("sql_position"));
        fs::remove_dir_all(&directory).unwrap();
    }
}
