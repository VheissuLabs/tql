# DuckDB, libSQL/Turso and ClickHouse

Date: 2026-10-12
Status: proposed
Release: 0.14.0, second of three parts (schemas, then these drivers, then
Redis/Valkey)

## Goal

Three more SQL engines, each working everywhere a SQL connection works today:
the connection form, `tql open`, the connection list, the browser, the SQL
editor and completion, structure, filters, export, the agent and MCP, tags and
read-only.

| Engine | What it is | How tql talks to it |
| --- | --- | --- |
| DuckDB | an analytics database in a single file | in process, as SQLite is |
| libSQL / Turso | SQLite served over the network | HTTP, Turso's pipeline API |
| ClickHouse | a column-store analytics server | HTTP, its own interface |

## Decisions already made

- ClickHouse is browse and query only. Its updates are `ALTER TABLE … UPDATE`
  mutations that run later in the background, which does not fit editing and
  then `:w`. Cell editing, row editing, adding and deleting rows say so for a
  ClickHouse connection.
- DuckDB and libSQL edit like SQLite does.
- Schemas (the first part) land first. DuckDB has schemas and uses them.

## Out of scope

- Dump, load and sync for these engines. `tql dump`, `tql load` and `tql sync`
  say the engine is not supported, as they do for SQLite.
- ClickHouse's native TCP protocol (port 9000). The HTTP interface (8123, or
  8443 with TLS) covers what tql needs.
- Embedded replicas for libSQL. A local libSQL file is a SQLite file and opens
  as one already.
- MotherDuck and DuckDB extensions beyond what loads by itself.

## What the user sees

### Adding a connection

The driver choice on the connection form grows to:

| Driver | Fields |
| --- | --- |
| DuckDB | file, as for SQLite |
| libSQL | URL (`libsql://…` or `https://…`), auth token |
| ClickHouse | host, port (8123, or 8443 with TLS on), username, password, database, TLS |

SSH tunnels work for ClickHouse as they do for MySQL. A libSQL URL is reached
directly.

`tql open` and `:open` take:

- a DuckDB file, known by its header, or by the `.duckdb` or `.ddb` extension
  for a file that does not exist yet
- `libsql://db-org.turso.io?authToken=…`, and `https://` with the same
  `authToken`
- `clickhouse://user:pass@host:8123/db`, and `clickhouses://` for TLS on 8443

TablePlus import brings over ClickHouse and DuckDB connections, which
TablePlus also has. libSQL has no TablePlus driver.

Each engine has an icon setting under `[icons]` (`duckdb`, `libsql`,
`clickhouse`) and a label and default port in the driver list.

### Browsing

- **DuckDB:** tables listed under their schemas, as the first part describes,
  with `main` as the default schema. Every DuckDB type shows readably: `LIST`,
  `STRUCT` and `MAP` values as JSON (with the JSON viewer), `HUGEINT` and
  `DECIMAL` exactly, dates, times, timestamps with and without a time zone, and
  intervals.
- **libSQL:** exactly as SQLite, including the `b`-less single database.
- **ClickHouse:** databases through `b`, as on MySQL. Tables, views and
  materialized views are listed. A table with no sorting key is shown in no
  particular order, and the grid says so. `Array`, `Tuple`, `Map` and `JSON`
  show as JSON; `Nullable` and `LowCardinality` unwrap; `Decimal`, `Int128`,
  `Int256` and `UInt64` show exactly.
- Structure (`t`) shows each engine's columns, types, defaults and keys.
  ClickHouse shows the engine, sorting key, partition key and primary key in
  place of indexes.

### Editing

- DuckDB and libSQL edit, add and delete rows by primary key, written on `:w`,
  as SQLite does. A table without a primary key is read-only, as today.
- ClickHouse refuses edits, adds and deletes with "ClickHouse tables are
  read-only in tql: its updates are background mutations". The SQL editor still
  runs any statement the connection allows, unless its tag makes it read-only.

### Read-only

The rules stay the same as today: a read-only connection's SQL editor, agent
commands and MCP queries run reading statements only, and the engine itself
refuses a write hidden in a statement that looks like a read.

- **DuckDB:** the statement runs in a transaction that tql always rolls back,
  after the same check SQL Server uses for writing words.
- **libSQL:** `PRAGMA query_only` is set on the stream before the statement
  and the stream is closed after it. If the server refuses the pragma, tql
  falls back to a rolled-back transaction.
- **ClickHouse:** the request carries `readonly=1`, which the server enforces.

### Completion

Completion learns each engine's keywords and functions, and lists tables and
columns as it does for the engines it has.

## Design

- `Driver` gains `Duckdb`, `Libsql` and `Clickhouse`, with their names
  (`duckdb`, `libsql`, `clickhouse`), labels and default ports (none, none,
  8123). The store keeps reading old rows unchanged.
- Each engine implements `Database` in its own module beside the others:
  `database/duckdb.rs`, `database/libsql.rs`, `database/clickhouse/`.
- DuckDB uses the `duckdb` crate with its bundled library. libSQL and
  ClickHouse speak HTTP through `ureq`, which tql already uses for updates and
  the AI, so they bring no async runtime of their own. ClickHouse reads results
  as `JSONCompactEachRowWithNamesAndTypes`, so types arrive with the rows.
- `dsn.rs` learns the new schemes and `opener` learns the DuckDB header. The
  connection form's fields come from the driver, as they do now.
- Quoting: DuckDB and libSQL use double quotes, ClickHouse backticks.
- Paging: `LIMIT … OFFSET` on all three.

### The DuckDB build

DuckDB is C++, and the Linux releases are static musl builds made with
`musl-tools`, which has no C++ compiler. The first task proves a static
`x86_64` and `aarch64` musl build of tql with the bundled DuckDB, and measures
how much it adds to the binary. If `musl-tools` cannot do it, the Linux build
moves into an Alpine container, which has a musl C++ compiler. If neither can
produce a static binary, DuckDB ships on macOS only for 0.14.0 and the Linux
build says DuckDB is not in this build. Decide after the experiment, not
before.

## Testing

- DuckDB runs in the main suite: a file is made in the test, so no server is
  needed. Every type round-trips through the grid, the inspector, export and
  editing; the read-only refusal holds for a write inside a `WITH`.
- libSQL and ClickHouse join the live kinds suite, each skipped when its
  server is not reachable, as SQL Server is now. CI runs `clickhouse/clickhouse-server`
  and `ghcr.io/tursodatabase/libsql-server` as services.
- `dsn` and `opener` tests for every new scheme and the DuckDB header.
- The connection form test for each driver's fields.
- TablePlus import maps ClickHouse and DuckDB.
- `tql dump`, `load` and `sync` refuse the new engines with a clear message.
