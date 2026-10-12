use std::path::{Path, PathBuf};

use crate::console::prelude::*;
use crate::driver::Driver;

use crate::config_file;
use crate::database::{self, Database, Exporter, Table};
use crate::encrypter::Encrypter;
use crate::paths;
use crate::settings;
use crate::store::{Connection, Store};

const EVERY_TABLE: &str = "every table";

pub struct Export;

impl Command for Export {
    fn signature(&self) -> &str {
        "export {connection? : The tql connection name, asked for when left out} {table? : The table to export, or every table when omitted} {--sql= : Where to write the file, a path or a directory} {--database= : Which database on the server, for a connection that names none} {--limit= : Export at most this many rows per table} {--list : List the tables in the connection and stop}"
    }

    fn description(&self) -> &str {
        "Export a table, or a whole database, to re-importable SQL"
    }

    fn help(&self) -> &str {
        r#"Writes rows as <fg=cyan>insert</> statements you can replay into another database.
Data only, no schema, so the target table must already exist.

<fg=yellow>Examples</>

  <fg=green>tql export</>
      asks which connection and which table

  <fg=green>tql export prod orders --limit=1000 --sql=./orders.sql</>
      a thousand rows of one table into a named file

  <fg=green>tql export prod --sql=./prod.sql</>
      every table in the database, into one file

  <fg=green>tql export prod orders</>
      auto-named file in ~/.config/tql/exports

  <fg=green>tql export prod --list</>
      just show which tables are there

<fg=yellow>Notes</>

  --sql takes a file path or a directory.
  --limit applies per table, so it bounds a whole-database export too.
  Rows are read in chunks, so a large table does not go through memory at once."#
    }

    fn execute(&self, cmd: &mut Cmd) -> Result {
        export(cmd)
    }
}

fn export(cmd: &mut Cmd) -> Result {
    let loaded = config_file::prepare(&paths::config_file());
    if let Some(error) = &loaded.error {
        cmd.warn(error);
    }
    settings::install(loaded.settings);

    let store = paths::store();
    let connections = Store::open(&store)
        .and_then(|opened| opened.connections())
        .unwrap_or_default();
    let Some((mut connection, asked)) = chosen_connection(cmd, connections)? else {
        return exit(1);
    };

    let Some(ask_for_database) = named_database(cmd, &mut connection) else {
        return exit(1);
    };

    let encrypter = Encrypter::from_file(&paths::key()).ok();
    let mut database = match database::connect(&connection, encrypter.as_ref()) {
        Ok(database) => database,
        Err(error) => {
            cmd.error(format!("{error:#}"));
            return exit(1);
        }
    };

    if ask_for_database && !ask_database(cmd, database.as_mut(), &mut connection)? {
        return exit(1);
    }

    let available = match database.tables() {
        Ok(tables) => tables,
        Err(error) => {
            cmd.error(format!("Could not read the schema: {error:#}"));
            return exit(1);
        }
    };

    if cmd.flag("list") {
        for table in &available {
            cmd.line(format!("  {table}"));
        }
        return Ok(());
    }

    let mut typed = cmd.argument("table").map(String::from);
    if typed.is_none() && asked {
        typed = choose_table(cmd, &available)?;
    }

    let default_schema = database.default_schema();
    let table = match typed {
        Some(typed) => match Table::resolve(&typed, &available, default_schema.as_deref()) {
            Some(table) => Some(table),
            None => {
                cmd.error(format!(
                    "No table named [{typed}] in [{}].",
                    connection.name
                ));
                let known: Vec<String> = available.iter().map(Table::to_string).collect();
                cmd.line(format!("  known: {}", known.join(", ")));
                return exit(1);
            }
        },
        None => None,
    };

    let tables = table.map(|table| vec![table]).unwrap_or(available);
    if tables.is_empty() {
        cmd.error("That connection has no tables.");
        return exit(1);
    }

    let limit = cmd
        .option("limit")
        .map(|limit| limit.trim().parse::<i64>().unwrap_or_default().max(1) as usize);

    let exporter = Exporter::new(&connection, Some(&store));
    let destination = destination(cmd, &exporter, &tables, asked)?;
    let exported = match exporter.tables(database.as_mut(), &tables, destination, limit) {
        Ok(exported) => exported,
        Err(error) => {
            cmd.error(format!("Export failed: {error:#}"));
            return exit(1);
        }
    };

    let what = match tables.as_slice() {
        [only] => only.to_string(),
        many => format!("{} tables", many.len()),
    };
    cmd.line(format!(
        "  {} rows from {what} ({}) in {}ms",
        exported.rows,
        exported.size(),
        exported.duration_ms
    ));
    cmd.line(format!("  {}", exported.path.display()));
    Ok(())
}

