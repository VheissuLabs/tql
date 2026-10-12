use crate::console::prelude::*;
use crate::terminal::CmdTui;

use crate::config_file;
use crate::database;
use crate::live::{self, Live};
use crate::opener;
use crate::paths;
use crate::settings;
use crate::store_writer::StoreWriter;
use crate::tui::Browser;
use crate::tui::clipboard::SystemClipboard;

pub struct Open;

impl Command for Open {
    fn signature(&self) -> &str {
        "open {path : a SQLite or DuckDB file, or a mysql:// pgsql:// sqlsrv:// libsql:// clickhouse:// redis:// or valkey:// connection string} {--name= : what to call it in the connection list} {--tag= : what it is: production, staging, dev or local} {--peek : open it without remembering it}"
    }

    fn description(&self) -> &str {
        "Open a database by path or connection string"
    }

    fn aliases(&self) -> &[&'static str] {
        &["connect"]
    }

    fn execute(&self, cmd: &mut Cmd) -> Result {
        open(cmd)
    }
}

fn open(cmd: &mut Cmd) -> Result {
    let tag = cmd.option("tag").map(String::from);
    if let Some(refusal) = opener::refuse_tag(tag.as_deref()) {
        cmd.error(refusal);
        cmd.line("  --name= is what you are looking for to call it something.");
        return exit(1);
    }

    let target = match opener::target(cmd.argument("path").unwrap_or_default()) {
        Ok(target) => target,
        Err(refusal) => {
            cmd.error(format!("{refusal:#}"));
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

    if opened.connection.driver == crate::driver::Driver::Redis {
        let store = match crate::keystore::connect(&opened.connection, opened.encrypter.as_ref()) {
            Ok(store) => store,
            Err(error) => {
                cmd.error(format!("{error:#}"));
                return exit(1);
            }
        };
        if !cmd.has_terminal() {
            cmd.error("tql needs a terminal. Run it directly rather than piping into it.");
            return exit(1);
        }
        if opened.connection.id != 0 {
            StoreWriter::open(&paths::store(), &paths::key())?.touch(opened.connection.id)?;
        }
        settings::install(config_file::prepare(&paths::config_file()).settings);
        let mut keys =
            crate::tui::KeyBrowser::new(opened.connection, store)?.with_store(paths::store());
        cmd.tui(&mut keys)?;
        return Ok(());
    }

    let database = match database::connect(&opened.connection, opened.encrypter.as_ref()) {
        Ok(database) => database,
        Err(error) => {
            cmd.error(format!("{error:#}"));
            return exit(1);
        }
    };

    if !cmd.has_terminal() {
        cmd.error("tql needs a terminal. Run it directly rather than piping into it.");
        return exit(1);
    }

    if opened.connection.id != 0 {
        StoreWriter::open(&paths::store(), &paths::key())?.touch(opened.connection.id)?;
    }

    let loaded = config_file::prepare(&paths::config_file());
    let message = loaded.message();
    settings::install(loaded.settings);

    let live = Live::register(&live::directory(), &opened.connection.name);
    let mut browser = Browser::new(opened.connection, database)?
        .with_status(message)
        .with_store(paths::store())
        .with_clipboard(SystemClipboard)
        .with_live(live);
    cmd.tui(&mut browser)?;
    Ok(())
}
