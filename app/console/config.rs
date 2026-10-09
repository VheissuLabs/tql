use std::fs;
use std::path::Path;

use anyhow::Context as _;
use toml::{Table, Value};

#[derive(Debug, Default, Clone)]
pub struct Config {
    root: Table,
}

impl Config {
    pub fn merge_source(&mut self, name: &str, source: &str) -> anyhow::Result<()> {
        let mut table: Table = toml::from_str(source)
            .with_context(|| format!("config file `{name}` is not valid TOML"))?;
        interpolate_table(&mut table);

        match self.root.get_mut(name) {
            Some(Value::Table(existing)) => merge(existing, table),
            _ => {
                self.root.insert(name.to_string(), Value::Table(table));
            }
        }
        Ok(())
    }

    pub fn load_dir(&mut self, dir: &Path) -> anyhow::Result<()> {
        if !dir.is_dir() {
            return Ok(());
        }

        let mut paths: Vec<_> = fs::read_dir(dir)
            .with_context(|| format!("cannot read config directory `{}`", dir.display()))?
            .filter_map(|entry| entry.ok().map(|entry| entry.path()))
            .filter(|path| {
                path.extension()
                    .is_some_and(|extension| extension == "toml")
            })
            .collect();
        paths.sort();

        for path in paths {
            let Some(name) = path.file_stem().and_then(|stem| stem.to_str()) else {
                continue;
            };
            let source = fs::read_to_string(&path)
                .with_context(|| format!("cannot read config file `{}`", path.display()))?;
            self.merge_source(name, &source)?;
        }
        Ok(())
    }

    pub fn get(&self, key: &str) -> Option<&Value> {
        let mut segments = key.split('.');
        let first = self.root.get(segments.next()?)?;
        segments.try_fold(first, |value, segment| match value {
            Value::Table(table) => table.get(segment),
            Value::Array(items) => items.get(segment.parse::<usize>().ok()?),
            _ => None,
        })
    }

    pub fn str(&self, key: &str) -> Option<&str> {
        self.get(key)?.as_str()
    }

    pub fn bool(&self, key: &str) -> Option<bool> {
        match self.get(key)? {
            Value::Boolean(enabled) => Some(*enabled),
            Value::String(text) => match text.to_ascii_lowercase().as_str() {
                "true" | "1" | "yes" | "on" => Some(true),
                "false" | "0" | "no" | "off" | "" => Some(false),
                _ => None,
            },
            _ => None,
        }
    }

    pub fn integer(&self, key: &str) -> Option<i64> {
        match self.get(key)? {
            Value::Integer(number) => Some(*number),
            Value::String(text) => text.trim().parse().ok(),
            _ => None,
        }
    }

    pub fn strings(&self, key: &str) -> Vec<String> {
        self.get(key)
            .and_then(Value::as_array)
            .map(|items| {
                items
                    .iter()
                    .filter_map(|item| item.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default()
    }
}

fn merge(base: &mut Table, overrides: Table) {
    for (key, value) in overrides {
        match value {
            Value::Table(nested) => match base.get_mut(&key) {
                Some(Value::Table(existing)) => merge(existing, nested),
                _ => {
                    base.insert(key, Value::Table(nested));
                }
            },
            other => {
                base.insert(key, other);
            }
        }
    }
}

fn interpolate_table(table: &mut Table) {
    for (_, value) in table.iter_mut() {
        interpolate_value(value);
    }
}

fn interpolate_value(value: &mut Value) {
    match value {
        Value::String(text) => *text = interpolate(text),
        Value::Array(items) => items.iter_mut().for_each(interpolate_value),
        Value::Table(table) => interpolate_table(table),
        _ => {}
    }
}

fn interpolate(text: &str) -> String {
    let mut output = String::new();
    let mut rest = text;

    while let Some(start) = rest.find("${") {
        output.push_str(&rest[..start]);
        let after = &rest[start + 2..];
        let Some(end) = after.find('}') else {
            output.push_str(&rest[start..]);
            return output;
        };

        let (variable, default) = match after[..end].split_once(":-") {
            Some((variable, default)) => (variable, Some(default)),
            None => (&after[..end], None),
        };
        let resolved = std::env::var(variable)
            .ok()
            .filter(|value| !value.is_empty())
            .or_else(|| default.map(String::from))
            .unwrap_or_default();

        output.push_str(&resolved);
        rest = &after[end + 1..];
    }

    output.push_str(rest);
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_dotted_keys_across_tables_and_arrays() {
        let mut config = Config::default();
        config
            .merge_source(
                "app",
                "name = \"Zero\"\n[deploy]\nhosts = [\"one\", \"two\"]\n",
            )
            .unwrap();

        assert_eq!(config.str("app.name"), Some("Zero"));
        assert_eq!(config.str("app.deploy.hosts.1"), Some("two"));
        assert_eq!(config.strings("app.deploy.hosts"), ["one", "two"]);
        assert_eq!(config.get("app.missing"), None);
        assert_eq!(config.get("nope.name"), None);
    }

    #[test]
    fn later_sources_deep_merge_over_earlier_ones() {
        let mut config = Config::default();
        config
            .merge_source("app", "name = \"Zero\"\n[db]\nhost = \"a\"\nport = 1\n")
            .unwrap();
        config.merge_source("app", "[db]\nhost = \"b\"\n").unwrap();

        assert_eq!(config.str("app.name"), Some("Zero"));
        assert_eq!(config.str("app.db.host"), Some("b"));
        assert_eq!(config.integer("app.db.port"), Some(1));
    }

    #[test]
    fn interpolates_environment_variables_with_defaults() {
        unsafe { std::env::set_var("ZERO_CONFIG_TEST_DEBUG", "true") };
        let mut config = Config::default();
        config
            .merge_source(
                "app",
                "debug = \"${ZERO_CONFIG_TEST_DEBUG}\"\nenv = \"${ZERO_CONFIG_TEST_UNSET:-local}\"\nurl = \"http://${ZERO_CONFIG_TEST_UNSET:-localhost}:80\"\n",
            )
            .unwrap();

        assert_eq!(config.bool("app.debug"), Some(true));
        assert_eq!(config.str("app.env"), Some("local"));
        assert_eq!(config.str("app.url"), Some("http://localhost:80"));
    }
}
