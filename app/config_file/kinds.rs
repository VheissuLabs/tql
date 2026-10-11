use super::template::Setting;
use crate::ai::providers::MODELS;
use crate::settings::COLORS;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Kind {
    Flag,
    Whole { least: i64, most: i64 },
    Seconds { most: f64 },
    Choice(&'static [&'static str]),
    Text,
    Secret,
}

const SQL_POSITIONS: &[&str] = &["top", "bottom"];
const SQL_EDITORS: &[&str] = &["simple", "vim"];
const ROW_STYLES: &[&str] = &["marker", "dim-others", "bold", "inverse", "underline"];

pub const PROVIDERS: [&str; 12] = [
    "auto",
    MODELS[0].0,
    MODELS[1].0,
    MODELS[2].0,
    MODELS[3].0,
    MODELS[4].0,
    MODELS[5].0,
    MODELS[6].0,
    MODELS[7].0,
    MODELS[8].0,
    MODELS[9].0,
    MODELS[10].0,
];

pub fn kind(setting: &Setting) -> Option<Kind> {
    Some(match (setting.section, setting.key) {
        ("ui", "sql_position") => Kind::Choice(SQL_POSITIONS),
        ("ui", "sql_editor") => Kind::Choice(SQL_EDITORS),
        ("ui", "row_style") => Kind::Choice(ROW_STYLES),
        ("ui", "top_margin" | "bottom_margin" | "left_margin" | "right_margin") => Kind::Whole {
            least: 0,
            most: 500,
        },
        ("ui", "sql_height") => Kind::Whole {
            least: 0,
            most: 500,
        },
        ("ui", "sidebar_width") => Kind::Whole {
            least: 8,
            most: 500,
        },
        ("ui", "inspect_related") => Kind::Whole {
            least: 0,
            most: 1000,
        },
        ("ui", "double_click_ms") => Kind::Whole {
            least: 0,
            most: 5000,
        },
        ("ui", "mouse_row_offset" | "mouse_column_offset") => Kind::Whole {
            least: -1000,
            most: 1000,
        },
        ("ui", "status_seconds") => Kind::Seconds { most: 86_400.0 },
        ("ui", "time_zone" | "export_path") => Kind::Text,
        (
            "ui",
            "sql_always"
            | "sql_complete"
            | "modal_ring"
            | "mouse"
            | "hotkey_bar"
            | "confirm_create_on_production",
        ) => Kind::Flag,
        ("theme", _) => Kind::Choice(&COLORS),
        ("updates", "check" | "automatic") => Kind::Flag,
        ("ai", "provider") => Kind::Choice(&PROVIDERS),
        ("ai", "model" | "url") => Kind::Text,
        ("ai", "timeout") => Kind::Whole {
            least: 1,
            most: 3600,
        },
        ("ai", "key") => Kind::Secret,
        ("mcp", "read_screen" | "read_production") => Kind::Flag,
        ("icons", _) => Kind::Text,
        _ => return None,
    })
}

pub fn applies_at_start(setting: &Setting) -> bool {
    matches!(
        (setting.section, setting.key),
        ("ui", "mouse") | ("updates", _)
    )
}

#[cfg(test)]
mod tests {
    use super::super::template::SETTINGS;
    use super::*;

    #[test]
    fn every_setting_says_how_it_is_edited() {
        for setting in SETTINGS {
            assert!(
                kind(setting).is_some(),
                "[{}] {} has no kind",
                setting.section,
                setting.key
            );
        }
    }

    #[test]
    fn every_default_is_one_of_its_own_choices_and_inside_its_range() {
        for setting in SETTINGS {
            let default: toml::Value = toml::from_str::<toml::Table>(&format!(
                "value = {}",
                setting.default
            ))
            .unwrap()["value"]
                .clone();
            match kind(setting).unwrap() {
                Kind::Flag => assert!(default.is_bool(), "{}", setting.key),
                Kind::Whole { least, most } => {
                    let number = default.as_integer().unwrap();
                    assert!((least..=most).contains(&number), "{}", setting.key);
                }
                Kind::Seconds { .. } => assert!(default.as_integer().is_some(), "{}", setting.key),
                Kind::Choice(choices) => {
                    assert!(
                        choices.contains(&default.as_str().unwrap()),
                        "{}",
                        setting.key
                    )
                }
                Kind::Text | Kind::Secret => assert!(default.is_str(), "{}", setting.key),
            }
        }
    }
}
