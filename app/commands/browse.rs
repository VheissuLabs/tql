use zerorust::prelude::*;
use zerorust_tui::CmdTui;

use crate::database;
use crate::encrypter::Encrypter;
use crate::paths;
use crate::settings::{self, Settings};
use crate::store::Store;
use crate::tui::clipboard::SystemClipboard;
use crate::tui::{Browser, Exit, Picker};

#[command("browse", description = "Browse and query your databases")]
fn browse(cmd: &mut Cmd) -> Result {
    let mut notice = None;
    match Settings::load(&paths::config_file()) {
        Ok(loaded) => settings::install(loaded),
        Err(error) => notice = Some(format!("{error:#}")),
    }

    let store = Store::open(&paths::store())?;
    let encrypter = Encrypter::from_file(&paths::key()).ok();

    let mut chosen = None;
    loop {
        let connection = match chosen.take() {
            Some(connection) => connection,
            None => {
                let mut picker = Picker::new(store.connections()?, notice.take());
                let Some(connection) = cmd.tui(&mut picker)? else {
                    return Ok(());
                };
                connection
            }
        };

        let opened = database::connect(&connection, encrypter.as_ref())
            .and_then(|database| Browser::new(connection.clone(), database))
            .map(|browser| {
                browser
                    .with_store(paths::store())
                    .with_clipboard(SystemClipboard)
            });
        let mut browser = match opened {
            Ok(browser) => browser,
            Err(error) => {
                notice = Some(format!("{}: {error:#}", connection.name));
                continue;
            }
        };

        match cmd.tui(&mut browser)? {
            Exit::Quit => return Ok(()),
            Exit::Connections => {}
            Exit::Open(id) => {
                chosen = store
                    .connections()?
                    .into_iter()
                    .find(|connection| connection.id == id);
            }
        }
    }
}
