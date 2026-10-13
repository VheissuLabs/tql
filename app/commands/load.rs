use std::path::{Path, PathBuf};
use std::time::Instant;

use serde_json::Value;

use crate::commands::dumps::{self, Saved};
use crate::console::prelude::*;
use crate::database;
use crate::driver::Driver;
use crate::dump::{self, Engine, LoadOptions, StatementReader};
use crate::store::Connection;

pub struct Load;

impl Command for Load {
    fn signature(&self) -> &str {
        "load {source : A folder from tql dump, or a .sql file from tql export} {connection : The tql connection to load it into} {--database= : Which database on the server, for a connection that names none} {--drop : Drop and recreate tables that already exist} {--threads=4 : How many tables, or pieces of a table, to load at once} {--force : Load into a production connection without asking} {--dry-run : Show the command tql would run, and stop}"
    }

    fn description(&self) -> &str {
        "Load a dump folder or an exported .sql file into a database"
    }

    fn aliases(&self) -> &[&'static str] {
        &["import"]
    }

    fn help(&self) -> &str {
        r#"Reads back what <fg=green>tql dump</> wrote, with myloader for MySQL and pg_restore for
Postgres, or replays a <fg=green>tql export</> file in one transaction.

Tables that already exist stop the load, unless <fg=green>--drop</> says to replace them.

<fg=yellow>Examples</>

  <fg=green>tql load ~/.config/tql/exports/notarydash-20261007-091200 local</>
      a dump into the local connection

  <fg=green>tql load ./nd local --drop</>
      the same, replacing tables local already has

  <fg=green>tql load ./orders.sql local</>
      an export's rows into tables that exist"#
    }

    fn execute(&self, cmd: &mut Cmd) -> Result {
        let source = resolved(cmd.argument("source").unwrap_or_default());
        if !source.exists() {
            cmd.error(format!("There is nothing at {}.", source.display()));
            return exit(1);
        }

        let saved = Saved::open(cmd);
        let name = cmd.argument("connection").unwrap_or_default().to_string();
        let Some(mut connection) = saved.find(cmd, &name) else {
            return exit(1);
        };
        if !dumps::settle_database(cmd, &mut connection) || !dumps::may_write_to(cmd, &connection)?
        {
            return exit(1);
        }

        match source.is_file() {
            true => replay(cmd, &saved, &source, &connection),
            false => restore(cmd, &saved, &source, &connection),
        }
    }
}

fn resolved(source: &str) -> PathBuf {
    let resolved = crate::paths::resolve(
        source,
        &crate::paths::home(),
        &std::env::current_dir().unwrap_or_default(),
    );
    let trimmed = resolved.trim_end_matches('/');
    PathBuf::from(match trimmed.is_empty() {
        true => "/",
        false => trimmed,
    })
}

fn restore(cmd: &mut Cmd, saved: &Saved, directory: &Path, connection: &Connection) -> Result {
    let Some(source) = Engine::recognise(directory) else {
        cmd.error(format!(
            "{} does not look like a dump from tql dump, mydumper or pg_dump.",
            directory.display()
        ));
        return exit(1);
    };
    let Some(engine) = dumps::engine_for(cmd, connection.driver) else {
        return exit(1);
    };
    if engine != source {
        cmd.error(format!(
            "That dump is from {} and {} is {}; tql does not move between engines yet.",
            source.label(),
            connection.name,
            engine.label()
        ));
        return exit(1);
    }
    if !dumps::tool_is_installed(cmd, engine.loader(), engine) {
        return exit(1);
    }
    if connection.driver != Driver::Sqlite
        && let Some(failure) = saved.test(&dumps::server_of(connection))
    {
        cmd.error(failure);
        return exit(1);
    }

    let options = LoadOptions {
        drop: cmd.flag("drop"),
        threads: dumps::threads(cmd),
    };
    let job = match engine.load(connection, directory, &options, saved.encrypter()) {
        Ok(job) => job,
        Err(error) => {
            cmd.error(format!("{error:#}"));
            return exit(1);
        }
    };

    let what = match dump::read_manifest(directory) {
        None => dumps::shorten(directory),
        Some(manifest) => {
            let text = |key: &str, otherwise: &str| match manifest.get(key) {
                None | Some(Value::Null) => otherwise.to_string(),
                Some(Value::String(text)) => text.clone(),
                Some(other) => other.to_string(),
            };
            format!(
                "{} from {}",
                text("database", "the dump"),
                text("connection", "somewhere")
            )
        }
    };
    let replacing = match options.drop {
        true => ", replacing tables it already has",
        false => "",
    };
    cmd.marked(&format!(
        "  Loading {what} into <fg=cyan>{}</>{replacing}",
        connection.name
    ));

    if cmd.flag("dry-run") {
        let what = match job.native {
            None => job.describe(),
            Some(_) => format!(
                "a SQLite restore into {}",
                connection.database.as_deref().unwrap_or_default()
            ),
        };
        cmd.line(format!("  {what}"));
        return Ok(());
    }

    let started = Instant::now();
    if !dumps::run_quietly_failing(cmd, job, "Loading") {
        cmd.error("The load did not finish.");
        if !options.drop {
            cmd.marked("  if a table already exists, <fg=green>--drop</> replaces it.");
        }
        return exit(1);
    }

    cmd.marked(&format!(
        "  <fg=green>Loaded</> in {}",
        dump::elapsed(started.elapsed().as_secs_f64())
    ));
    Ok(())
}

fn replay(cmd: &mut Cmd, saved: &Saved, file: &Path, connection: &Connection) -> Result {
    if cmd.flag("drop") {
        cmd.error(
            "--drop is for dump folders; a .sql file from tql export goes into tables that exist.",
        );
        return exit(1);
    }

    let mut database = match database::connect(connection, saved.encrypter()) {
        Ok(database) => database,
        Err(error) => {
            cmd.error(format!("{error:#}"));
            return exit(1);
        }
    };

    let atomic = connection.driver != Driver::Clickhouse;
    cmd.marked(&format!(
        "  Replaying {} into <fg=cyan>{}</>, {}",
        dumps::shorten(file),
        connection.name,
        match atomic {
            true => "in one transaction",
            false => "one statement at a time, since ClickHouse has no transactions",
        }
    ));
    if cmd.flag("dry-run") {
        return Ok(());
    }

    let started = Instant::now();
    let (begin, commit, rollback) = match connection.driver {
        Driver::Sqlsrv => (
            "begin transaction",
            "commit transaction",
            "rollback transaction",
        ),
        _ => ("begin", "commit", "rollback"),
    };
    let mut statements = 0;

    let mut replay = || -> anyhow::Result<()> {
        if atomic {
            database.run(begin)?;
        }
        for statement in StatementReader::for_driver(connection.driver).read(file)? {
            database.run(&statement?)?;
            statements += 1;
        }
        if atomic {
            database.run(commit)?;
        }
        Ok(())
    };
    let replayed = replay();

    if let Err(error) = replayed {
        if atomic {
            let _ = database.run(rollback);
        }
        cmd.error(match atomic {
            true => format!(
                "Statement {} failed, so nothing was loaded: {error:#}",
                statements + 1
            ),
            false => format!(
                "Statement {} failed; the {statements} before it were loaded: {error:#}",
                statements + 1
            ),
        });
        return exit(1);
    }

    cmd.marked(&format!(
        "  <fg=green>Loaded</> {statements} statements in {}",
        dump::elapsed(started.elapsed().as_secs_f64())
    ));
    Ok(())
}
