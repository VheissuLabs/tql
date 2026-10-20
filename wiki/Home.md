![tql browsing a database: filtering a column, sorting, inspecting a row and its related records, and writing a join in the SQL editor](https://raw.githubusercontent.com/VheissuLabs/tql/main/docs/demo.gif)

tql (pronounced *TEE-kwuhl*, like sequel) is a database client for the terminal,
for MySQL, MariaDB, PostgreSQL, SQL Server, SQLite, DuckDB, libSQL and Turso,
ClickHouse, Snowflake, Redis and Valkey, and the hosted databases that speak
their protocol. It is written in Rust and ships as one file.

## Why

The database is the part of the job you cannot see. Everything else is in the
terminal already (the editor, the logs, the deploy) and then the schema is
behind a window you have to go and find, in an application that costs money and
knows nothing about the rest of your work.

tql is that window, in the terminal, and it is built around three things a GUI
tends not to do:

- **It shows you the query.** The SQL pane always holds the statement that
  produced what is on screen. Click a header to sort and the `order by` appears.
  Filter a column and the `where` appears, with its value bound, never glued in.
  You learn the language by using the tool.
- **It is the same engine for you and for an agent.** The `tql query` commands,
  the MCP server and the interface all go through the same code, so what your
  agent can see is what you can see. The agent's side only reads, and every
  statement either of you runs lands in one shared history. An agent can also
  put a statement in your SQL editor for you to read and run.
- **It makes production look like production.** A connection carries a tag and
  the tag carries a color, so the screen tells you where you are before you
  press `d`. Read-only connections refuse every write. Edits and deletions are
  pending until you type `:w`.

It is one binary, no configuration required to start, and it opens a SQLite file
as happily as a Postgres server behind an SSH tunnel.

## Install it

```bash
brew install vheissulabs/tap/tql            # macOS and Linux, Homebrew
curl -fsSL https://raw.githubusercontent.com/VheissuLabs/tql/main/install.sh | sh   # a script
```

There are `.deb`, `.rpm` and `PKGBUILD` packages, and standalone binaries, on
[the latest release](https://github.com/VheissuLabs/tql/releases/latest). See
[Installation](Installation) for all of them, including how to check a download.

Then run `tql` on its own to pick a connection, or point it straight at a
database:

```bash
tql                                        # the connection list
tql ./database.sqlite                      # a SQLite file
tql 'mysql://user:pw@db.example.com:3306/shop'
```

## Databases

| Database | |
| --- | --- |
| **MySQL** and **MariaDB** | built in |
| **PostgreSQL** | built in |
| **SQLite** | built in, any `.sqlite`, `.sqlite3` or `.db` file |
| **SQL Server** | built in, nothing else to install |
| **DuckDB** | built in, any `.duckdb` file, schemas and all |
| **libSQL** and **Turso** | built in, a `libsql://` URL and its auth token ([Connecting](Connecting#libsql-and-turso)) |
| **ClickHouse** | built in, browse, query and edit ([Connecting](Connecting#clickhouse)) |
| **Snowflake** | built in, with an access token, a key pair or a browser sign-in ([Connecting](Connecting#snowflake)) |
| **Redis** and **Valkey** | built in, a key browser of their own ([Redis and Valkey](Redis-and-Valkey)) |

A hosted database that speaks the MySQL or PostgreSQL protocol connects the same
way, with a connection string or the connection form, over TLS and, if it needs
one, an SSH tunnel:

- **MySQL protocol:** Laravel Cloud, Amazon RDS and Aurora, Google Cloud SQL,
  Azure Database, DigitalOcean, PlanetScale, TiDB, SingleStore
- **PostgreSQL protocol:** Neon, Supabase, Amazon RDS and Aurora, Google Cloud
  SQL, Azure Database, DigitalOcean, Crunchy Bridge, Timescale, CockroachDB

## The wiki

| Page | |
| --- | --- |
| [Installation](Installation) | every way to install and update tql |
| [Connecting](Connecting) | connection strings, the connection form, tags, groups, SSH and TLS |
| [Browsing data](Browsing-Data) | the grid, sorting, filtering, following links, structure |
| [Editing data](Editing-Data) | cells, row forms, adding, pending changes, deleting |
| [Redis and Valkey](Redis-and-Valkey) | the key browser: namespaces, every type, TTLs, the command pane |
| [The row inspector](Row-Inspector) | a row's fields and its related records |
| [The SQL editor](SQL-Editor) | simple and vim styles, history, completion |
| [Asking for SQL](Asking-for-SQL) | ask in english, and which model answers |
| [Exporting](Exporting) | rows out as re-importable SQL |
| [Dump, load and sync](Dump-Load-and-Sync) | whole databases, with mydumper, pg_dump and the rest |
| [Commands](Commands) | every `tql` subcommand and flag |
| [Keys](Keys) | every key, and how to rebind them |
| [Configuration](Configuration) | every setting in `config.toml` |
| [For agents](For-Agents) | the `tql` commands and the MCP server |
| [Troubleshooting](Troubleshooting) | when something is not where you expected it |
| [Developing](Developing) | where the code lives, and the drawing rules |
| [Packaging and releasing](Packaging-and-Releasing) | the binary, the packages, the tap and the AUR |
| [Changelog](Changelog) | what changed in each release |

## A database to try it on

The sample database used in development is [Chinook](https://github.com/lerocha/chinook-database),
which is not committed to the repository. Fetch it and open it:

```bash
curl -L -o chinook.db https://github.com/lerocha/chinook-database/raw/master/ChinookDatabase/DataSources/Chinook_Sqlite.sqlite
tql chinook.db
```
