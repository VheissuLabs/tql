use std::path::PathBuf;

use serde_json::{Value, json};

use super::answers;
use crate::agent;
use crate::console::prelude::*;
use crate::paths;
use crate::store_writer::StoreWriter;
use crate::tableplus::{self, AFTER_MACOS_ASKS, Entry};

pub struct TablePlus;

impl Command for TablePlus {
    fn signature(&self) -> &str {
        "tableplus {--dry-run : List what would come over, and touch nothing} {--without-passwords : Leave passwords in the Keychain} {--from= : The TablePlus Data folder, when it is not in the usual place} {--json : JSON, which is what you get when the output is piped} {--table : Lines for a person, which is what you get in a terminal}"
    }

    fn description(&self) -> &str {
        "Bring your TablePlus connections into tql, groups and passwords too"
    }

    fn help(&self) -> &str {
        r#"Reads the connections TablePlus has saved on this Mac and adds each one to
tql, with its group, its environment as a tag, its SSH tunnel and TLS
settings. Passwords come from the Keychain, and macOS asks you to allow
each one. A connection tql already has is left alone, so running it twice
is safe.

<fg=yellow>For an agent</>

  Run <fg=green>tql tableplus --dry-run</> first and show the person what will
  come over, then <fg=green>tql tableplus</>. Both answer in JSON when piped.
  Tell the person to watch for the macOS Keychain prompts."#
    }

    fn execute(&self, cmd: &mut Cmd) -> Result {
        let directory = match cmd.option("from") {
            Some(from) => Some(PathBuf::from(
                paths::resolve(from, &paths::home(), &std::env::current_dir()?)
                    .trim_end_matches('/'),
            )),
            None => tableplus::find(),
        };
        let Some(directory) = directory else {
            return refuse(
                cmd,
                match cfg!(target_os = "macos") {
                    true => "TablePlus has no saved connections on this Mac.",
                    false => {
                        "tql reads TablePlus for Mac; TablePlus for Windows and Linux is not supported yet."
                    }
                },
            );
        };

        let dry_run = cmd.flag("dry-run");
        let with_passwords = !cmd.flag("without-passwords");
        let results = StoreWriter::prepare(&paths::store())
            .and_then(|()| StoreWriter::open(&paths::store(), &paths::key()))
            .and_then(|writer| {
                let plan = tableplus::plan(&directory, &writer)?;
                match dry_run {
                    true => Ok(plan),
                    false => tableplus::import(plan, with_passwords, &writer),
                }
            });
        let results = match results {
            Ok(results) => results,
            Err(refusal) => return refuse(cmd, &format!("{refusal:#}")),
        };

        match answers::wants_json(cmd) {
            true => {
                let report = report(&directory, &results, dry_run);
                cmd.line(escaped(&agent::pretty(&report)));
            }
            false => draw(cmd, &results, dry_run),
        }
        Ok(())
    }
}

fn report(directory: &std::path::Path, results: &[Entry], dry_run: bool) -> Value {
    let coming: Vec<Value> = results
        .iter()
        .filter(|entry| entry.skip.is_none())
        .map(|entry| {
            json!({
                "name": entry.name,
                "driver": entry.driver,
                "group": entry.group,
                "tag": entry.tag,
                "over_ssh": entry.over_ssh,
                "password": entry.password.as_deref().unwrap_or(AFTER_MACOS_ASKS),
            })
        })
        .collect();
    let skipped: Vec<Value> = results
        .iter()
        .filter_map(|entry| {
            let reason = entry.skip.as_ref()?;
            Some(json!({ "name": entry.name, "reason": reason }))
        })
        .collect();

    let mut report = serde_json::Map::new();
    report.insert("source".into(), json!(directory.display().to_string()));
    report.insert("dry_run".into(), json!(dry_run));
    report.insert(
        match dry_run {
            true => "would_import",
            false => "imported",
        }
        .into(),
        Value::Array(coming),
    );
    report.insert("skipped".into(), Value::Array(skipped));
    Value::Object(report)
}

fn draw(cmd: &mut Cmd, results: &[Entry], dry_run: bool) {
    for entry in results {
        if let Some(reason) = &entry.skip {
            let line = format!(
                "  {}   {}  {}",
                cmd.gray("skip"),
                entry.name,
                cmd.gray(reason)
            );
            cmd.line(line);
            continue;
        }

        let r#where: Vec<&str> = [
            Some(entry.driver.as_str()),
            entry.group.as_deref(),
            entry.tag.as_deref(),
            entry.over_ssh.then_some("ssh"),
        ]
        .into_iter()
        .flatten()
        .filter(|part| !part.is_empty() && *part != "0")
        .collect();
        let password = entry.password.as_deref().unwrap_or(AFTER_MACOS_ASKS);
        let line = format!(
            "  {}  {}  {}",
            cmd.green(match dry_run {
                true => "would",
                false => "added",
            }),
            entry.name,
            cmd.gray(format!("{} · password {password}", r#where.join(" · "))),
        );
        cmd.line(line);
    }

    let count = results.iter().filter(|entry| entry.skip.is_none()).count();
    cmd.new_line();
    let closing = match dry_run {
        true => format!(
            "  {count} to bring over. Run {} to do it.",
            cmd.green("tql tableplus")
        ),
        false => format!("  Added {count}. Run {} to see them.", cmd.green("tql")),
    };
    cmd.line(closing);
}

fn refuse(cmd: &mut Cmd, message: &str) -> Result {
    match answers::wants_json(cmd) {
        true => cmd.line(escaped(&json!({ "error": message }).to_string())),
        false => cmd.error(message),
    }
    exit(1)
}

fn escaped(json: &str) -> String {
    let mut escaped = String::with_capacity(json.len());
    for character in json.chars() {
        match character.is_ascii() {
            true => escaped.push(character),
            false => {
                let mut units = [0; 2];
                for unit in character.encode_utf16(&mut units) {
                    escaped.push_str(&format!("\\u{unit:04x}"));
                }
            }
        }
    }
    escaped
}
