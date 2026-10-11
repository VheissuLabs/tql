use std::cell::RefCell;
use std::collections::HashMap;
use toml::{Table, Value};

use crate::tui::Keymap;

pub const COLORS: [&str; 11] = [
    "dim", "default", "black", "red", "green", "yellow", "blue", "magenta", "cyan", "white", "gray",
];

#[derive(Debug, Clone, PartialEq)]
pub struct Settings {
    pub modal_ring: bool,
    pub confirm_create_on_production: bool,
    pub mcp_read_screen: bool,
    pub mcp_read_production: bool,
    pub hotkey_bar: bool,
    pub top_margin: u16,
    pub bottom_margin: u16,
    pub left_margin: u16,
    pub right_margin: u16,
    pub pane_gap: u16,
    pub status_seconds: f64,
    pub sidebar_width: usize,
    pub sql_at_bottom: bool,
    pub sql_always: bool,
    pub sql_height: usize,
    pub sql_complete: bool,
    pub sql_vim: bool,
    pub row_style: String,
    pub mouse: bool,
    pub double_click_ms: u64,
    pub mouse_row_offset: i64,
    pub mouse_column_offset: i64,
    pub time_zone: String,
    pub inspect_related: usize,
    pub keymap: Keymap,
    pub export_path: String,
    pub update_check: bool,
    pub update_automatic: bool,
    pub ai: Ai,
    theme: HashMap<String, String>,
    icons: HashMap<String, String>,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            modal_ring: true,
            confirm_create_on_production: true,
            mcp_read_screen: false,
            mcp_read_production: false,
            hotkey_bar: true,
            top_margin: 0,
            bottom_margin: 0,
            left_margin: 0,
            right_margin: 0,
            pane_gap: 0,
            status_seconds: 4.0,
            sidebar_width: 24,
            sql_at_bottom: false,
            sql_always: false,
            sql_height: 0,
            sql_complete: true,
            sql_vim: false,
            row_style: "marker".to_string(),
            mouse: true,
            double_click_ms: 400,
            mouse_row_offset: 0,
            mouse_column_offset: 0,
            time_zone: "UTC".to_string(),
            inspect_related: 10,
            keymap: Keymap::default(),
            export_path: String::new(),
            update_check: true,
            update_automatic: true,
            ai: Ai::default(),
            theme: [
                ("border", "dim"),
                ("focus_border", "cyan"),
                ("focus_title", "cyan"),
                ("modal_border", "gray"),
                ("modal_focus_border", "cyan"),
                ("modal_title", "white"),
                ("modal_focus_title", "cyan"),
                ("cursor", "default"),
                ("selection", "default"),
                ("deleted", "red"),
                ("edited", "yellow"),
                ("added", "green"),
            ]
            .into_iter()
            .map(|(key, color)| (key.to_string(), color.to_string()))
            .collect(),
            icons: [
                ("mysql", "\u{e704}"),
                ("pgsql", "\u{e76e}"),
                ("sqlite", "\u{e7c4}"),
                ("sqlsrv", "\u{f1c0}"),
                ("default", "\u{f1c0}"),
            ]
            .into_iter()
            .map(|(driver, icon)| (driver.to_string(), icon.to_string()))
            .collect(),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Ai {
    pub provider: String,
    pub model: String,
    pub timeout: u64,
    pub url: String,
    pub key: String,
}

impl Default for Ai {
    fn default() -> Self {
        Ai {
            provider: "auto".to_string(),
            model: String::new(),
            timeout: 60,
            url: String::new(),
            key: String::new(),
        }
    }
}

const AI_ENVIRONMENT: [(&str, &str); 5] = [
    ("TQL_AI_PROVIDER", "ai.provider"),
    ("TQL_AI_MODEL", "ai.model"),
    ("TQL_AI_TIMEOUT", "ai.timeout"),
    ("TQL_AI_URL", "ai.url"),
    ("TQL_AI_KEY", "ai.key"),
];

const ENVIRONMENT: [(&str, &str); 5] = [
    ("TQL_MOUSE", "ui.mouse"),
    ("TQL_DOUBLE_CLICK_MS", "ui.double_click_ms"),
    ("TQL_INSPECT_RELATED", "ui.inspect_related"),
    ("TQL_MOUSE_ROW_OFFSET", "ui.mouse_row_offset"),
    ("TQL_MOUSE_COLUMN_OFFSET", "ui.mouse_column_offset"),
];

fn from_environment(value: &str) -> Value {
    match value.trim().to_lowercase().as_str() {
        "true" | "(true)" => Value::Boolean(true),
        "false" | "(false)" => Value::Boolean(false),
        number => number
            .parse()
            .map(Value::Integer)
            .unwrap_or_else(|_| Value::String(value.to_string())),
    }
}

fn scalar(value: &Value) -> String {
    match value {
        Value::String(text) => text.trim().to_string(),
        other => other.to_string(),
    }
}

impl Settings {
    pub fn from_table(table: &Table) -> Self {
        Self::from_sources(table, |name| std::env::var(name).ok())
    }

