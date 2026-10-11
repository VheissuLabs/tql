use std::path::Path;

use super::ago;

pub const LIMIT: usize = 200;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Run {
    pub statement: String,
    pub source: String,
    pub succeeded: bool,
    pub rows: Option<usize>,
    pub at: String,
}

pub struct Execution<'a> {
    pub connection_id: i64,
    pub statement: &'a str,
    pub error: Option<&'a str>,
    pub rows: Option<usize>,
    pub duration_ms: u128,
}

pub fn record(store: &Path, source: &str, execution: Execution) {
    if execution.connection_id == 0 {
        return;
    }
    let Ok(connection) = rusqlite::Connection::open(store) else {
        return;
    };
    let _ = connection.execute(
        "insert into query_executions \
         (connection_id, statement, source, succeeded, error, row_count, duration_ms, created_at, updated_at) \
         values (?1, ?2, ?7, ?3, ?4, ?5, ?6, datetime('now'), datetime('now'))",
        (
            execution.connection_id,
            execution.statement,
            execution.error.is_none(),
            execution.error,
            execution.rows.map(|rows| rows as i64),
            execution.duration_ms as i64,
            source,
        ),
    );
}

pub fn of(store: &Path, connection_id: i64) -> Vec<Run> {
    let read = || -> rusqlite::Result<Vec<Run>> {
        let connection = rusqlite::Connection::open_with_flags(
            store,
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
        )?;
        let mut statement = connection.prepare(
            "select statement, source, succeeded, row_count, created_at from query_executions \
             where id in (select max(id) from query_executions where connection_id = ?1 \
             and source in ('editor', 'mcp', 'cli') group by statement) \
             order by id desc limit ?2",
        )?;
        let runs = statement.query_map((connection_id, LIMIT as i64), |row| {
            Ok(Run {
                statement: row.get(0)?,
                source: row.get(1)?,
                succeeded: row.get(2)?,
                rows: row.get::<_, Option<i64>>(3)?.map(|rows| rows as usize),
                at: row.get::<_, Option<String>>(4)?.unwrap_or_default(),
            })
        })?;
        runs.collect()
    };
    read().unwrap_or_default()
}

pub fn one_line(statement: &str) -> String {
    statement.split_whitespace().collect::<Vec<_>>().join(" ")
}

pub fn hint(run: &Run, now: i64) -> String {
    let source = (run.source != "editor").then(|| run.source.clone());
    let outcome = match (run.succeeded, run.rows) {
        (false, _) => Some("failed".to_string()),
        (true, None) => None,
        (true, Some(1)) => Some("1 row".to_string()),
        (true, Some(rows)) => Some(format!("{rows} rows")),
    };
    let at = Some(ago::short(&run.at, now)).filter(|at| !at.is_empty());
    [source, outcome, at]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join(" · ")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store(name: &str) -> std::path::PathBuf {
        let path =
            std::env::temp_dir().join(format!("tql-history-{}-{name}.sqlite", std::process::id()));
        let _ = std::fs::remove_file(&path);
        rusqlite::Connection::open(&path)
            .unwrap()
            .execute_batch(
                "create table query_executions (id integer primary key autoincrement, connection_id integer, \
                 statement text, source text, succeeded integer, error text, row_count integer, \
                 duration_ms integer, created_at text, updated_at text);",
            )
            .unwrap();
        path
    }

    fn ran(store: &Path, connection_id: i64, statement: &str, source: &str) {
        rusqlite::Connection::open(store)
            .unwrap()
            .execute(
                "insert into query_executions (connection_id, statement, source, succeeded, row_count, created_at) \
                 values (?1, ?2, ?3, 1, 1, datetime('now'))",
                (connection_id, statement, source),
            )
            .unwrap();
    }

    #[test]
    fn lists_what_people_ran_newest_first_once_each_and_leaves_out_what_tql_ran() {
        let path = store("lists");
        ran(&path, 1, "select 1", "editor");
        ran(&path, 1, "select * from \"fruit\" limit 50 offset 0", "tui");
        ran(&path, 1, "select 2", "mcp");
        ran(&path, 1, "select 3", "cli");
        ran(&path, 2, "select 4", "editor");
        ran(&path, 1, "select 1", "editor");

        let runs = of(&path, 1);
        std::fs::remove_file(&path).unwrap();

        let statements: Vec<_> = runs.iter().map(|run| run.statement.as_str()).collect();
        assert_eq!(statements, ["select 1", "select 3", "select 2"]);
        assert!(hint(&runs[1], ago::now()).starts_with("cli · 1 row · "));
    }

    #[test]
    fn records_what_the_editor_ran_and_whether_it_failed() {
        let path = store("records");
        record(
            &path,
            "editor",
            Execution {
                connection_id: 3,
                statement: "select nope",
                error: Some("no such column: nope"),
                rows: None,
                duration_ms: 2,
            },
        );

        let runs = of(&path, 3);
        std::fs::remove_file(&path).unwrap();

        assert_eq!(runs.len(), 1);
        assert!(hint(&runs[0], ago::now()).starts_with("failed · "));
    }

    #[test]
    fn reads_a_statement_on_one_line() {
        assert_eq!(
            one_line("select name\n  from fruit "),
            "select name from fruit"
        );
    }
}
