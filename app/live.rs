use std::fs::{self, File};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant, SystemTime};

use anyhow::{Context, anyhow, bail};
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
    request: PathBuf,
    reply: PathBuf,
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
            request: directory.join(format!("{stem}.ask")),
            reply: directory.join(format!("{stem}.screen")),
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

    pub fn asked(&self) -> bool {
        self.request.exists()
    }

    pub fn answer(&self, screen: &Value) {
        let unread = self.reply.with_extension("screen.new");
        let written =
            fs::write(&unread, screen.to_string()).and_then(|()| fs::rename(&unread, &self.reply));
        if written.is_err() {
            let _ = fs::remove_file(&unread);
        }
        let _ = fs::remove_file(&self.request);
    }
}

impl Drop for Live {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.mailbox);
        let _ = fs::remove_file(&self.request);
        let _ = fs::remove_file(&self.reply);
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
                for leftover in ["sql", "ask", "screen"] {
                    let _ = fs::remove_file(registration.with_extension(leftover));
                }
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

fn chosen(directory: &Path, connection: &str) -> anyhow::Result<Open> {
    let mut open = open_ones(directory);
    let mut elsewhere: Vec<String> = open.iter().map(|open| open.connection.clone()).collect();
    open.retain(|open| open.connection == connection);
    if let Some(chosen) = open.into_iter().max_by_key(|open| open.used) {
        return Ok(chosen);
    }
    elsewhere.sort();
    elsewhere.dedup();
    Err(anyhow!(match elsewhere.is_empty() {
        true => {
            format!("tql is not open. Ask the user to open [{connection}] in tql, then try again.")
        }
        false => format!(
            "No tql is open on [{connection}]. tql is open on: {}",
            elsewhere.join(", ")
        ),
    }))
}

pub fn read_screen(directory: &Path, connection: &str) -> anyhow::Result<Value> {
    let chosen = chosen(directory, connection)?;
    let request = chosen.mailbox.with_extension("ask");
    let reply = chosen.mailbox.with_extension("screen");
    let _ = fs::remove_file(&reply);
    fs::write(&request, "").context("Could not ask tql for its screen")?;

    let started = Instant::now();
    while started.elapsed() < WAIT_FOR_PICKUP {
        if let Ok(answer) = fs::read_to_string(&reply) {
            let _ = fs::remove_file(&reply);
            return serde_json::from_str(&answer).context("tql answered with something unreadable");
        }
        std::thread::sleep(LOOK_EVERY);
    }
    let _ = fs::remove_file(&request);
    bail!("tql on [{connection}] did not answer; it may be busy. Try again in a moment.")
}

pub fn deliver(directory: &Path, connection: &str, sql: &str) -> anyhow::Result<String> {
    if sql.trim().is_empty() {
        bail!("Give the SQL to put in the editor.");
    }
    let chosen = chosen(directory, connection)?;

    let unread = chosen.mailbox.with_extension("sql.new");
    fs::write(&unread, sql)
        .and_then(|()| fs::rename(&unread, &chosen.mailbox))
        .context("Could not hand the SQL to tql")?;

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
