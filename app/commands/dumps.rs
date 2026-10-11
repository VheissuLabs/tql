use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command as Process, Stdio};
use std::sync::{Once, mpsc};
use std::thread;

use regex::Regex;

use crate::config_file;
use crate::console::prelude::*;
use crate::console::progress::Progress;
use crate::database::{self, Exporter};
use crate::dump::{self, Engine, Job, Update, secrets};
use crate::encrypter::Encrypter;
use crate::paths;
use crate::settings;
use crate::store::{Connection, Store};
use crate::store_writer::StoreWriter;
use crate::tag;
use crate::tunnel;

const RECENT: usize = 40;
const EXPLAINED: usize = 8;

pub struct Saved {
    pub store: PathBuf,
    pub connections: Vec<Connection>,
    pub encrypter: Option<Encrypter>,
    _tunnels: tunnel::CloseAtExit,
    _secrets: secrets::ForgetAtExit,
}

impl Saved {
    pub fn open(cmd: &mut Cmd) -> Self {
        let loaded = config_file::prepare(&paths::config_file());
        if let Some(error) = &loaded.error {
            cmd.warn(error);
        }
        settings::install(loaded.settings);
        let store = paths::store();
        if let Err(error) = StoreWriter::prepare(&store) {
            cmd.error(format!("could not prepare {}: {error:#}", store.display()));
        }
        Saved {
            connections: Store::open(&store)
                .and_then(|opened| opened.connections())
                .unwrap_or_default(),
            store,
            encrypter: Encrypter::from_file(&paths::key()).ok(),
            _tunnels: tunnel::CloseAtExit,
            _secrets: secrets::ForgetAtExit,
        }
    }

    pub fn find(&self, cmd: &mut Cmd, name: &str) -> Option<Connection> {
        if let Some(found) = self
            .connections
            .iter()
            .find(|connection| connection.name == name)
        {
            return Some(found.clone());
        }
        let mut known: Vec<&str> = self
            .connections
            .iter()
            .map(|connection| connection.name.as_str())
            .collect();
        known.sort();
        cmd.error(format!("No connection named [{name}]."));
        cmd.line(format!("  known: {}", known.join(", ")));
        None
    }

    pub fn encrypter(&self) -> Option<&Encrypter> {
        self.encrypter.as_ref()
    }

    pub fn exports(&self, connection: &Connection) -> anyhow::Result<PathBuf> {
        Exporter::new(connection, Some(&self.store)).directory()
    }

    pub fn test(&self, connection: &Connection) -> Option<String> {
        database::connect(connection, self.encrypter())
            .err()
            .map(|error| format!("{error:#}"))
    }
}

pub fn settle_database(cmd: &mut Cmd, connection: &mut Connection) -> bool {
    if let Some(database) = cmd.option("database")
        && connection.driver != "sqlite"
    {
        connection.database = Some(database.to_string());
    }
    if connection.driver == "sqlite" || !dump::active_database(connection).trim().is_empty() {
        return true;
    }
    cmd.error(format!(
        "[{}] is a server, not a database.",
        connection.name
    ));
    cmd.marked("  name one with <fg=green>--database=</>.");
    false
}

pub fn engine_for(cmd: &mut Cmd, driver: &str) -> Option<Engine> {
    let engine = Engine::for_driver(driver);
    if engine.is_none() {
        cmd.error(format!(
            "{} is not supported yet; tql dumps and loads MySQL, Postgres and SQLite.",
            dump::label(driver)
        ));
    }
    engine
}

pub fn tool_is_installed(cmd: &mut Cmd, tool: Option<&str>, engine: Engine) -> bool {
    let Some(tool) = tool else {
        return true;
    };
    if on_path(tool) {
        return true;
    }
    cmd.error(format!(
        "{tool} is not installed, and tql uses it for {}.",
        dump::label(engine.name())
    ));
    cmd.marked(&format!("  <fg=green>{}</>", engine.install()));
    false
}

fn on_path(tool: &str) -> bool {
    std::env::var_os("PATH").is_some_and(|path| {
        std::env::split_paths(&path).any(|directory| {
            let candidate = directory.join(tool);
            std::fs::metadata(&candidate).is_ok_and(|meta| {
                use std::os::unix::fs::PermissionsExt;
                meta.is_file() && meta.permissions().mode() & 0o111 != 0
            })
        })
    })
}

