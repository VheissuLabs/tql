use std::fs::{self, DirBuilder};
use std::os::unix::fs::DirBuilderExt;
use std::path::PathBuf;
use std::time::Instant;

use crate::commands::dumps::{self, Saved};
use crate::console::prelude::*;
use crate::dump::{self, DumpOptions, Engine, LoadOptions};
use crate::paths;
use crate::store::Connection;

pub struct Sync;

impl Command for Sync {
    fn signature(&self) -> &str {
        "sync {from : The connection to copy from, which is only ever read} {to : The connection to copy into} {tables?* : Only these tables, or every table when none are named} {--database= : The database to copy, for a connection that names none} {--into= : What the database is called on the other side, when it is not the same} {--threads=4 : How many tables, or pieces of a table, to copy at once} {--no-lock : Skip the consistent snapshot, for a MySQL user without the privileges it needs} {--via= : The folder the copy passes through on its way} {--keep : Keep that folder afterwards, as a backup} {--force : Copy into a production connection, or past the free space check} {--dry-run : Show both steps without running them}"
    }

    fn description(&self) -> &str {
        "Make one database a copy of another: a dump and a load in one step"
    }

    fn help(&self) -> &str {
        r#"Dumps a database from one connection and loads it into another, replacing
the tables it brings over. Tables it does not bring over are left alone.

<fg=yellow>Examples</>

  <fg=green>tql sync prod local --database=shop</>
      shop on prod becomes shop on local

  <fg=green>tql sync prod local --database=shop --into=shop_copy</>
      the same, under another name locally

  <fg=green>tql sync prod local --database=shop users orders</>
      just two tables"#
    }

