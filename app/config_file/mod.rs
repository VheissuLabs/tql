use std::fs;
use std::path::Path;

use toml::Table;

use crate::settings::Settings;

pub mod file;
pub mod template;
pub mod tidy;

#[derive(Debug, Default)]
pub struct Loaded {
    pub settings: Settings,
    pub notice: Option<String>,
    pub error: Option<String>,
}

impl Loaded {
    pub fn message(&self) -> Option<String> {
        self.error.clone().or_else(|| self.notice.clone())
    }
}

pub fn prepare(path: &Path) -> Loaded {
    let written = match file::ensure(path) {
        Ok(ensured) if ensured.created => Ok(Some(format!("wrote {}", path.display()))),
        Ok(ensured) if !ensured.added.is_empty() => Ok(Some(format!(
            "{} new setting{} added to config.toml: {}",
            ensured.added.len(),
            if ensured.added.len() == 1 { "" } else { "s" },
            ensured.added.join(", ")
        ))),
        Ok(_) => Ok(None),
        Err(error) => Err(format!("could not write {}: {error}", path.display())),
    };

    let mut loaded = load(path);
    match written {
        Ok(notice) => loaded.notice = loaded.notice.or(notice),
        Err(error) => loaded.error = loaded.error.or(Some(error)),
    }
    loaded
}

pub fn load(path: &Path) -> Loaded {
    if !path.is_file() {
        return Loaded::default();
    }
    let name = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    let unreadable = |reason: String| Loaded {
        error: Some(format!("{name} could not be read: {reason}")),
        ..Loaded::default()
    };

    let source = match fs::read_to_string(path) {
        Ok(source) => source,
        Err(error) => return unreadable(error.to_string()),
    };
    let table: Table = match toml::from_str(&source) {
        Ok(table) => table,
        Err(error) => return unreadable(reason(&source, &error)),
    };

    let (hoisted, moved) = file::hoist(table);
    let settings = Settings::from_table(&hoisted);
    let clashes = settings.keymap.clashes();
    let notice = if !moved.is_empty() {
        Some(format!(
            "read {} from the top of config.toml, move them under their [section] to keep them working",
            moved.join(", ")
        ))
    } else if !clashes.is_empty() {
        Some(format!("in [keys]: {}", clashes.join(" · ")))
    } else {
        None
    };
    Loaded {
        settings,
        notice,
        error: None,
    }
}

fn reason(source: &str, error: &toml::de::Error) -> String {
    let message = error.message().trim();
    match error.span() {
        Some(span) => {
            let line = source[..span.start.min(source.len())].matches('\n').count() + 1;
            format!("{message} on line {line}")
        }
        None => message.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn written(name: &str, contents: Option<&str>) -> std::path::PathBuf {
        let directory =
            std::env::temp_dir().join(format!("tql-config-load-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&directory);
        fs::create_dir_all(&directory).unwrap();
        let path = directory.join("config.toml");
        if let Some(contents) = contents {
            fs::write(&path, contents).unwrap();
        }
        path
    }

    #[test]
    fn reads_settings_and_keeps_shipped_defaults_for_the_rest() {
        let path = written(
            "reads",
            Some(
                "# how the app looks\n[ui]\n\n# where the editor sits\nsql_position = \"bottom\"\nsidebar_width = 30\n",
            ),
        );
        let loaded = load(&path);

        assert!(loaded.settings.sql_at_bottom);
        assert_eq!(loaded.settings.sidebar_width, 30);
        assert_eq!(loaded.settings.row_style, "marker");
        assert_eq!(loaded.message(), None);
    }

    #[test]
    fn reports_a_broken_file_instead_of_crashing() {
        let path = written("broken", Some("[ui\nsql_position = \"bottom\"\n"));
        let loaded = load(&path);

        let error = loaded.error.unwrap();
        assert!(
            error.starts_with("config.toml could not be read: "),
            "{error}"
        );
        assert!(error.ends_with("on line 1"), "{error}");
        assert!(!loaded.settings.sql_at_bottom);
    }

    #[test]
    fn ignores_an_empty_file() {
        let loaded = load(&written("empty", Some("")));

        assert_eq!(loaded.message(), None);
        assert_eq!(loaded.settings, Settings::default());
    }

    #[test]
    fn says_which_settings_it_read_from_above_the_first_section() {
        let loaded = load(&written(
            "hoisted",
            Some("sidebar_width = 40\n[ui]\nmouse = false\n"),
        ));

        assert_eq!(loaded.settings.sidebar_width, 40);
        assert_eq!(
            loaded.notice.as_deref(),
            Some(
                "read sidebar_width from the top of config.toml, move them under their [section] to keep them working"
            )
        );
    }

    #[test]
    fn says_when_two_actions_ask_for_the_same_key() {
        let loaded = load(&written("clash", Some("[keys]\nfilter_rows = \"r\"\n")));

        assert_eq!(
            loaded.notice.as_deref(),
            Some("in [keys]: r is both filter_rows and reload")
        );
    }

    #[test]
    fn writes_the_file_on_first_run_and_says_so() {
        let path = written("first-run", None);
        let first = prepare(&path);

        assert_eq!(first.notice, Some(format!("wrote {}", path.display())));
        assert_eq!(first.settings, Settings::default());

        assert_eq!(prepare(&path).message(), None);

        fs::write(
            &path,
            "# tql configuration 0.6.2\n[ui]\nsidebar_width = 40\n",
        )
        .unwrap();
        assert_eq!(
            prepare(&path).notice.as_deref(),
            Some("2 new settings added to config.toml: check, automatic")
        );
    }

    #[test]
    fn an_error_wins_over_a_notice() {
        let path = written("error-wins", Some("[ui]\nsidebar_width = \n"));
        let loaded = prepare(&path);

        assert!(loaded.message().unwrap().contains("could not be read"));
    }
}