pub fn may_write_to(cmd: &mut Cmd, connection: &Connection) -> Result<bool> {
    if connection.read_only {
        cmd.error(format!(
            "{} is read only, so nothing can be loaded into it.",
            connection.name
        ));
        return Ok(false);
    }
    let production =
        tag::parse(connection.tag.as_deref().unwrap_or_default()) == Some("production");
    if !production || cmd.flag("force") || cmd.flag("dry-run") {
        return Ok(true);
    }
    if !cmd.is_interactive() {
        cmd.error(format!(
            "{} is tagged production; --force loads into it anyway.",
            connection.name
        ));
        return Ok(false);
    }
    cmd.confirm_box(
        &format!(
            "{} is tagged production. Load into it anyway?",
            connection.name
        ),
        false,
    )
}

pub fn room_for(
    cmd: &mut Cmd,
    estimate: Option<u64>,
    engine: Engine,
    directory: &Path,
    folder_option: &str,
) -> bool {
    let Some(estimate) = estimate else {
        return true;
    };
    let needed = match engine.compresses() {
        true => estimate / 3,
        false => estimate,
    };
    let Some(free) = dump::free_space(directory) else {
        return true;
    };
    if free >= needed {
        return true;
    }
    cmd.error(format!(
        "The dump needs about {} and the drive under {} has {} free.",
        dump::human_bytes(needed),
        paths::shorten(&directory.display().to_string()),
        dump::human_bytes(free)
    ));
    cmd.marked(&format!(
        "  choose another folder with <fg=green>{folder_option}=</>, or <fg=green>--force</> to try anyway."
    ));
    false
}

pub fn server_of(connection: &Connection) -> Connection {
    let mut server = connection.clone();
    if server.driver == "mysql" {
        server.database = None;
    }
    server
}

pub fn threads(cmd: &Cmd) -> usize {
    cmd.option("threads")
        .and_then(|threads| threads.trim().parse::<i64>().ok())
        .unwrap_or(0)
        .max(1) as usize
}

pub fn folder(cmd: &Cmd, option: &str) -> Option<PathBuf> {
    cmd.option(option).map(|folder| {
        let resolved = paths::resolve(
            folder,
            &paths::home(),
            &std::env::current_dir().unwrap_or_default(),
        );
        let trimmed = resolved.trim_end_matches('/');
        PathBuf::from(match trimmed.is_empty() {
            true => "/",
            false => trimmed,
        })
    })
}

pub fn stamp() -> String {
    jiff::Zoned::now().strftime("%Y%m%d-%H%M%S").to_string()
}

pub fn shorten(path: &Path) -> String {
    paths::shorten(&path.display().to_string())
}

pub fn run_quietly_failing(cmd: &mut Cmd, job: Job, label: &str) -> bool {
    match run_job(cmd, job, label) {
        Ok(finished) => finished,
        Err(error) => {
            cmd.error(format!("{error:#}"));
            false
        }
    }
}

enum Said {
    Line(String),
    Done,
}

fn run_job(cmd: &mut Cmd, mut job: Job, label: &str) -> Result<bool> {
    if let Some(native) = job.native.take() {
        native()?;
        return Ok(true);
    }

    stop_cleanly_on_interrupt();

    let verbose = cmd.is_verbose();
    let live = !verbose && !cmd.is_piped();
    let mut bar = match live {
        true => start_progress(cmd, label, job.expected_tables()),
        false => None,
    };

    let mut child = Process::new(&job.command[0])
        .args(&job.command[1..])
        .envs(job.environment.iter().map(|(key, value)| (key, value)))
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;

    let (said, heard) = mpsc::channel();
    let readers: Vec<_> = [
        child
            .stdout
            .take()
            .map(|stream| Box::new(stream) as Box<dyn Read + Send>),
        child
            .stderr
            .take()
            .map(|stream| Box::new(stream) as Box<dyn Read + Send>),
    ]
    .into_iter()
    .flatten()
    .map(|stream| {
        let said = said.clone();
        thread::spawn(move || relay(stream, said))
    })
    .collect();
    drop(said);

    let mut recent: Vec<String> = Vec::new();
    let mut open = readers.len();
    while open > 0 {
        let Ok(heard) = heard.recv() else {
            break;
        };
        let line = match heard {
            Said::Done => {
                open -= 1;
                continue;
            }
            Said::Line(line) => line,
        };
        if line.trim().is_empty() {
            continue;
        }
        if verbose {
            gray(cmd, &line);
            continue;
        }
        recent.push(line.clone());
        if recent.len() > RECENT {
            recent.remove(0);
        }
        if live && let Some(update) = job.progress.as_mut().and_then(|watch| watch(&line)) {
            bar = show_progress(cmd, bar, label, update);
        }
    }
    for reader in readers {
        let _ = reader.join();
    }

    let status = child.wait()?;
    let succeeded = status.success();

    if let Some(mut bar) = bar {
        if succeeded {
            bar.progress = bar.total;
        }
        bar.hint.clear();
        bar.finish(cmd);
    }

    if !succeeded && !verbose {
        explain_failure(cmd, &recent);
    }
    Ok(succeeded)
}

