use crate::console::prelude::*;

use crate::config_file;
use crate::paths;
use crate::settings;
use crate::store_writer::StoreWriter;
use crate::updates::{self, Installation, Releases, Remembered, Updater, check};

pub struct Update;

impl Command for Update {
    fn signature(&self) -> &str {
        "update {--background : Check and update quietly, the way tql does once a day by itself}"
    }

    fn description(&self) -> &str {
        "Update tql to the latest release"
    }

    fn execute(&self, cmd: &mut Cmd) -> Result {
        update(cmd)
    }
}

fn update(cmd: &mut Cmd) -> Result {
    let loaded = config_file::prepare(&paths::config_file());
    let automatic = loaded.settings.update_automatic;
    settings::install(loaded.settings);
    let _ = StoreWriter::prepare(&paths::store());

    let current = updates::current_version();
    let installation = Installation::running();
    let updater = Updater::new(Releases::configured());
    let remembered = Remembered::new(&paths::store());

    if cmd.flag("background") {
        check::run(&current, &installation, &updater, &remembered, automatic);
        return Ok(());
    }

    let mut started = |line: &str| cmd.line(line);
    match check::by_hand(&current, &installation, &updater, &remembered, &mut started) {
        Ok(said) => cmd.info(said),
        Err(failed) => {
            cmd.error(failed);
            return exit(1);
        }
    }
    Ok(())
}
