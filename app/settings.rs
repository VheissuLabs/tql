use std::cell::RefCell;
use std::collections::HashMap;
use std::fs;
use std::path::Path;

use anyhow::Context;
use toml::{Table, Value};

const COLORS: [&str; 11] = [
    "dim", "default", "black", "red", "green", "yellow", "blue", "magenta", "cyan", "white", "gray",
];

#[derive(Debug, Clone)]
pub struct Settings {
    pub modal_ring: bool,
    pub hotkey_bar: bool,
    pub status_seconds: f64,
    pub sidebar_width: usize,
    pub sql_at_bottom: bool,
    pub sql_always: bool,
    pub sql_height: usize,
    pub sql_complete: bool,
    pub row_style: String,
    pub mouse: bool,
    pub double_click_ms: u64,
    pub mouse_row_offset: i64,
    pub mouse_column_offset: i64,
    pub time_zone: String,
    theme: HashMap<String, String>,
    icons: HashMap<String, String>,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            modal_ring: true,
            hotkey_bar: true,
            status_seconds: 4.0,
            sidebar_width: 24,
            sql_at_bottom: false,
            sql_always: false,
            sql_height: 0,
            sql_complete: true,
            row_style: "marker".to_string(),
            mouse: true,
            double_click_ms: 400,
            mouse_row_offset: 0,
            mouse_column_offset: 0,
            time_zone: "UTC".to_string(),
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

impl Settings {
    pub fn load(path: &Path) -> anyhow::Result<Self> {
        let mut settings = Settings::default();
        if !path.is_file() {
            return Ok(settings);
        }

        let source =
            fs::read_to_string(path).with_context(|| format!("cannot read {}", path.display()))?;
        let table: Table = toml::from_str(&source)
            .with_context(|| format!("{} is not valid TOML", path.display()))?;
        for (section, values) in &table {
            let Value::Table(values) = values else {
                continue;
            };
            for (key, value) in values {
                settings.set(&format!("{section}.{key}"), value);
            }
        }
        Ok(settings)
    }

    pub fn set(&mut self, key: &str, value: &Value) {
        let flag = || value.as_bool().unwrap_or_default();
        let number = || {
            value
                .as_integer()
                .map(|number| number.max(0) as usize)
                .unwrap_or_default()
        };
        let text = || value.as_str().unwrap_or_default().to_string();

        match key {
            "ui.modal_ring" => self.modal_ring = flag(),
            "ui.hotkey_bar" => self.hotkey_bar = flag(),
            "ui.status_seconds" => {
                self.status_seconds = value
                    .as_float()
                    .or_else(|| value.as_integer().map(|seconds| seconds as f64))
                    .unwrap_or(4.0)
                    .max(0.0);
            }
            "ui.sidebar_width" => self.sidebar_width = number().max(8),
            "ui.sql_position" => self.sql_at_bottom = text() == "bottom",
            "ui.sql_always" => self.sql_always = flag(),
            "ui.sql_height" => self.sql_height = number(),
            "ui.sql_complete" => self.sql_complete = flag(),
            "ui.row_style" => self.row_style = text(),
            "ui.mouse" => self.mouse = flag(),
            "ui.double_click_ms" => self.double_click_ms = number() as u64,
            "ui.mouse_row_offset" => self.mouse_row_offset = value.as_integer().unwrap_or_default(),
            "ui.mouse_column_offset" => {
                self.mouse_column_offset = value.as_integer().unwrap_or_default()
            }
            "ui.time_zone" => self.time_zone = text(),
            _ => {
                if let Some(name) = key.strip_prefix("theme.") {
                    self.theme.insert(name.to_string(), text());
                } else if let Some(driver) = key.strip_prefix("icons.") {
                    self.icons.insert(driver.to_string(), text());
                }
            }
        }
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
    fn reads_tqls_config_file_and_ignores_unknown_colors() {
        let path = std::env::temp_dir().join(format!("tql-settings-{}.toml", std::process::id()));
        fs::write(
            &path,
            "[ui]\nsql_always = true\nsql_position = \"bottom\"\nmodal_ring = false\nsidebar_width = 30\n\
             [theme]\nselection = \"cyan\"\ngrid = \"inherit\"\nborder = \"rainbow\"\n[ai]\nkey = \"never read\"\n",
        )
        .unwrap();

        let settings = Settings::load(&path).unwrap();
        fs::remove_file(&path).unwrap();

        assert!(settings.sql_always && settings.sql_at_bottom && !settings.modal_ring);
        assert_eq!(settings.sidebar_width, 30);
        assert_eq!(settings.color("selection", "default"), "cyan");
        assert_eq!(settings.color("border", "dim"), "dim");
    }

    #[test]
    fn sql_height_follows_tqls_rule() {
        let settings = Settings::default();
        assert_eq!(settings.sql_height(26), 8);
        assert_eq!(settings.sql_height(9), 5);
        assert_eq!(settings.sql_height(60), 10);
    }
}