fn relay(mut stream: Box<dyn Read + Send>, said: mpsc::Sender<Said>) {
    let mut pending: Vec<u8> = Vec::new();
    let mut chunk = [0u8; 8192];
    let mut after_return = false;
    loop {
        let read = match stream.read(&mut chunk) {
            Ok(0) | Err(_) => break,
            Ok(read) => read,
        };
        for &byte in &chunk[..read] {
            if byte == b'\n' && after_return {
                after_return = false;
                continue;
            }
            after_return = byte == b'\r';
            if byte == b'\n' || byte == b'\r' {
                let line = String::from_utf8_lossy(&std::mem::take(&mut pending)).into_owned();
                let _ = said.send(Said::Line(line));
            } else {
                pending.push(byte);
            }
        }
    }
    if !pending.is_empty() {
        let _ = said.send(Said::Line(String::from_utf8_lossy(&pending).into_owned()));
    }
    let _ = said.send(Said::Done);
}

fn start_progress(cmd: &mut Cmd, label: &str, expected: usize) -> Option<Progress> {
    if expected < 1 {
        return None;
    }
    let mut bar = Progress::new(label, expected, "connecting…");
    bar.start(cmd);
    Some(bar)
}

fn show_progress(
    cmd: &mut Cmd,
    bar: Option<Progress>,
    label: &str,
    update: Update,
) -> Option<Progress> {
    let total = update
        .total
        .filter(|total| *total > 0)
        .or(bar.as_ref().map(|bar| bar.total));
    let Some(total) = total.filter(|total| *total >= 1) else {
        return bar;
    };
    let mut bar = bar.or_else(|| start_progress(cmd, label, total))?;
    bar.total = total;
    bar.progress = update.done.min(total);
    bar.hint = update.current.unwrap_or_default();
    bar.render(cmd, false);
    Some(bar)
}

fn explain_failure(cmd: &mut Cmd, recent: &[String]) {
    let problem = Regex::new(r"(?i)warning|error|critical|fatal").expect("the pattern is valid");
    let problems: Vec<&String> = recent
        .iter()
        .filter(|line| problem.is_match(line))
        .collect();
    let shown: Vec<&String> = match problems.is_empty() {
        true => recent.iter().collect(),
        false => problems,
    };
    for line in &shown[shown.len().saturating_sub(EXPLAINED)..] {
        gray(cmd, line.trim());
    }
    cmd.marked("  run it again with <fg=green>-v</> to see everything the tool said.");
}

fn gray(cmd: &mut Cmd, line: &str) {
    let (open, close) = (cmd.markup("<fg=gray>"), cmd.markup("</>"));
    cmd.line(format!("  {open}{line}{close}"));
}

fn stop_cleanly_on_interrupt() {
    static INSTALLED: Once = Once::new();
    INSTALLED.call_once(|| {
        let Ok(mut signals) = signal_hook::iterator::Signals::new([
            signal_hook::consts::SIGINT,
            signal_hook::consts::SIGTERM,
        ]) else {
            return;
        };
        thread::spawn(move || {
            if signals.forever().next().is_some() {
                tunnel::close_all();
                secrets::forget();
                print!("\x1b[?25h");
                std::process::exit(130);
            }
        });
    });
}
