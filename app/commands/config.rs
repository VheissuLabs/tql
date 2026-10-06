use std::fs;

use zerorust::prelude::*;

use crate::config_file::{file, template, tidy};
use crate::paths;
use crate::settings::Settings;

#[command(
    "config {--tidy : Put the settings back in order, keeping your values} {--path : Print the path to the file and stop}",
    description = "Show where the config file is, and put it back in order"
)]
fn config(cmd: &mut Cmd) -> Result {
    let path = paths::config_file();
    let _ = file::ensure(&path);
    let shown = path.display().to_string();

    if cmd.flag("path") {
        cmd.line(&shown);
        return Ok(());
    }

    if !path.is_file() {
        cmd.error(format!(
            "No config file yet. Run tql once and it writes {shown}."
        ));
        return exit(1);
    }

    let contents = fs::read_to_string(&path)?;
    let (tidied, moved) = tidy::apply(&contents);
    let moved = unique(moved);

    if !cmd.flag("tidy") {
        cmd.line(format!("  {shown}"));
        cmd.line(format!(
            "  written for {}, template is {}",
            file::written_for(&contents)
                .unwrap_or_else(|| "before tql wrote versions into it".to_string()),
            template::VERSION
        ));
        let clashes = toml::from_str(&contents)
            .map(|table| Settings::from_table(&file::hoist(table).0).keymap.clashes())
            .unwrap_or_default();
        for clash in clashes {
            cmd.comment(format!("  {clash}"));
        }
        if moved.is_empty() {
            cmd.info("  in order");
            return Ok(());
        }
        cmd.comment(format!("  out of order: {}", moved.join(", ")));
        cmd.line("  tql config --tidy sorts it, keeping your values.");
        return Ok(());
    }

    if tidied == contents {
        cmd.line(format!("  Already in order: {shown}"));
        return Ok(());
    }

    let backup = format!("{shown}.bak");
    fs::copy(&path, &backup)?;
    fs::write(&path, tidied)?;

    cmd.line(format!("  Tidied {shown}"));
    cmd.line(format!(
        "  {} setting{} moved: {}",
        moved.len(),
        if moved.len() == 1 { "" } else { "s" },
        moved.join(", ")
    ));
    cmd.line(format!("  The file as it was: {backup}"));
    Ok(())
}

fn unique(names: Vec<String>) -> Vec<String> {
    let mut kept: Vec<String> = Vec::new();
    for name in names {
        if !kept.contains(&name) {
            kept.push(name);
        }
    }
    kept
}
