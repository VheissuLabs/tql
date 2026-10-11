use std::collections::BTreeMap;
use std::sync::LazyLock;

use regex::Regex;

use super::template::{self, SETTINGS};

static HEADER: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^\[([^\]]+)\]$").unwrap());
static ASSIGNMENT: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\s*([A-Za-z0-9_.-]+)\s*=").unwrap());
static HEADING: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^#\s*tql configuration").unwrap());

#[derive(Debug, Clone)]
struct Entry {
    key: String,
    text: String,
}

#[derive(Default)]
struct Section {
    name: String,
    above: Vec<String>,
    notes: Vec<String>,
    entries: Vec<Entry>,
}

#[derive(Default)]
struct File {
    preamble: Vec<String>,
    root: Vec<Entry>,
    sections: Vec<Section>,
}

impl File {
    fn section(&mut self, name: &str) -> &mut Section {
        let at = match self
            .sections
            .iter()
            .position(|section| section.name == name)
        {
            Some(at) => at,
            None => {
                self.sections.push(Section {
                    name: name.to_string(),
                    ..Section::default()
                });
                self.sections.len() - 1
            }
        };
        &mut self.sections[at]
    }

    fn set_aside(&mut self, current: Option<&str>, comments: Vec<String>) {
        match current {
            None => self.preamble.extend(comments),
            Some(name) => self.section(name).notes.extend(comments),
        }
    }

    fn keep(&mut self, open: Option<(Option<String>, Entry)>) {
        match open {
            Some((None, entry)) => self.root.push(entry),
            Some((Some(belongs), entry)) => self.section(&belongs).entries.push(entry),
            None => {}
        }
    }
}

pub fn apply(contents: &str) -> (String, Vec<String>) {
    let file = parse(contents);
    let order: Vec<String> = SETTINGS
        .iter()
        .map(|setting| format!("{}.{}", setting.section, setting.key))
        .collect();
    let mut moved = Vec::new();

    let mut out = format!("{}\n", file.preamble.join("\n").trim_end());
    for entry in &file.root {
        out.push('\n');
        out.push_str(&entry.text);
    }

    for section in ordered(&file.sections) {
        let mut known = BTreeMap::new();
        let mut unknown = Vec::new();
        for entry in &section.entries {
            let name = format!("{}.{}", section.name, entry.key);
            match order.iter().position(|known_name| *known_name == name) {
                Some(at) => {
                    known.insert(at, entry);
                }
                None => unknown.push(entry),
            }
        }
        let sorted: Vec<&Entry> = known.into_values().chain(unknown).collect();

        for (index, entry) in section.entries.iter().enumerate() {
            if sorted.get(index).map(|sorted| &sorted.key) != Some(&entry.key) {
                moved.push(entry.key.clone());
            }
        }

        out.push('\n');
        for line in &section.above {
            out.push_str(line);
            out.push('\n');
        }
        out.push_str(&format!("[{}]\n", section.name));
        for note in &section.notes {
            out.push_str(note);
            out.push('\n');
        }
        for entry in sorted {
            out.push('\n');
            out.push_str(&entry.text);
        }
    }

    (out, moved)
}

fn ordered(sections: &[Section]) -> Vec<&Section> {
    let template = template::sections();
    let known = template
        .iter()
        .filter_map(|name| sections.iter().find(|section| section.name == *name));
    let theirs = sections
        .iter()
        .filter(|section| !template.contains(&section.name.as_str()));
    known.chain(theirs).collect()
}

