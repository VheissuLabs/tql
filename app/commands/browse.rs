use crate::console::prelude::*;
use crate::terminal::{Flow, Session};

use crate::config_file;
use crate::database;
use crate::driver::Driver;
use crate::encrypter::Encrypter;
use crate::keystore;
use crate::live::{self, Live};
use crate::paths;
use crate::registry::{self, Entry, Needs, Shell};
use crate::settings;
use crate::store::{Connection, Store};
use crate::store_writer::StoreWriter;
use crate::tui::clipboard::SystemClipboard;
use crate::tui::{Browser, Exit, KeyBrowser, Picker};
use crate::updates::{self, Installation, Releases, Remembered, check};

pub struct Browse;

impl Command for Browse {
    fn signature(&self) -> &str {
        "browse"
    }

    fn description(&self) -> &str {
        "Browse and query your databases"
    }

    fn execute(&self, cmd: &mut Cmd) -> Result {
        browse(cmd, None, None, None)
    }
}

pub struct Connect;

impl Command for Connect {
    fn signature(&self) -> &str {
        "connect {connection : The saved connection, by name} {--db= : Which database on the server to open, for a connection that has several}"
    }

    fn description(&self) -> &str {
        "Open a saved connection by name, straight into its tables"
    }

    fn help(&self) -> &str {
        r#"Skips the connection list and opens the connection you name, as if you had
picked it there. The list is still one key away.

<fg=yellow>Examples</>

  <fg=green>tql connect prod</>
      opens prod on the database it is saved with

  <fg=green>tql connect prod --db=analytics</>
      opens prod on another database on the same server, this once

A path or a connection string opens it the way <fg=green>tql open</> does."#
    }

    fn execute(&self, cmd: &mut Cmd) -> Result {
        let name = cmd.argument("connection").map(String::from);
        let database = cmd.option("db").map(String::from);
        browse(cmd, None, name, database)
    }
}

pub struct Launch {
    entry: &'static Entry,
    signature: String,
    description: String,
}

impl Launch {
    pub fn all() -> Vec<Launch> {
        registry::opening()
            .map(|entry| Launch {
                entry,
                description: launching(entry.description),
                signature: match entry.needs {
                    Needs::Nothing => entry.word().to_string(),
                    Needs::Connection => format!(
                        "{} {{connection? : The connection to open; without one, tql asks}}",
                        entry.word()
                    ),
                },
            })
            .collect()
    }
}

impl Command for Launch {
    fn signature(&self) -> &str {
        &self.signature
    }

    fn description(&self) -> &str {
        &self.description
    }

    fn execute(&self, cmd: &mut Cmd) -> Result {
        debug_assert_eq!(self.entry.shell, Shell::Opens);
        if self.entry.name == registry::Name::Settings && !cmd.has_terminal() {
            cmd.line(paths::config_file().display().to_string());
            return Ok(());
        }
        let connection = match self.entry.needs {
            Needs::Connection => cmd.argument("connection").map(String::from),
            Needs::Nothing => None,
        };
        browse(cmd, Some(self.entry), connection, None)
    }
}

