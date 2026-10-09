use std::fs::{self, File};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant, SystemTime};

use serde_json::{Value, json};

use crate::paths;

const FRESHEN_EVERY: Duration = Duration::from_secs(1);
const WAIT_FOR_PICKUP: Duration = Duration::from_secs(2);
const LOOK_EVERY: Duration = Duration::from_millis(50);

static OPENED: AtomicUsize = AtomicUsize::new(0);

pub fn directory() -> PathBuf {
    paths::config_directory().join("live")
}

pub struct Live {
    held: File,
    registration: PathBuf,
    mailbox: PathBuf,
    freshened: Instant,
}

impl Live {
    pub fn register(directory: &Path, connection: &str) -> Option<Live> {
        fs::create_dir_all(directory).ok()?;
        let stem = format!(
            "{}-{}",
            std::process::id(),
            OPENED.fetch_add(1, Ordering::Relaxed)
        );
        let registration = directory.join(format!("{stem}.json"));
        let unclaimed = directory.join(format!("{stem}.json.new"));
        fs::write(&unclaimed, json!({ "connection": connection }).to_string()).ok()?;
        let held = File::open(&unclaimed).ok()?;
        held.try_lock().ok()?;
        fs::rename(&unclaimed, &registration).ok()?;
        Some(Live {
            held,
            registration,
            mailbox: directory.join(format!("{stem}.sql")),
            freshened: Instant::now(),
        })
    }

    pub fn used(&mut self) {
        if self.freshened.elapsed() < FRESHEN_EVERY {
            return;
        }
        self.freshened = Instant::now();
        if let Ok(registration) = File::options().write(true).open(&self.registration) {
            let _ = registration.set_modified(SystemTime::now());
        }
    }

    pub fn take(&self) -> Option<String> {
        let sql = fs::read_to_string(&self.mailbox).ok()?;
        let _ = fs::remove_file(&self.mailbox);
        Some(sql)
    }
}

impl Drop for Live {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.mailbox);
        let _ = fs::remove_file(&self.registration);
        let _ = self.held.unlock();
    }
}

struct Open {
    connection: String,
    used: SystemTime,
    mailbox: PathBuf,
}

fn open_ones(directory: &Path) -> Vec<Open> {
    let Ok(entries) = fs::read_dir(directory) else {
        return Vec::new();
    };
    entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            path.extension()
                .is_some_and(|extension| extension == "json")
        })
        .filter_map(|registration| {
            let file = File::open(&registration).ok()?;
            if file.try_lock().is_ok() {
                let _ = fs::remove_file(registration.with_extension("sql"));
                let _ = fs::remove_file(&registration);
                return None;
            }
            let read: Value =
                serde_json::from_str(&fs::read_to_string(&registration).ok()?).ok()?;
            Some(Open {
                connection: read["connection"].as_str()?.to_string(),
                used: file.metadata().and_then(|meta| meta.modified()).ok()?,
                mailbox: registration.with_extension("sql"),
            })
        })
        .collect()
}

pub fn deliver(directory: &Path, connection: &str, sql: &str) -> Result<String, String> {
    if sql.trim().is_empty() {
        return Err("Give the SQL to put in the editor.".to_string());
    }
    let open = open_ones(directory);
    let Some(chosen) = open
        .iter()
        .filter(|open| open.connection == connection)
        .max_by_key(|open| open.used)
    else {
        let mut elsewhere: Vec<&str> = open.iter().map(|open| open.connection.as_str()).collect();
        elsewhere.sort();
        elsewhere.dedup();
        return Err(match elsewhere.is_empty() {
            true => format!(
                "tql is not open. Ask the user to open [{connection}] in tql, then try again."
            ),
            false => format!(
                "No tql is open on [{connection}]. tql is open on: {}",
                elsewhere.join(", ")
            ),
        });
    };

    let unread = chosen.mailbox.with_extension("sql.new");
    fs::write(&unread, sql)
        .and_then(|()| fs::rename(&unread, &chosen.mailbox))
        .map_err(|error| format!("Could not hand the SQL to tql: {error}"))?;

    let started = Instant::now();
    while started.elapsed() < WAIT_FOR_PICKUP {
        if !chosen.mailbox.exists() {
            return Ok(format!(
                "The SQL is in the SQL editor of the tql open on [{connection}]. Nothing ran; the user reads it and runs it."
            ));
        }
        std::thread::sleep(LOOK_EVERY);
    }
    Ok(format!(
        "tql on [{connection}] is busy; the SQL goes into its SQL editor as soon as it is free. Nothing ran."
    ))
}