    pub fn from_sources(table: &Table, environment: impl Fn(&str) -> Option<String>) -> Self {
        let mut settings = Settings::default();
        for (variable, key) in ENVIRONMENT {
            if let Some(value) = environment(variable) {
                settings.set(key, &from_environment(&value));
            }
        }
        for (section, values) in table {
            let Value::Table(values) = values else {
                continue;
            };
            for (key, value) in values {
                settings.set(&format!("{section}.{key}"), value);
            }
        }
        for (variable, key) in AI_ENVIRONMENT {
            if let Some(value) = environment(variable).filter(|value| !value.trim().is_empty()) {
                settings.set(key, &from_environment(&value));
            }
        }
        if environment("NO_MOUSE").is_some_and(|value| !value.is_empty()) {
            settings.mouse = false;
        }
        settings
    }

    pub fn set(&mut self, key: &str, value: &Value) -> bool {
        let flag = || value.as_bool().unwrap_or_default();
        let number = || {
            value
                .as_integer()
                .map(|number| number.max(0) as usize)
                .unwrap_or_default()
        };
        let text = || value.as_str().unwrap_or_default().to_string();
        let offset = || {
            value
                .as_integer()
                .unwrap_or_default()
                .clamp(-MOUSE_OFFSET_MOST, MOUSE_OFFSET_MOST)
        };

        match key {
            "ui.modal_ring" => self.modal_ring = flag(),
            "ui.confirm_create_on_production" => self.confirm_create_on_production = flag(),
            "mcp.read_screen" => self.mcp_read_screen = flag(),
            "mcp.read_production" => self.mcp_read_production = flag(),
            "ui.hotkey_bar" => self.hotkey_bar = flag(),
            "ui.top_margin" => self.top_margin = number().min(u16::MAX as usize) as u16,
            "ui.bottom_margin" => self.bottom_margin = number().min(u16::MAX as usize) as u16,
            "ui.left_margin" => self.left_margin = number().min(u16::MAX as usize) as u16,
            "ui.right_margin" => self.right_margin = number().min(u16::MAX as usize) as u16,
            "ui.status_seconds" => {
                self.status_seconds = value
                    .as_float()
                    .or_else(|| value.as_integer().map(|seconds| seconds as f64))
                    .filter(|seconds| !seconds.is_nan())
                    .unwrap_or(4.0)
                    .clamp(0.0, STATUS_SECONDS_MOST);
            }
            "ui.sidebar_width" => self.sidebar_width = number().max(8),
            "ui.sql_position" => self.sql_at_bottom = text() == "bottom",
            "ui.sql_always" => self.sql_always = flag(),
            "ui.sql_height" => self.sql_height = number(),
            "ui.sql_complete" => self.sql_complete = flag(),
            "ui.sql_editor" => self.sql_vim = text().to_lowercase() == "vim",
            "ui.row_style" => self.row_style = text(),
            "ui.mouse" => self.mouse = flag(),
            "ui.double_click_ms" => self.double_click_ms = number() as u64,
            "ui.mouse_row_offset" => self.mouse_row_offset = offset(),
            "ui.mouse_column_offset" => self.mouse_column_offset = offset(),
            "ui.time_zone" => self.time_zone = text(),
            "ui.inspect_related" => self.inspect_related = number(),
            "ui.export_path" => self.export_path = text(),
            "updates.check" => self.update_check = flag(),
            "updates.automatic" => self.update_automatic = flag(),
            "ai.provider" => self.ai.provider = scalar(value),
            "ai.model" => self.ai.model = scalar(value),
            "ai.timeout" => {
                self.ai.timeout = value
                    .as_integer()
                    .or_else(|| value.as_float().map(|seconds| seconds as i64))
                    .or_else(|| {
                        value
                            .as_str()
                            .and_then(|seconds| seconds.trim().parse().ok())
                    })
                    .unwrap_or(60)
                    .max(1) as u64;
            }
            "ai.url" => self.ai.url = scalar(value),
            "ai.key" => self.ai.key = scalar(value),
            _ => {
                if let Some(name) = key.strip_prefix("theme.") {
                    self.theme.insert(name.to_string(), text());
                } else if let Some(driver) = key.strip_prefix("icons.") {
                    self.icons.insert(driver.to_string(), text());
                } else if let Some(action) = key.strip_prefix("keys.") {
                    return self.keymap.rebind(action, value);
                } else {
                    return false;
                }
            }
        }
        true
    }