    fn execute(&self, cmd: &mut Cmd) -> Result {
        let saved = Saved::open(cmd);
        let from = cmd.argument("from").unwrap_or_default().to_string();
        let to = cmd.argument("to").unwrap_or_default().to_string();
        let Some(mut source) = saved.find(cmd, &from) else {
            return exit(1);
        };
        let Some(mut target) = saved.find(cmd, &to) else {
            return exit(1);
        };
        if !dumps::settle_database(cmd, &mut source) {
            return exit(1);
        }

        if target.driver != "sqlite" {
            let own = target
                .database
                .clone()
                .map(|database| database.trim().to_string())
                .filter(|database| !database.is_empty());
            target.database = cmd
                .option("into")
                .map(String::from)
                .or(own)
                .or_else(|| source.database.clone());
        }

        let Some(engine) = dumps::engine_for(cmd, &source.driver) else {
            return exit(1);
        };
        if Engine::for_driver(&target.driver) != Some(engine) {
            cmd.error(format!(
                "{} is {} and {} is {}; tql does not move between engines yet.",
                source.name,
                dump::label(&source.driver),
                target.name,
                dump::label(&target.driver)
            ));
            return exit(1);
        }
        if same_place(&source, &target) {
            cmd.error(format!("That would copy {} onto itself.", label(&source)));
            return exit(1);
        }
        if !dumps::may_write_to(cmd, &target)? {
            return exit(1);
        }
        if !dumps::tool_is_installed(cmd, engine.dumper(), engine)
            || !dumps::tool_is_installed(cmd, engine.loader(), engine)
        {
            return exit(1);
        }

        let reachable = match target.driver.as_str() {
            "sqlite" => vec![source.clone()],
            _ => vec![source.clone(), dumps::server_of(&target)],
        };
        for connection in reachable {
            if let Some(failure) = saved.test(&connection) {
                cmd.error(format!("{}: {failure}", connection.name));
                return exit(1);
            }
        }

        let directory = match directory(cmd, &saved, &source) {
            Ok(directory) => directory,
            Err(error) => {
                cmd.error(format!("{error:#}"));
                return exit(1);
            }
        };
        let dump_options = DumpOptions {
            tables: cmd.arguments("tables").to_vec(),
            threads: dumps::threads(cmd),
            data_only: false,
            no_lock: cmd.flag("no-lock"),
        };
        let load_options = LoadOptions {
            drop: true,
            threads: dump_options.threads,
        };

        let built = engine
            .dump(&source, &directory, &dump_options, saved.encrypter())
            .and_then(|dumping| {
                let loading = engine.load(&target, &directory, &load_options, saved.encrypter())?;
                let estimate =
                    engine.estimated_bytes(&source, &dump_options.tables, saved.encrypter())?;
                Ok((dumping, loading, estimate))
            });
        let (dumping, loading, estimate) = match built {
            Ok(built) => built,
            Err(error) => {
                cmd.error(format!("{error:#}"));
                return exit(1);
            }
        };

        if !cmd.flag("force") && !dumps::room_for(cmd, estimate, engine, &directory, "--via") {
            return exit(1);
        }

        let about = estimate
            .map(|bytes| format!(" (about {} of data)", dump::human_bytes(bytes)))
            .unwrap_or_default();
        cmd.marked(&format!(
            "  Syncing <fg=cyan>{}</> into <fg=cyan>{}</>{about}",
            label(&source),
            label(&target)
        ));

        if cmd.flag("dry-run") {
            for job in [&dumping, &loading] {
                let what = match job.native {
                    None => job.describe(),
                    Some(_) => "a SQLite copy".to_string(),
                };
                cmd.line(format!("  {what}"));
            }
            return Ok(());
        }

        if let Some(parent) = directory.parent()
            && !parent.is_dir()
        {
            DirBuilder::new()
                .recursive(true)
                .mode(0o755)
                .create(parent)?;
        }

        let started = Instant::now();
        if !dumps::run_quietly_failing(cmd, dumping, "Dumping") {
            let _ = fs::remove_dir_all(&directory);
            cmd.error(format!(
                "The copy out of {} did not finish, so {} was not touched.",
                source.name, target.name
            ));
            return exit(1);
        }

        dump::write_manifest(
            &directory,
            engine,
            &source,
            &dump_options,
            cmd.app_version(),
        )?;

        if !dumps::run_quietly_failing(cmd, loading, "Loading") {
            cmd.error(format!("Loading into {} did not finish.", target.name));
            cmd.marked(&format!(
                "  the dump is still in {}; <fg=green>tql load</> can try it again.",
                dumps::shorten(&directory)
            ));
            return exit(1);
        }

        match cmd.flag("keep") {
            true => cmd.line(format!(
                "  the dump is kept in {}",
                dumps::shorten(&directory)
            )),
            false => {
                let _ = fs::remove_dir_all(&directory);
            }
        }

        cmd.marked(&format!(
            "  <fg=green>Synced</> in {}",
            dump::elapsed(started.elapsed().as_secs_f64())
        ));
        Ok(())
    }
}

fn same_place(source: &Connection, target: &Connection) -> bool {
    let place = |connection: &Connection| match connection.driver.as_str() {
        "sqlite" => vec![paths::expand(
            connection.database.as_deref().unwrap_or_default(),
        )],
        _ => vec![
            connection.host.clone().unwrap_or_default(),
            connection.port.unwrap_or_default().to_string(),
            connection.ssh_host.clone().unwrap_or_default(),
            dump::active_database(connection),
        ],
    };
    source.driver == target.driver && place(source) == place(target)
}

fn label(connection: &Connection) -> String {
    match connection.driver.as_str() {
        "sqlite" => connection.name.clone(),
        _ => format!(
            "{} · {}",
            connection.name,
            dump::active_database(connection)
        ),
    }
}

fn directory(cmd: &Cmd, saved: &Saved, source: &Connection) -> anyhow::Result<PathBuf> {
    if let Some(chosen) = dumps::folder(cmd, "via") {
        return Ok(chosen);
    }
    let subject = match dump::active_database(source).is_empty() {
        true => source.name.clone(),
        false => dump::active_database(source),
    };
    Ok(saved.exports(source)?.join(format!(
        "sync-{}-{}",
        dump::slug(&subject, "sync"),
        dumps::stamp()
    )))
}
