use std::cell::RefCell;
use std::path::{Path, PathBuf};

use super::Releases;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum How {
    Homebrew,
    Package,
    ByItself,
    Built,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Installation {
    pub how: How,
    pub path: PathBuf,
}

thread_local! {
    static PRETENDED: RefCell<Option<Installation>> = const { RefCell::new(None) };
}

pub fn pretend_running(installation: Installation) {
    PRETENDED.with(|pretended| *pretended.borrow_mut() = Some(installation));
}

impl Installation {
    pub fn new(how: How, path: impl Into<PathBuf>) -> Self {
        Installation {
            how,
            path: path.into(),
        }
    }

    pub fn running() -> Self {
        if let Some(pretended) = PRETENDED.with(|pretended| pretended.borrow().clone()) {
            return pretended;
        }
        Installation::of(&running_path())
    }

    pub fn of(path: &Path) -> Self {
        let shown = path.display().to_string();
        let how = if shown.contains("/Cellar/") {
            How::Homebrew
        } else if shown.starts_with("/usr/bin/") {
            How::Package
        } else if shown.contains("/target/") || shown.contains("/.cargo/bin/") {
            How::Built
        } else {
            How::ByItself
        };
        Installation::new(how, path)
    }

    pub fn updates_itself(&self) -> bool {
        self.how == How::ByItself
    }

    pub fn upgrade(&self, releases: &Releases) -> String {
        match self.how {
            How::Homebrew => "brew upgrade tql-rust".to_string(),
            How::Package => "install the new .deb, .rpm or PKGBUILD".to_string(),
            How::ByItself => "tql-rust update".to_string(),
            How::Built => format!("get it from {}", releases.page()),
        }
    }

    pub fn relaunch(&self) -> PathBuf {
        self.path.clone()
    }
}

pub fn running_path() -> PathBuf {
    let executable = std::env::current_exe().unwrap_or_default();
    std::fs::canonicalize(&executable).unwrap_or(executable)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn knows_how_tql_rust_was_installed_from_where_the_binary_is() {
        for (path, how) in [
            (
                "/opt/homebrew/Cellar/tql-rust/0.7.0/bin/tql-rust",
                How::Homebrew,
            ),
            (
                "/home/linuxbrew/.linuxbrew/Cellar/tql-rust/0.7.0/bin/tql-rust",
                How::Homebrew,
            ),
            ("/usr/bin/tql-rust", How::Package),
            ("/usr/local/bin/tql-rust", How::ByItself),
            ("/Users/someone/.local/bin/tql-rust", How::ByItself),
            (
                "/Users/someone/.local/bin/.tql-rust-versions/tql-rust-0.8.0",
                How::ByItself,
            ),
            ("/Users/someone/.cargo/bin/tql-rust", How::Built),
            (
                "/Users/someone/Code/tql-rust/target/release/tql-rust",
                How::Built,
            ),
        ] {
            assert_eq!(Installation::of(Path::new(path)).how, how, "{path}");
        }
    }

    #[test]
    fn tells_each_kind_of_install_how_to_upgrade() {
        let releases = Releases::github("VheissuLabs/tql-rust");
        for (how, says) in [
            (How::Homebrew, "brew upgrade tql-rust"),
            (How::Package, ".deb"),
            (How::ByItself, "tql-rust update"),
            (How::Built, "github.com/VheissuLabs/tql-rust/releases"),
        ] {
            let upgrade = Installation::new(how, "/x/tql-rust").upgrade(&releases);
            assert!(upgrade.contains(says), "{upgrade}");
        }
    }

    #[test]
    fn finds_the_running_binary_by_its_real_path() {
        let path = running_path();

        assert!(path.is_absolute());
        assert_eq!(std::fs::canonicalize(&path).unwrap(), path);
    }
}
