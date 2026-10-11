use std::fs::{self, OpenOptions};
use std::os::unix::fs::{PermissionsExt, symlink};
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::LazyLock;

use regex::Regex;

use super::installation::{self, Installation};
use super::{BINARY, UpdateFailed, failed, version};

pub const DIRECTORY: &str = ".tql-versions";

pub const NEXT: &str = "next";

pub const KEPT: usize = 3;

pub const STAGING_VARIABLE: &str = "TQL_STAGING";

static REPORTED: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^Tql v?(\d+\.\d+\.\d+)").unwrap());

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Versions {
    pub link: PathBuf,
}

impl Versions {
    pub fn for_installed(path: &Path) -> Self {
        let parent = path.parent().unwrap_or(Path::new("/"));
        if parent.file_name().is_some_and(|name| name == DIRECTORY) {
            return Versions {
                link: parent.parent().unwrap_or(Path::new("/")).join(BINARY),
            };
        }
        Versions {
            link: path.to_path_buf(),
        }
    }

    pub fn folder(&self) -> PathBuf {
        self.link
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from("/"))
    }

    pub fn directory(&self) -> PathBuf {
        self.folder().join(DIRECTORY)
    }

    pub fn file(&self, version: &str) -> PathBuf {
        self.directory().join(format!("{BINARY}-{version}"))
    }

    pub fn stage(&self, version: &str, binary: &[u8]) -> Result<(), UpdateFailed> {
        if !writable(&self.folder()) {
            return Err(failed(format!(
                "{} is not writable; run tql update with sudo",
                self.folder().display()
            )));
        }
        let could_not =
            |error: std::io::Error| failed(format!("could not stage v{version}: {error}"));
        fs::create_dir_all(self.directory()).map_err(could_not)?;

        let staged = self
            .directory()
            .join(format!(".download-{}", random_suffix()));
        let outcome = self.verify_and_keep(version, binary, &staged);
        if staged.exists() {
            let _ = fs::remove_file(&staged);
        }
        outcome?;

        fs::write(self.directory().join(NEXT), format!("{BINARY}-{version}")).map_err(could_not)?;
        self.prune();
        Ok(())
    }

    pub fn switches_safely_now(&self) -> bool {
        self.link.is_symlink()
    }

    pub fn activate(&self, others_running: impl FnOnce() -> bool) -> bool {
        let marker = self.directory().join(NEXT);
        let Ok(name) = fs::read_to_string(&marker) else {
            return false;
        };
        let name = name.trim();

        if name.is_empty() || name.contains('/') || !self.directory().join(name).is_file() {
            let _ = fs::remove_file(&marker);
            return false;
        }

        if !self.link.is_symlink() && self.link.exists() && others_running() {
            return false;
        }

        let temporary = self
            .folder()
            .join(format!(".{BINARY}-link-{}", random_suffix()));
        if symlink(Path::new(DIRECTORY).join(name), &temporary).is_err() {
            return false;
        }
        if fs::rename(&temporary, &self.link).is_err() {
            let _ = fs::remove_file(&temporary);
            return false;
        }

        let _ = fs::remove_file(&marker);
        true
    }

    fn verify_and_keep(
        &self,
        version: &str,
        binary: &[u8],
        staged: &Path,
    ) -> Result<(), UpdateFailed> {
        let could_not =
            |error: std::io::Error| failed(format!("could not stage v{version}: {error}"));
        fs::write(staged, binary).map_err(could_not)?;
        fs::set_permissions(staged, fs::Permissions::from_mode(0o755)).map_err(could_not)?;

        let reported = Command::new(staged)
            .arg("--version")
            .env(STAGING_VARIABLE, "1")
            .stdin(Stdio::null())
            .stderr(Stdio::null())
            .output()
            .ok()
            .filter(|output| output.status.success())
            .and_then(|output| reported_version(&String::from_utf8_lossy(&output.stdout)));

        if reported.as_deref() != Some(version) {
            return Err(failed(format!(
                "the download for v{version} did not run as v{version}, so it was not installed"
            )));
        }

        fs::rename(staged, self.file(version)).map_err(could_not)
    }

    fn prune(&self) {
        let prefix = format!("{BINARY}-");
        let Ok(entries) = fs::read_dir(self.directory()) else {
            return;
        };
        let mut kept: Vec<(String, PathBuf)> = entries
            .flatten()
            .filter_map(|entry| {
                let name = entry.file_name().to_string_lossy().into_owned();
                let numbered = version::of(name.strip_prefix(&prefix)?)?;
                Some((numbered, entry.path()))
            })
            .collect();
        kept.sort_by(|first, second| version::compare(&second.0, &first.0));

        for (_, old) in kept.into_iter().skip(KEPT) {
            let _ = fs::remove_file(old);
        }
    }
}

pub fn reported_version(output: &str) -> Option<String> {
    output.lines().find_map(|line| {
        REPORTED
            .captures(line.trim())
            .map(|captured| captured[1].to_string())
    })
}

pub fn activate_on_start() {
    if std::env::var_os(STAGING_VARIABLE).is_some() {
        return;
    }
    let path = installation::running_path();
    if !Installation::of(&path).updates_itself() {
        return;
    }
    let versions = Versions::for_installed(&path);
    if !versions.activate(others_running) {
        return;
    }
    let _ = Command::new(&versions.link)
        .args(std::env::args_os().skip(1))
        .exec();
}

fn others_running() -> bool {
    let Ok(output) = Command::new("pgrep").args(["-x", BINARY]).output() else {
        return false;
    };
    let own = std::process::id().to_string();
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(str::trim)
        .any(|process| !process.is_empty() && process != own)
}

fn writable(folder: &Path) -> bool {
    let probe = folder.join(format!(".{BINARY}-probe-{}", random_suffix()));
    let created = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&probe)
        .is_ok();
    if created {
        let _ = fs::remove_file(&probe);
    }
    created
}

fn random_suffix() -> String {
    let mut bytes = [0u8; 4];
    let _ = getrandom::fill(&mut bytes);
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_version_from_its_own_line_not_whatever_else_is_printed() {
        assert_eq!(
            reported_version("Using something 8.4.26\nTql v0.8.0\n").as_deref(),
            Some("0.8.0")
        );
        assert_eq!(reported_version("Tql 0.8.1").as_deref(), Some("0.8.1"));
        assert_eq!(reported_version("tql 0.8.0\n"), None);
    }

    #[test]
    fn links_from_inside_the_versions_folder_back_to_the_binary_beside_it() {
        assert_eq!(
            Versions::for_installed(Path::new("/home/me/.local/bin/.tql-versions/tql-0.8.1")).link,
            Path::new("/home/me/.local/bin/tql")
        );
        assert_eq!(
            Versions::for_installed(Path::new("/usr/local/bin/tql")).link,
            Path::new("/usr/local/bin/tql")
        );
        assert_eq!(
            Versions::for_installed(Path::new("/usr/local/bin/tql")).file("0.8.0"),
            Path::new("/usr/local/bin/.tql-versions/tql-0.8.0")
        );
    }
}
