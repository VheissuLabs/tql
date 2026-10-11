use std::fs::{self, DirBuilder};
use std::os::unix::fs::DirBuilderExt;
use std::path::{Path, PathBuf};
use std::time::Instant;

use crate::commands::dumps::{self, Saved};
use crate::console::prelude::*;
use crate::driver::Driver;
use crate::dump::{self, DumpOptions};
use crate::store::Connection;

pub struct Dump;

impl Command for Dump {
    fn signature(&self) -> &str {
        "dump {connection : The tql connection to dump} {tables?* : Only these tables, or every table when none are named} {--to= : The folder to write the dump into} {--database= : Which database on the server, for a connection that names none} {--threads=4 : How many tables, or pieces of a table, to dump at once} {--data-only : Rows only, for loading into tables that already exist} {--no-lock : Skip the consistent snapshot, for a MySQL user without the privileges it needs} {--force : Dump even when the drive looks too small} {--dry-run : Show the command tql would run, and stop}"
    }

    fn description(&self) -> &str {
        "Dump a database, schema and data, with mydumper or pg_dump"
    }

    fn help(&self) -> &str {
        r#"Writes a whole database, or some of its tables, into a folder that
<fg=green>tql load</> reads back. MySQL uses mydumper and Postgres uses pg_dump, in
parallel and from a consistent snapshot. SQLite needs no tool.

<fg=yellow>Examples</>

  <fg=green>tql dump prod</>
      every table, into a new folder next to your exports

  <fg=green>tql dump prod orders users --to=./orders</>
      two tables, into a folder you name

  <fg=green>tql dump prod --data-only</>
      rows only, to load into a database that already has the tables"#
    }

    fn execute(&self, cmd: &mut Cmd) -> Result {
        let saved = Saved::open(cmd);
        let name = cmd.argument("connection").unwrap_or_default().to_string();
        let Some(mut connection) = saved.find(cmd, &name) else {
            return exit(1);
        };
        if !dumps::settle_database(cmd, &mut connection) {
            return exit(1);
        }
        let Some(engine) = dumps::engine_for(cmd, connection.driver) else {
            return exit(1);
        };
        if !dumps::tool_is_installed(cmd, engine.dumper(), engine) {
            return exit(1);
        }

        let directory = match directory(cmd, &saved, &connection) {
            Ok(directory) => directory,
            Err(error) => {
                cmd.error(format!("{error:#}"));
                return exit(1);
            }
        };
        if fs::read_dir(&directory).is_ok_and(|mut entries| entries.next().is_some()) {
            cmd.error(format!(
                "{} already has something in it; dump into a new or empty folder.",
                directory.display()
            ));
            return exit(1);
        }

        if let Some(failure) = saved.test(&connection) {
            cmd.error(failure);
            return exit(1);
        }

        let options = DumpOptions {
            tables: cmd.arguments("tables").to_vec(),
            threads: dumps::threads(cmd),
            data_only: cmd.flag("data-only"),
            no_lock: cmd.flag("no-lock"),
        };

        let built = engine
            .dump(&connection, &directory, &options, saved.encrypter())
            .and_then(|job| {
                let estimate =
                    engine.estimated_bytes(&connection, &options.tables, saved.encrypter())?;
                Ok((job, estimate))
            });
        let (job, estimate) = match built {
            Ok(built) => built,
            Err(error) => {
                cmd.error(format!("{error:#}"));
                return exit(1);
            }
        };

        if !cmd.flag("force") && !dumps::room_for(cmd, estimate, engine, &directory, "--to") {
            return exit(1);
        }

        let about = estimate
            .map(|bytes| format!(" (about {} of data)", dump::human_bytes(bytes)))
            .unwrap_or_default();
        cmd.marked(&format!(
            "  Dumping <fg=cyan>{}</>{about} into {}",
            connection.name,
            dumps::shorten(&directory)
        ));

        if cmd.flag("dry-run") {
            let what = match job.native {
                None => job.describe(),
                Some(_) => format!(
                    "a SQLite backup of {}",
                    connection.database.as_deref().unwrap_or_default()
                ),
            };
            cmd.line(format!("  {what}"));
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
        if !dumps::run_quietly_failing(cmd, job, "Dumping") {
            cmd.error(format!(
                "The dump did not finish; what is in {} is incomplete.",
                dumps::shorten(&directory)
            ));
            return exit(1);
        }

        dump::write_manifest(&directory, engine, &connection, &options, cmd.app_version())?;

        cmd.marked(&format!(
            "  <fg=green>Dumped</> {} in {}",
            dump::human_bytes(dump::directory_size(&directory)),
            dump::elapsed(started.elapsed().as_secs_f64())
        ));
        cmd.marked(&format!(
            "  load it with <fg=green>tql load {} <connection></>",
            dumps::shorten(&directory)
        ));
        Ok(())
    }
}

fn directory(cmd: &Cmd, saved: &Saved, connection: &Connection) -> anyhow::Result<PathBuf> {
    if let Some(chosen) = dumps::folder(cmd, "to") {
        return Ok(chosen);
    }
    let subject = match connection.driver {
        Driver::Sqlite => Path::new(connection.database.as_deref().unwrap_or_default())
            .file_stem()
            .map(|stem| stem.to_string_lossy().into_owned())
            .unwrap_or_default(),
        _ => dump::active_database(connection),
    };
    let subject = match subject.is_empty() {
        true => connection.name.clone(),
        false => subject,
    };
    Ok(saved.exports(connection)?.join(format!(
        "{}-{}",
        dump::slug(&subject, "dump"),
        dumps::stamp()
    )))
}
