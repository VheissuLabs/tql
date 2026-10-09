use std::cell::RefCell;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

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
    config_base().join("tql")
}

fn config_base() -> PathBuf {
    match env::var_os("XDG_CONFIG_HOME").filter(|value| !value.is_empty()) {
        Some(base) => PathBuf::from(base),
        None => home().join(".config"),
    }
}

pub fn carry_over_dotsql() -> bool {
    carry_over(&config_base().join("dotsql"), &config_directory())
}

// tql was dotsql once. Its folder moves over whole, since the passwords in the
// store only open with the key that sits beside them.
pub fn carry_over(from: &Path, to: &Path) -> bool {
    if from == to || to.is_dir() || !from.is_dir() || fs::rename(from, to).is_err() {
        return false;
    }
    let old = to.join("dotsql.sqlite");
    let new = to.join("tql.sqlite");
    if old.is_file() && !new.is_file() {
        let _ = fs::rename(old, new);
    }
    true
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
    expand_from(path, &home())
}

pub fn shorten(path: &str) -> String {
    shorten_from(path, &home())
}

pub fn shorten_from(path: &str, home: &Path) -> String {
    let home = home.display().to_string();
    match path.strip_prefix(&format!("{home}/")) {
        Some(rest) if !home.is_empty() => format!("~/{rest}"),
        _ => path.to_string(),
    }
}

pub fn expand_from(path: &str, home: &Path) -> String {
    match path.strip_prefix('~') {
        Some(rest) if rest.is_empty() || rest.starts_with('/') => {
            format!("{}{rest}", home.display())
        }
        _ => path.to_string(),
    }
}

pub fn resolve(path: &str, home: &Path, from: &Path) -> String {
    let expanded = expand_from(path.trim(), home);
    let absolute = if expanded.starts_with('/') {
        PathBuf::from(&expanded)
    } else {
        from.join(&expanded)
    };
    fs::canonicalize(&absolute)
        .unwrap_or(absolute)
        .display()
        .to_string()
}

pub fn home() -> PathBuf {
    PathBuf::from(env::var_os("HOME").unwrap_or_default())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn carries_a_dotsql_folder_over_with_its_store_renamed() {
        let base = env::temp_dir().join(format!("tql-dotsql-{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        fs::create_dir_all(base.join("dotsql")).unwrap();
        fs::write(base.join("dotsql/dotsql.sqlite"), "store").unwrap();
        fs::write(base.join("dotsql/key"), "key").unwrap();

        assert!(carry_over(&base.join("dotsql"), &base.join("tql")));
        assert_eq!(
            fs::read_to_string(base.join("tql/tql.sqlite")).unwrap(),
            "store"
        );
        assert_eq!(fs::read_to_string(base.join("tql/key")).unwrap(), "key");
        assert!(!base.join("dotsql").exists());

        fs::create_dir_all(base.join("dotsql")).unwrap();
        assert!(!carry_over(&base.join("dotsql"), &base.join("tql")));
        fs::remove_dir_all(base).unwrap();
    }

    #[test]
    fn resolves_a_path_under_home_and_one_relative_to_where_it_started() {
        let directory = env::temp_dir().join(format!("tql-paths-{}", std::process::id()));
        fs::create_dir_all(directory.join("code")).unwrap();
        fs::write(directory.join("code/lunar.sqlite"), "").unwrap();
        let real = fs::canonicalize(directory.join("code/lunar.sqlite")).unwrap();

        let from_home = resolve(" ~/code/lunar.sqlite ", &directory, Path::new("/"));
        let relative = resolve(
            "lunar.sqlite",
            Path::new("/nowhere"),
            &directory.join("code"),
        );
        let missing = resolve("gone.sqlite", Path::new("/nowhere"), Path::new("/tmp/away"));
        fs::remove_dir_all(&directory).unwrap();

        assert_eq!(from_home, real.display().to_string());
        assert_eq!(relative, real.display().to_string());
        assert_eq!(missing, "/tmp/away/gone.sqlite");
    }

    #[test]
    fn shortens_only_paths_inside_home() {
        let home = Path::new("/home/me");

        assert_eq!(shorten_from("/home/me/db.sqlite", home), "~/db.sqlite");
        assert_eq!(
            shorten_from("/home/meadow/db.sqlite", home),
            "/home/meadow/db.sqlite"
        );
        assert_eq!(expand_from("~", home), "/home/me");
        assert_eq!(expand_from("~other/x", home), "~other/x");
    }
}
