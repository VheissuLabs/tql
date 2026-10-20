What changed in each release. The full file lives in the repository and is always
up to date:

- [CHANGELOG.md](https://github.com/VheissuLabs/tql/blob/main/CHANGELOG.md)
  — every release, grouped into New Features, Enhancements and Bug Fixes.

Fuller announcements for the recent releases live under `release-notes/`:

| Release | |
| --- | --- |
| [0.16.0](https://github.com/VheissuLabs/tql/blob/main/release-notes/v0.16.0.md) | Snowflake single sign-on, and editing Snowflake and ClickHouse tables in the grid |
| [0.15.0](https://github.com/VheissuLabs/tql/blob/main/release-notes/v0.15.0.md) | Snowflake, over its SQL API with a programmatic access token |
| [0.14.2](https://github.com/VheissuLabs/tql/blob/main/release-notes/v0.14.2.md) | `tql connect`: open a saved connection by name, straight from the shell |
| [0.14.1](https://github.com/VheissuLabs/tql/blob/main/release-notes/v0.14.1.md) | Fixes for SQL Server, ClickHouse and Redis/Valkey |
| [0.14.0](https://github.com/VheissuLabs/tql/blob/main/release-notes/v0.14.0.md) | DuckDB, libSQL and Turso, ClickHouse, Redis and Valkey; schemas; schema-aware completion |
| [0.13.2](https://github.com/VheissuLabs/tql/blob/main/release-notes/v0.13.2.md) | More of tql's shell commands, from inside tql (`:update`, `:open`, `:export database`, `:v`, `:help`) |
| [0.13.1](https://github.com/VheissuLabs/tql/blob/main/release-notes/v0.13.1.md) | Show or hide the SQL editor (`\|`, `toggle_sql`), and wide-character fixes |
| [0.13.0](https://github.com/VheissuLabs/tql/blob/main/release-notes/v0.13.0.md) | A settings screen, and a command palette everywhere |
| [0.12.2](https://github.com/VheissuLabs/tql/blob/main/release-notes/v0.12.2.md) | Read-only enforced in more places, wide characters, export paging |
| [0.12.1](https://github.com/VheissuLabs/tql/blob/main/release-notes/v0.12.1.md) | `tql -v` prints the version |
| [0.12.0](https://github.com/VheissuLabs/tql/blob/main/release-notes/v0.12.0.md) | The Rust rewrite: the same tql, a third of the size |
| [0.11.0](https://github.com/VheissuLabs/tql/blob/main/release-notes/v0.11.0.md) | `tql dump`, `tql load` and `tql sync`, and the TablePlus import |
| [0.9.0](https://github.com/VheissuLabs/tql/blob/main/release-notes/v0.9.0.md) | SQL Server on macOS, and the agent commands |
| [0.6.0](https://github.com/VheissuLabs/tql/blob/main/release-notes/v0.6.0.md) | The releases before 0.9 |

## 0.16.0

- Snowflake signs in through the browser when no access token is saved, with
  Snowflake's sign-in page or your company's single sign-on behind it. The
  sign-in is saved with the connection and renewed, so the next launch, `tql
  dump`, `tql export` and the MCP tools need no browser.
- Snowflake and ClickHouse tables are edited in the grid. A Snowflake change
  commits only if exactly the rows you edited changed; ClickHouse counts the
  rows first and waits for the mutation. Tables with no primary key are
  edited too, matching the row on its values, and a match that fits more than
  one row changes nothing.

## 0.15.0

- Snowflake connections over its SQL API with a programmatic access token:
  browse and query, schemas, declared keys for links and paging, and every
  type shown as Snowflake writes it. Tables, columns and keys come from `SHOW`
  commands, which do not wake the warehouse.
- The connection form asks a Snowflake connection for its Account, Database,
  Access token, Warehouse and Role.

## 0.14.2

- `tql connect <connection>` opens the saved connection you name, skipping the
  list; `--db=<database>` opens another database on the same server for that
  session.

## 0.14.0

- Four new kinds of database: DuckDB files, libSQL and Turso, ClickHouse
  (browse and query), and Redis and Valkey in a key browser of their own. See
  [Connecting](Connecting) and [Redis and Valkey](Redis-and-Valkey).
- Postgres and SQL Server list every schema under folding headers, and a table
  outside your default schema is `schema.table` everywhere.
- Completion offers schemas and their tables, and the columns of
  `schema.table` ([#5](https://github.com/VheissuLabs/tql/issues/5)); `↵` takes
  a suggestion as `tab` does, and `sql_keyword_case` sets the case keywords are
  written in.
- The status line shows where you are among all the rows, `8,501 of 50,000`,
  estimated (`~4.9M`) on a big table that would be slow to count.
- The insert-mode cursor in the SQL editor sits right after the text.

## 0.13.2

- `:update` checks for a newer tql and installs it in the background, on either
  screen, with the result in the status bar.
- `:open <path or connection string>` opens a SQLite file or a connection string,
  saving it as `tql open` does and reusing one already saved.
- `:export database` writes every table in the current database to one `.sql`
  file.
- `:v` / `:version` say which version of tql is running, and `:help` / `:h` open
  help.

## 0.13.0

- A settings screen: `tql settings`, `:settings`, or Settings in the ctrl+k
  palette. Every setting sits under a section that folds, takes effect at once,
  and is written to `config.toml` straight away, that one line, your comments and
  order kept.
- A ctrl+k palette on the connection list, with its commands and every connection
  by name.
- `tql new`, `tql sql [connection]` and `tql history [connection]` open tql on the
  screen the command belongs to.

## 0.12.0

The Rust rewrite. It is the same tql: the same screens, keys, commands, config and
connections, checked frame by frame against 0.11. It is also a third of the size
(11 MB on a Mac, down from 35), starts in about 7 ms instead of 170, needs nothing
else installed, and SQL Server is built in on macOS and Linux. Everything that
behaves differently from 0.11 on purpose is listed in `parity/deliberate.md`.

Bringing your existing install over takes nothing: connections, passwords, settings
and history carry over from the same `~/.config/tql`. See
[Installation](Installation#coming-from-tql-011-or-earlier).