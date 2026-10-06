use zerorust::prelude::*;

use crate::config_file;
use crate::paths;
use crate::settings;
use crate::store_writer::StoreWriter;
use crate::updates::remembered::{CHECKED, INSTALLED, LATEST};
use crate::updates::{
    self, Installation, Releases, Remembered, Unavailable, Updater, Versions, check, version,
};

#[command(
    "update {--background : Check and update quietly, the way tql-rust does once a day by itself}",
    description = "Update tql-rust to the latest release"
)]
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

    remembered.write(CHECKED, &check::now().to_string());

    let latest = match updater.releases.latest() {
        Ok(latest) => latest,
        Err(Unavailable::NoReleases) => {
            cmd.error(format!(
                "No tql-rust release has been published at {} yet.",
                updater.releases.page()
            ));
            return exit(1);
        }
        Err(Unavailable::Unreachable) => {
            cmd.error("Could not reach GitHub to see what the latest release is.");
            return exit(1);
        }
    };

    remembered.write(LATEST, &latest);

    if !version::is_newer(&latest, &current) {
        cmd.info(format!("tql-rust {current} is the latest."));
        return Ok(());
    }

    if !installation.updates_itself() {
        cmd.line(format!(
            "tql-rust {latest} is out. You have {current}; {}.",
            installation.upgrade(&updater.releases)
        ));
        return Ok(());
    }

    cmd.line(format!("Updating tql-rust {current} to {latest}…"));

    if let Err(failure) = updater.install(&latest, &installation.path) {
        cmd.error(format!("{}.", capitalised(&failure.to_string())));
        return exit(1);
    }

    remembered.write(INSTALLED, &latest);

    let versions = Versions::for_installed(&installation.path);
    cmd.info(if versions.switches_safely_now() && versions.activate(|| false) {
        format!(
            "Updated to {latest}, checked against its published checksum. The next tql-rust you start is the new one."
        )
    } else {
        format!(
            "Downloaded {latest} and checked it against its published checksum. It takes over the next time you start tql-rust."
        )
    });
    Ok(())
}

fn capitalised(message: &str) -> String {
    let mut characters = message.chars();
    match characters.next() {
        Some(first) => first.to_uppercase().chain(characters).collect(),
        None => String::new(),
    }
}