fn chosen_connection(
    cmd: &mut Cmd,
    connections: Vec<Connection>,
) -> Result<Option<(Connection, bool)>> {
    if let Some(name) = cmd.argument("connection") {
        let name = name.to_string();
        if let Some(found) = connections
            .iter()
            .find(|connection| connection.name == name)
        {
            return Ok(Some((found.clone(), false)));
        }
        let mut known: Vec<String> = connections
            .into_iter()
            .map(|connection| connection.name)
            .collect();
        known.sort();
        cmd.error(format!("No connection named [{name}]."));
        cmd.line(format!("  known: {}", known.join(", ")));
        return Ok(None);
    }

    if connections.is_empty() {
        cmd.error("No saved connections.");
        cmd.line("  tql open <path-or-dsn> saves one.");
        return Ok(None);
    }

    if !cmd.is_interactive() {
        cmd.error("Which connection? Name one, or run it in a terminal to be asked.");
        let known: Vec<&str> = connections
            .iter()
            .map(|connection| connection.name.as_str())
            .collect();
        cmd.line(format!("  known: {}", known.join(", ")));
        return Ok(None);
    }

    let labels: Vec<String> = connections
        .iter()
        .map(|connection| format!("{}  ·  {}", connection.name, connection.describe()))
        .collect();
    let answer = cmd.select("Export from", &labels, None)?;
    let chosen = labels
        .iter()
        .position(|label| *label == answer)
        .map(|index| connections[index].clone());
    Ok(chosen.map(|connection| (connection, true)))
}

fn named_database(cmd: &mut Cmd, connection: &mut Connection) -> Option<bool> {
    let named = cmd.option("database").map(String::from);

    if connection.driver == Driver::Sqlite {
        if named.is_some() {
            cmd.error("A sqlite connection is one file; --database means nothing to it.");
            return None;
        }
        return Some(false);
    }

    if let Some(named) = named {
        connection.database = Some(named);
        return Some(false);
    }

    let unnamed = connection
        .database
        .as_deref()
        .is_none_or(|database| database.trim().is_empty());
    if !unnamed {
        return Some(false);
    }

    if !cmd.is_interactive() {
        cmd.error(format!(
            "[{}] is a server, not a database.",
            connection.name
        ));
        cmd.line("  name one with --database=.");
        return None;
    }
    Some(true)
}

fn ask_database(
    cmd: &mut Cmd,
    database: &mut dyn Database,
    connection: &mut Connection,
) -> Result<bool> {
    let databases = database.databases().unwrap_or_default();
    if databases.is_empty() {
        cmd.error("That server offered no databases.");
        return Ok(false);
    }

    let chosen = cmd.select("Which database?", &databases, None)?;
    if let Err(error) = database.use_database(&chosen) {
        cmd.error(format!("{error:#}"));
        return Ok(false);
    }
    connection.database = Some(chosen);
    Ok(true)
}

fn choose_table(cmd: &mut Cmd, available: &[Table]) -> Result<Option<String>> {
    let every = format!("{EVERY_TABLE} ({})", available.len());
    let options: Vec<String> = std::iter::once(every.clone())
        .chain(available.iter().map(Table::to_string))
        .collect();
    let chosen = if available.len() <= 15 {
        cmd.select("Which table?", &options, None)?
    } else {
        cmd.search(
            "Which table?",
            &options,
            1,
            "type to filter, or pick the first for all of them",
        )?
    };
    Ok((chosen != every).then_some(chosen))
}

fn folder_of(path: &Path) -> PathBuf {
    match path.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => parent.to_path_buf(),
        _ => PathBuf::from("."),
    }
}

fn ask_where(cmd: &mut Cmd, exporter: &Exporter, tables: &[Table]) -> Result<Option<String>> {
    let only = (tables.len() == 1).then(|| &tables[0]);
    let suggested = exporter.filename(only)?.display().to_string();
    loop {
        let answer = cmd.ask_with_hint(
            "Save it where?",
            &paths::shorten(&suggested),
            "a file, or a folder to have it named for you",
        )?;
        let answer = answer.trim().to_string();
        let wanted = if answer.is_empty() {
            suggested.clone()
        } else {
            answer.clone()
        };
        let folder = folder_of(Path::new(&paths::expand(&wanted)));
        if folder.is_dir() {
            return Ok((!answer.is_empty()).then_some(answer));
        }
        cmd.error(format!("There is no folder [{}].", folder.display()));
    }
}

fn destination(
    cmd: &mut Cmd,
    exporter: &Exporter,
    tables: &[Table],
    asked: bool,
) -> Result<Option<PathBuf>> {
    let sql = match cmd.option("sql") {
        Some(sql) => Some(sql.to_string()),
        None if asked => ask_where(cmd, exporter, tables)?,
        None => None,
    };
    let Some(sql) = sql else {
        return Ok(None);
    };

    let path = PathBuf::from(paths::expand(&sql));
    if !path.is_dir() {
        return Ok(Some(path));
    }
    let only = (tables.len() == 1).then(|| &tables[0]);
    let named = exporter.filename(only)?;
    Ok(Some(path.join(named.file_name().unwrap_or_default())))
}