fn parse(contents: &str) -> File {
    let mut file = File::default();
    let mut current: Option<String> = None;
    let mut pending: Vec<String> = Vec::new();
    let mut open: Option<(Option<String>, Entry)> = None;

    for line in contents.split('\n') {
        let trimmed = line.trim();

        if let Some(header) = HEADER.captures(trimmed) {
            file.keep(open.take());
            let name = header[1].to_string();
            file.section(&name).above.append(&mut pending);
            current = Some(name);
            continue;
        }

        if trimmed.is_empty() {
            file.keep(open.take());
            let comments = std::mem::take(&mut pending);
            file.set_aside(current.as_deref(), comments);
            continue;
        }

        if trimmed.starts_with('#') {
            file.keep(open.take());
            if current.is_none() && HEADING.is_match(trimmed) {
                file.preamble.push(line.to_string());
            } else {
                pending.push(line.to_string());
            }
            continue;
        }

        let Some(assignment) = ASSIGNMENT.captures(line) else {
            if let Some((_, entry)) = open.as_mut() {
                entry.text.push_str(line.trim_end());
                entry.text.push('\n');
            }
            continue;
        };

        file.keep(open.take());
        let key = assignment[1].to_string();
        let text = pending
            .drain(..)
            .chain([line.trim_end().to_string()])
            .collect::<Vec<_>>()
            .join("\n")
            + "\n";
        let belongs = current
            .clone()
            .or_else(|| template::section_of(&key).map(str::to_string));
        open = Some((belongs, Entry { key, text }));
    }

    file.keep(open.take());
    file.set_aside(current.as_deref(), pending);
    file
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn puts_the_settings_back_in_template_order_keeping_the_values() {
        let scrambled = "# tql configuration 0.4.0\n\n[ai]\n\n# mine\nkey = \"secret\"\nprovider = \"auto\"\n\n[ui]\nsidebar_width = 40\n\n# Where the SQL editor sits: \"top\" or \"bottom\"\nsql_position = \"bottom\"";

        let (tidied, moved) = apply(scrambled);
        let at = |needle: &str| tidied.find(needle).unwrap();

        assert!(at("sql_position") < at("sidebar_width"));
        assert!(at("[ui]") < at("[ai]"));
        assert!(at("provider") < at("# mine\nkey = \"secret\""));
        assert_eq!(moved, ["sidebar_width", "sql_position", "key", "provider"]);
    }

    #[test]
    fn moves_a_setting_written_above_the_first_section_into_it() {
        let (tidied, _) =
            apply("# tql configuration 0.4.0\nmouse_row_offset = 1\n\n[ui]\nsidebar_width = 40\n");

        assert!(tidied.find("mouse_row_offset").unwrap() > tidied.find("[ui]").unwrap());
    }

    #[test]
    fn keeps_a_section_and_a_setting_it_has_never_heard_of() {
        let (tidied, moved) = apply(
            "# tql configuration 0.4.0\n\n[mine]\nwhatever = true\n\n[ui]\nmystery = 1\nsidebar_width = 40\n",
        );

        assert_eq!(
            tidied,
            "# tql configuration 0.4.0\n\n[ui]\n\nsidebar_width = 40\n\nmystery = 1\n\n[mine]\n\nwhatever = true\n"
        );
        assert_eq!(moved, ["mystery", "sidebar_width"]);
    }

    #[test]
    fn leaves_a_tidy_file_as_it_is() {
        let template = template::render();

        assert_eq!(apply(&template), (template, Vec::new()));
    }

    #[test]
    fn a_comment_above_a_section_header_stays_above_it() {
        let (tidied, _) =
            apply("# tql configuration 0.7.1\n\n# the keys I changed\n[keys]\nsql = \"S\"\n");

        assert_eq!(
            tidied,
            "# tql configuration 0.7.1\n\n# the keys I changed\n[keys]\n\nsql = \"S\"\n"
        );
    }

    #[test]
    fn a_value_written_over_several_lines_moves_whole() {
        let (tidied, _) = apply(
            "[keys]\nyank_value = [\n  \"y\",\n  \"ctrl+y\",\n]\n\n[ui]\nsidebar_width = 40\n",
        );

        assert_eq!(
            tidied,
            "\n\n[ui]\n\nsidebar_width = 40\n\n[keys]\n\nyank_value = [\n  \"y\",\n  \"ctrl+y\",\n]\n"
        );
        assert!(toml::from_str::<toml::Table>(&tidied).is_ok());
    }
}
