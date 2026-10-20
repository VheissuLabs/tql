Running `tql` with nothing after it opens the connection list, which is how you
will use it nearly all the time. The rest are for the things a full-screen
interface is the wrong shape for.

```bash
tql list        # shows all of these
```

## Top-level commands

| Command | What it does |
| --- | --- |
| `tql` | the interface: pick a connection and browse |
| `tql connect <connection> [--db=<database>]` | open a saved connection by name, skipping the list; `--db` picks another database on the same server |
| `tql open <path-or-dsn>` | open a database by path or connection string, saving it; `tql connect` takes these too |
| `tql export [connection] [table]` | write rows out as re-importable SQL |
| `tql tableplus` | bring your TablePlus connections over, groups and passwords too |
| `tql dump <connection> [tables…]` | dump a database, schema and data, with mydumper or pg_dump |
| `tql load <dump-or-sql> <connection>` | load a dump, or replay an export; `tql import` is the same |
| `tql sync <from> <to>` | make one database a copy of another, a dump and a load in one step |
| `tql settings` | change your settings on a screen, saved as you go |
| `tql new` | open tql on a new connection form |
| `tql sql [connection]` | open tql in the SQL editor; without a connection, tql asks |
| `tql history [connection]` | open tql on the query history |
| `tql config` | where the config file is; `--tidy` puts it back in order |
| `tql connections` | list the saved connections |
| `tql tables <connection>` | list a connection's tables |
| `tql describe <connection> <table>` | a table's columns, types and primary key |
| `tql query <connection> "select …"` | run a read-only query; `-` reads it from stdin |
| `tql mcp:start tql` | run the MCP server on stdio, for an agent |
| `tql update` | update to the latest release now; see [`[updates]`](Configuration#updates) |
| `tql -v` | the version you have; `-V` and `--version` say the same |

Flags for `open` and `tableplus`, and the full flag list for
`dump`/`load`/`sync`, are on [Connecting](Connecting) and
[Dump, load and sync](Dump-Load-and-Sync). `tql export` is on
[Exporting](Exporting).

A debug build from a source checkout also shows `make:command`, which writes the
skeleton of a new command. tql runs its store's migrations for itself, so there
is no command for them.

## In the interface

`:open`, `:update`, `:export` (this table) and `:export database` (every table)
do the same from the command line or the `ctrl+k` palette, and `:v` says which
version you have.

| Command | Action |
| --- | --- |
| `:q` `:quit` | quit |
| `:c` `:connections` | back to the connection list |
| `:w` `:write` | write pending edits and deletions |
| `:r` `:reload` | reload the table; from the SQL editor, `:r` runs the statement |
| `:run` | run the SQL editor's statement |
| `:sql` | open the SQL editor |
| `:history` | the query history |
| `:tables` | focus the table list |
| `:rows` | focus the rows |
| `:export` | write this table to a `.sql` file |
| `:export database` | write every table in this database to one `.sql` file |
| `:create database <name>` | create a database on this server and switch to it |
| `:drop database <name>` | drop a database on this server; drop the one in use and the database list opens |
| `:settings` | change your settings |
| `:v` `:version` | show which version of tql this is |
| `:update` | check for a newer tql and install it, while you keep working |
| `:open <path or connection string>` | open a SQLite or DuckDB file or a `mysql://`, `pgsql://`, `sqlsrv://`, `libsql://`, `clickhouse://`, `snowflake://`, `redis://` or `valkey://` URL, saved to the connection list as `tql open` does |
| `:h` `:help` | the keys and commands |
| `:sign-out` | on the connection list, forget the Snowflake browser sign-in saved with the highlighted connection |

A command that belongs to the other screen says so: `:sql` on the connection list
says it works once a connection is open. Every command here that needs the screen
also runs from the shell and opens tql on it: `tql settings`, `tql sql shop`,
`tql history shop`, `tql new`.

## The command palette

`ctrl+k` opens one list of everything you can do from here: every action, with the
key that does it, the `:` commands that have no key, the tables, the other
databases on a server, and your other connections. Type to narrow it (exact and
prefix matches first, then words, then letters in order) and `↵` runs the one you
are on, exactly as its key would.

The key shown beside an action is the one it is bound to, so a key you rebind is
the one it offers. `palette = "ctrl+p"` in [`[keys]`](Keys#rebinding) moves the
palette itself.

## Environment variables

| Variable | Effect |
| --- | --- |
| `TQL_BIN_DIR` | where the install script puts the binary |
| `TQL_VERSION` | pin the version the install script fetches |
| `TQL_UPDATE_CHECK=false` | stop the daily update check for one run |
| `TQL_AI_PROVIDER`, `TQL_AI_MODEL`, `TQL_AI_TIMEOUT`, `TQL_AI_URL`, `TQL_AI_KEY` | overrides for [`[ai]`](Configuration#ai) |
| `XDG_CONFIG_HOME` | move `~/.config/tql/`, which is how the tests stay out of your real connections |
