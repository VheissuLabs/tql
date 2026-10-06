use std::io::IsTerminal;

use zerorust::prelude::*;
use zerorust_tui::CmdTui;

use crate::database;
use crate::opener;
use crate::paths;
use crate::settings::{self, Settings};
use crate::store_writer::StoreWriter;
use crate::tui::Browser;
use crate::tui::clipboard::SystemClipboard;

#[command(
    "open {path : a SQLite file, or a mysql:// pgsql:// connection string} {--name= : what to call it in the connection list} {--tag= : what it is: production, staging, dev or local} {--peek : open it without remembering it}",
    description = "Open a database by path or connection string"
)]
fn open(cmd: &mut Cmd) -> Result {
    let tag = cmd.option("tag").map(String::from);
    if let Some(refusal) = opener::refuse_tag(tag.as_deref()) {
        cmd.error(refusal);
        cmd.line("  --name= is what you are looking for to call it something.");
        return exit(1);
    }

    let target = match opener::target(cmd.argument("path")) {
        Ok(target) => target,
        Err(refusal) => {
            cmd.error(refusal);
            return exit(1);
        }
    };

    StoreWriter::prepare(&paths::store())?;
    let opened = opener::connection_for(
        &paths::store(),
        &paths::key(),
        target,
        cmd.option("name"),
        tag.as_deref(),
        cmd.flag("peek"),
    )?;

    let database = match database::connect(&opened.connection, opened.encrypter.as_ref()) {
        Ok(database) => database,
        Err(error) => {
            cmd.error(format!("{error:#}"));
            return exit(1);
        }
    };

    if !std::io::stdin().is_terminal() {
        cmd.error("tql needs a terminal. Run it directly rather than piping into it.");
        return exit(1);
    }

    if opened.connection.id != 0 {
        StoreWriter::open(&paths::store(), &paths::key())?.touch(opened.connection.id)?;
    }

    if let Ok(loaded) = Settings::load(&paths::config_file()) {
        settings::install(loaded);
    }

    let mut browser = Browser::new(opened.connection, database)?
        .with_store(paths::store())
        .with_clipboard(SystemClipboard);
    cmd.tui(&mut browser)?;
    Ok(())
}