fn browse(
    cmd: &mut Cmd,
    start: Option<&'static Entry>,
    named: Option<String>,
    database: Option<String>,
) -> Result {
    let mut chosen = None;
    if let Some(name) = named {
        let Some(connection) = saved_connection(cmd, &name, database)? else {
            return exit(1);
        };
        chosen = Some(connection);
    }
    if !cmd.has_terminal() {
        cmd.error("tql needs a terminal. Run it directly rather than piping into it.");
        return exit(1);
    }
    let loaded = config_file::prepare(&paths::config_file());
    let message = loaded.message();
    let update_check = check::enabled(loaded.settings.update_check);
    settings::install(loaded.settings);

    StoreWriter::prepare(&paths::store())?;
    let store = Store::open(&paths::store())?;

    let remembered = Remembered::new(&paths::store());
    check::in_the_background(update_check, &remembered);
    let update_notice = || {
        check::notice(
            &updates::current_version(),
            &Installation::running(),
            &Releases::configured(),
            &remembered,
        )
    };
    let mut notice = loaded.error.clone();

    let mut pending = start;

    let mut session = Session::start()?;
    loop {
        let connection = match chosen.take() {
            Some(connection) => connection,
            None => {
                let mut picker =
                    Picker::new(store.connections()?, notice.take().or_else(update_notice))
                        .with_store(&paths::store(), &paths::key())
                        .with_clipboard(SystemClipboard);
                match pending {
                    Some(entry) if entry.needs == Needs::Nothing => {
                        pending = None;
                        if let Flow::Exit(_) = picker.run_named(entry.name, "") {
                            return Ok(());
                        }
                    }
                    Some(entry) => {
                        picker.status =
                            Some(format!("choose a connection to {}", entry.description));
                    }
                    None => {}
                }
                let Some(connection) = session.run(&mut picker)? else {
                    return Ok(());
                };
                picker.status = Some(connecting_to(&connection.name));
                session.show(&mut picker)?;
                connection
            }
        };

        let encrypter = Encrypter::from_file(&paths::key()).ok();
        if connection.driver == Driver::Redis {
            let opened = keystore::connect(&connection, encrypter.as_ref())
                .and_then(|store| KeyBrowser::new(connection.clone(), store))
                .map(|browser| browser.with_store(paths::store()));
            let mut keys = match opened {
                Ok(keys) => keys,
                Err(error) => {
                    notice = Some(format!("{}: {error:#}", connection.name));
                    continue;
                }
            };
            let _ = StoreWriter::open(&paths::store(), &paths::key())
                .and_then(|writer| writer.touch(connection.id));
            pending = None;
            match session.run(&mut keys)? {
                Exit::Quit => return Ok(()),
                Exit::Connections => {}
                Exit::Open(id) => {
                    chosen = store
                        .connections()?
                        .into_iter()
                        .find(|connection| connection.id == id);
                    if let Some(next) = &chosen {
                        keys.tell(connecting_to(&next.name));
                        session.show(&mut keys)?;
                    }
                }
            }
            continue;
        }
        let opened = database::connect(&connection, encrypter.as_ref())
            .and_then(|database| Browser::new(connection.clone(), database))
            .map(|browser| {
                browser
                    .with_store(paths::store())
                    .with_clipboard(SystemClipboard)
                    .with_status(message.clone())
                    .with_live(Live::register(&live::directory(), &connection.name))
            });
        let mut browser = match opened {
            Ok(browser) => browser,
            Err(error) => {
                notice = Some(format!("{}: {error:#}", connection.name));
                continue;
            }
        };

        let _ = StoreWriter::open(&paths::store(), &paths::key())
            .and_then(|writer| writer.touch(connection.id));
        if let Some(entry) = pending.take()
            && let Flow::Exit(exit) = browser.run_named(entry.name, "")
        {
            match exit {
                Exit::Quit => return Ok(()),
                _ => continue,
            }
        }

        match session.run(&mut browser)? {
            Exit::Quit => return Ok(()),
            Exit::Connections => {}
            Exit::Open(id) => {
                chosen = store
                    .connections()?
                    .into_iter()
                    .find(|connection| connection.id == id);
                if let Some(next) = &chosen {
                    browser.tell(connecting_to(&next.name));
                    session.show(&mut browser)?;
                }
            }
        }
    }
}

fn saved_connection(
    cmd: &mut Cmd,
    name: &str,
    database: Option<String>,
) -> Result<Option<Connection>> {
    StoreWriter::prepare(&paths::store())?;
    let saved = Store::open(&paths::store())?.connections()?;
    let Some(connection) = saved
        .iter()
        .find(|connection| connection.name.eq_ignore_ascii_case(name))
    else {
        let known: Vec<&str> = saved
            .iter()
            .map(|connection| connection.name.as_str())
            .collect();
        cmd.error(format!(
            "There is no tql connection named [{name}]. Known connections: {}",
            known.join(", ")
        ));
        return Ok(None);
    };
    let mut connection = connection.clone();
    if let Some(database) = database {
        if !connection.driver.has_databases() {
            cmd.error(format!(
                "[{}] is a {} connection, which is one database; --db means nothing to it.",
                connection.name,
                connection.driver.label()
            ));
            return Ok(None);
        }
        connection.database = Some(database);
    }
    Ok(Some(connection))
}

fn launching(description: &str) -> String {
    format!("{}, in tql", crate::tui::text::sentence(description))
}

fn connecting_to(name: &str) -> String {
    format!("connecting to {name}…")
}
