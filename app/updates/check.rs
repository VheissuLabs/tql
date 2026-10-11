use std::os::unix::process::CommandExt;
use std::process::{Command, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

use super::remembered::{CHECKED, INSTALLED, LATEST};
use super::{BINARY, Installation, Releases, Remembered, Updater, version};

pub const EVERY_SECONDS: i64 = 86_400;

pub const CHECK_VARIABLE: &str = "TQL_UPDATE_CHECK";

pub fn enabled(setting: bool) -> bool {
    let turned_off = std::env::var(CHECK_VARIABLE).is_ok_and(|value| {
        matches!(
            value.trim().to_lowercase().as_str(),
            "" | "false" | "(false)" | "0" | "off" | "no"
        )
    });
    setting && !turned_off
}

pub fn due(enabled: bool, remembered: &Remembered, now: i64) -> bool {
    if !enabled {
        return false;
    }
    let checked: i64 = remembered
        .read(CHECKED)
        .and_then(|checked| checked.trim().parse().ok())
        .unwrap_or_default();
    now.saturating_sub(checked) >= EVERY_SECONDS
}

pub fn run(
    current: &str,
    installation: &Installation,
    updater: &Updater,
    remembered: &Remembered,
    automatic: bool,
) -> Option<String> {
    remembered.write(CHECKED, &now().to_string());

    let latest = updater.releases.latest().ok()?;
    remembered.write(LATEST, &latest);

    if !version::is_newer(&latest, current) {
        return None;
    }

    if !installation.updates_itself() || !automatic {
        return notice(current, installation, &updater.releases, remembered);
    }

    if let Err(failure) = updater.install(&latest, &installation.path) {
        return Some(failure.to_string());
    }

    remembered.write(INSTALLED, &latest);
    notice(current, installation, &updater.releases, remembered)
}

pub fn notice(
    current: &str,
    installation: &Installation,
    releases: &Releases,
    remembered: &Remembered,
) -> Option<String> {
    let newer = |key: &str| {
        remembered
            .read(key)
            .filter(|remembered_version| version::is_newer(remembered_version, current))
    };
    if let Some(installed) = newer(INSTALLED) {
        return Some(format!("tql {installed} is ready · restart to use it"));
    }
    newer(LATEST).map(|latest| format!("tql {latest} is out · {}", installation.upgrade(releases)))
}

pub fn in_the_background(enabled: bool, remembered: &Remembered) {
    let program = Installation::running().relaunch();
    let ours = program
        .file_name()
        .is_some_and(|name| name.to_string_lossy().starts_with(BINARY));
    if !ours || !due(enabled, remembered, now()) {
        return;
    }
    let spawned = Command::new(program)
        .args(["update", "--background"])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .process_group(0)
        .spawn();
    if let Ok(mut child) = spawned {
        std::thread::spawn(move || child.wait());
    }
}

pub fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs() as i64)
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_nonsense_last_check_means_it_is_time_to_check() {
        let directory = std::env::temp_dir().join(format!("tql-due-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&directory);
        crate::store_writer::StoreWriter::prepare(&directory.join("tql.sqlite")).unwrap();
        let remembered = Remembered::new(&directory.join("tql.sqlite"));
        remembered.write(CHECKED, &i64::MIN.to_string());

        let due = due(true, &remembered, now());
        std::fs::remove_dir_all(&directory).unwrap();

        assert!(due);
    }
}
