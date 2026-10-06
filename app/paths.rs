use std::cell::RefCell;
use std::env;
use std::path::PathBuf;

thread_local! {
    static CONFIG_DIRECTORY: RefCell<Option<PathBuf>> = const { RefCell::new(None) };
}

pub fn use_config_directory(directory: PathBuf) {
    CONFIG_DIRECTORY.with(|chosen| *chosen.borrow_mut() = Some(directory));
}

pub fn config_directory() -> PathBuf {
    if let Some(chosen) = CONFIG_DIRECTORY.with(|chosen| chosen.borrow().clone()) {
        return chosen;
    }
    let base = match env::var_os("XDG_CONFIG_HOME").filter(|value| !value.is_empty()) {
        Some(base) => PathBuf::from(base),
        None => home().join(".config"),
    };
    base.join("tql")
}

pub fn store() -> PathBuf {
    config_directory().join("tql.sqlite")
}

pub fn config_file() -> PathBuf {
    config_directory().join("config.toml")
}

pub fn key() -> PathBuf {
    config_directory().join("key")
}

pub fn expand(path: &str) -> String {
    if path == "~" || path.starts_with("~/") {
        return format!("{}{}", home().display(), &path[1..]);
    }
    path.to_string()
}

pub fn shorten(path: &str) -> String {
    let home = home().display().to_string();
    match path.strip_prefix(&format!("{home}/")) {
        Some(rest) if !home.is_empty() => format!("~/{rest}"),
        _ => path.to_string(),
    }
}

fn home() -> PathBuf {
    PathBuf::from(env::var_os("HOME").unwrap_or_default())
}
