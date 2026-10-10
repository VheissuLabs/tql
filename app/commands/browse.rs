use crate::console::prelude::*;
use crate::terminal::Session;

use crate::config_file;
use crate::database;
use crate::encrypter::Encrypter;
use crate::live::{self, Live};
use crate::paths;
use crate::settings;
use crate::store::Store;
use crate::store_writer::StoreWriter;
use crate::tui::clipboard::SystemClipboard;
use crate::tui::{Browser, Exit, Picker};
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
        browse(cmd)
    }
}

fn browse(cmd: &mut Cmd) -> Result {
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

    let mut session = Session::start()?;
    let mut chosen = None;
    loop {
        let connection = match chosen.take() {
            Some(connection) => connection,
            None => {
                let mut picker =
                    Picker::new(store.connections()?, notice.take().or_else(update_notice))
                        .with_store(&paths::store(), &paths::key())
                        .with_clipboard(SystemClipboard);
                let Some(connection) = session.run(&mut picker)? else {
                    return Ok(());
                };
                picker.status = Some(connecting_to(&connection.name));
                session.show(&mut picker)?;
                connection
            }
        };

        let encrypter = Encrypter::from_file(&paths::key()).ok();
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

fn connecting_to(name: &str) -> String {
    format!("connecting to {name}…")
}
