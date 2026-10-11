use std::fs::{self, DirBuilder, OpenOptions};
use std::io::Write;
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
use std::path::PathBuf;
use std::sync::Mutex;

struct Held {
    directory: Option<PathBuf>,
    written: usize,
}

static HELD: Mutex<Held> = Mutex::new(Held {
    directory: None,
    written: 0,
});

pub fn write(name: &str, contents: &str) -> anyhow::Result<PathBuf> {
    let mut held = HELD.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let directory = match &held.directory {
        Some(directory) => directory.clone(),
        None => {
            let mut random = [0u8; 6];
            getrandom::fill(&mut random)
                .map_err(|error| anyhow::anyhow!("no randomness for a secret folder: {error}"))?;
            let hex: String = random.iter().map(|byte| format!("{byte:02x}")).collect();
            let directory = std::env::temp_dir().join(format!("tql-secrets-{hex}"));
            DirBuilder::new().mode(0o700).create(&directory)?;
            held.directory = Some(directory.clone());
            directory
        }
    };

    held.written += 1;
    let path = directory.join(format!("{}-{name}", held.written));
    OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&path)?
        .write_all(contents.as_bytes())?;
    Ok(path)
}

pub fn forget() {
    let mut held = HELD.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    if let Some(directory) = held.directory.take() {
        let _ = fs::remove_dir_all(directory);
    }
}

pub struct ForgetAtExit;

impl Drop for ForgetAtExit {
    fn drop(&mut self) {
        forget();
    }
}