    pub fn color(&self, key: &str, fallback: &str) -> String {
        self.theme
            .get(key)
            .filter(|color| COLORS.contains(&color.as_str()))
            .cloned()
            .unwrap_or_else(|| fallback.to_string())
    }

    pub fn icon(&self, driver: &str) -> String {
        self.icons
            .get(driver)
            .or_else(|| self.icons.get("default"))
            .cloned()
            .unwrap_or_else(|| "•".to_string())
    }

    pub fn sql_height(&self, frame_height: usize) -> usize {
        if self.sql_height > 0 {
            return self.sql_height.min(frame_height.saturating_sub(5)).max(3);
        }
        (frame_height / 3).clamp(5, 10)
    }
}

const STATUS_SECONDS_MOST: f64 = 86_400.0;
const MOUSE_OFFSET_MOST: i64 = 1_000;

thread_local! {
    static CURRENT: RefCell<Settings> = RefCell::new(Settings::default());
}

pub fn install(settings: Settings) {
    CURRENT.with(|current| *current.borrow_mut() = settings);
}

pub fn with<Answer>(read: impl FnOnce(&Settings) -> Answer) -> Answer {
    CURRENT.with(|current| read(&current.borrow()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_mouse_offsets_to_a_sensible_size() {
        let table: Table = toml::from_str(
            "[ui]\nmouse_row_offset = -9223372036854775808\nmouse_column_offset = 5\n",
        )
        .unwrap();

        let settings = Settings::from_table(&table);

        assert_eq!(settings.mouse_row_offset, -1_000);
        assert_eq!(settings.mouse_column_offset, 5);
    }

    #[test]
    fn reads_tqls_settings_and_ignores_unknown_colors() {
        let table: Table = toml::from_str(
            "[ui]\nsql_always = true\nsql_position = \"bottom\"\nmodal_ring = false\nsidebar_width = 30\n\
             [theme]\nselection = \"cyan\"\ngrid = \"inherit\"\nborder = \"rainbow\"\n[ai]\nkey = \"never read\"\n",
        )
        .unwrap();

        let settings = Settings::from_table(&table);

        assert!(settings.sql_always && settings.sql_at_bottom && !settings.modal_ring);
        assert_eq!(settings.sidebar_width, 30);
        assert_eq!(settings.color("selection", "default"), "cyan");
        assert_eq!(settings.color("border", "dim"), "dim");
    }

    #[test]
    fn reads_whether_to_check_for_updates_and_install_them() {
        let defaults = Settings::default();
        let turned_off = Settings::from_table(
            &toml::from_str("[updates]\ncheck = false\nautomatic = false\n").unwrap(),
        );

        assert!(defaults.update_check && defaults.update_automatic);
        assert!(!turned_off.update_check && !turned_off.update_automatic);
    }

    #[test]
    fn reads_the_blank_rows_above_the_frame() {
        let settings = Settings::from_table(&toml::from_str("[ui]\ntop_margin = 2\n").unwrap());

        assert_eq!(settings.top_margin, 2);
    }

    #[test]
    fn takes_defaults_from_the_environment_and_the_config_file_over_them() {
        let environment = |name: &str| match name {
            "TQL_MOUSE" => Some("false".to_string()),
            "TQL_DOUBLE_CLICK_MS" => Some("250".to_string()),
            "TQL_INSPECT_RELATED" => Some("3".to_string()),
            "TQL_MOUSE_ROW_OFFSET" => Some("-1".to_string()),
            "TQL_MOUSE_COLUMN_OFFSET" => Some("2".to_string()),
            _ => None,
        };

        let from_environment = Settings::from_sources(&Table::new(), environment);
        let file_wins = Settings::from_sources(
            &toml::from_str("[ui]\nmouse = true\ninspect_related = 7\n").unwrap(),
            environment,
        );

        assert!(!from_environment.mouse);
        assert_eq!(from_environment.double_click_ms, 250);
        assert_eq!(from_environment.inspect_related, 3);
        assert_eq!(from_environment.mouse_row_offset, -1);
        assert_eq!(from_environment.mouse_column_offset, 2);
        assert!(file_wins.mouse);
        assert_eq!(file_wins.inspect_related, 7);
        assert_eq!(file_wins.double_click_ms, 250);
    }

    #[test]
    fn no_mouse_in_the_environment_turns_the_mouse_off_whatever_the_file_says() {
        let settings =
            Settings::from_sources(&toml::from_str("[ui]\nmouse = true\n").unwrap(), |name| {
                (name == "NO_MOUSE").then(|| "1".to_string())
            });

        assert!(!settings.mouse);
    }

    #[test]
    fn sql_height_follows_tqls_rule() {
        let settings = Settings::default();
        assert_eq!(settings.sql_height(26), 8);
        assert_eq!(settings.sql_height(9), 5);
        assert_eq!(settings.sql_height(60), 10);
    }
}
