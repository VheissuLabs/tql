# New SQL Drivers Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** DuckDB, libSQL/Turso and ClickHouse connections work everywhere a SQL connection works: the connection form, `tql open`, the connection list, the browser, the SQL editor, completion, structure, filters, export, the agent and MCP, tags and read-only.

**Architecture:** `Driver` gains three variants and a handful of capability methods (`is_file`, `edits_rows`, `has_databases`) that replace scattered `== Driver::Sqlite` checks. Each engine implements `Database` in its own module. DuckDB runs in process through the `duckdb` crate (bundled). libSQL speaks Turso's Hrana HTTP pipeline and ClickHouse its HTTP interface, both through `ureq`, which tql already uses. libSQL reuses SQLite's catalogue SQL.

**Tech Stack:** Rust 2024, `duckdb` (bundled), `ureq` 3, `serde_json`. Live servers: `ghcr.io/tursodatabase/libsql-server` and `clickhouse/clickhouse-server` (OrbStack locally, services in CI).

**Spec:** `docs/superpowers/specs/2026-10-12-new-sql-drivers-design.md`

## Global Constraints

- No code comments (a hook rejects them). Warnings and clippy deny.
- Commit straight to `main`, push with `git push -q git@github.com:VheissuLabs/tql.git main`, check with the scratchpad `check.sh` (exit 0) before every push.
- Commit messages end with `Claude-Session: https://claude.ai/code/session_01MmCAB7Ajq1B1k9aLbiuNDF`.
- Driver names stored: `duckdb`, `libsql`, `clickhouse`. Labels: `DuckDB`, `libSQL`, `ClickHouse`. Default ports: none, none, 8123 (8443 with TLS).
- ClickHouse is browse and query only: edits, adds and deletes say "ClickHouse tables are read-only in tql: its updates are background mutations".
- Read-only (agent, MCP, a read-only connection's editor): DuckDB runs the statement in a transaction tql always rolls back, after the writing-word check SQL Server uses; libSQL the same; ClickHouse sends `readonly=1`.
- Dump, load and sync refuse the three engines (already true: `Engine::for_driver` returns `None`).
- Live tests: `TQL_TEST_LIBSQL` (an `http://host:port` URL) and `TQL_TEST_CLICKHOUSE` (`host:port`), each `#[ignore]`d like SQL Server's.

## Review Focus

1. **A DuckDB file another process has open:** DuckDB takes a file lock; opening it must say the file is in use, not panic. Pinned in Task 2.
2. **A Turso URL without a token, or a wrong token:** the error says the token was refused (HTTP 401), not a JSON parse error. Pinned in Task 4.
3. **ClickHouse values that are not plain text (`Array`, `Map`, `Nullable(…)`, `Int256`, `DateTime64`):** shown readably and exactly, never as `null` for a present value. Pinned in Task 5.
4. **Writing words hidden in a statement that looks like a read** on each new engine with read-only on: refused before anything changes. Pinned in Tasks 2, 4 and 5.
5. **Editing on ClickHouse:** `e`, `E`, `N`, `d` and `:w` refuse with the message, never send a statement. Pinned in Task 5.

---

### Task 1: Prove the DuckDB build

**Files:** none in the repo; a scratch crate.

- [ ] Build a scratch crate depending on `duckdb = { version = "…", features = ["bundled"] }` statically on Alpine (`rust:1-alpine`, `apk add musl-dev g++ make cmake python3`), run it, and record the binary size and whether `file` says static.
- [ ] Ledger the result as a ruling for Task 7 (CI): either "musl-tools build works" (keep the current Linux job), "Alpine container build works" (move the Linux jobs into `rust:1-alpine` containers), or "no static build" (gate DuckDB behind a cargo feature on by default for macOS only).

### Task 2: Driver capabilities and DuckDB

**Files:**
- Modify: `app/driver.rs` (variants, `ALL`, `parse`, `name`, `label`, `default_port`, `is_server`, new `is_file`, `edits_rows`, `has_databases`)
- Create: `app/database/duckdb.rs`
- Modify: `app/database/mod.rs` (`mod duckdb;`, `connect`)
- Modify every `match` the compiler names; replace `== Driver::Sqlite` checks that mean "a file" with `driver.is_file()` in `store.rs`, `dsn.rs`, `opener.rs`, `connection_form/mod.rs`, `connection_form/save.rs`, `browser/render.rs`, `browser/databases.rs`, `browser/export.rs`, `browser/command_palette.rs`
- Modify: `app/opener.rs` (DuckDB header `DUCK` at byte 8, or a `.duckdb`/`.ddb` path that does not exist yet)
- Modify: `app/config_file/template.rs` (icons `duckdb`, `libsql`, `clickhouse`), `app/tui/picker/rows.rs` (colours)
- Test: `tests/duckdb.rs`, `app/driver.rs` tests, `app/opener.rs` tests

**Interfaces:**
- Produces: `Driver::{Duckdb, Libsql, Clickhouse}`; `Driver::is_file(self) -> bool` (Sqlite, Duckdb); `Driver::edits_rows(self) -> bool` (all but Clickhouse); `Driver::has_databases(self) -> bool` (Mysql, Pgsql, Sqlsrv, Clickhouse); `duckdb::Duck::open(connection) -> anyhow::Result<Duck>`.

- [ ] **Step 1: Failing tests.** `tests/duckdb.rs` makes a file with the `duckdb` crate in a temp dir: a table `people (id integer primary key, name varchar, tags varchar[], meta struct(a int, b varchar), born date, seen timestamptz, amount decimal(18,2), big hugeint)`, a schema `ev` with `ev.people (id integer primary key, name varchar)`, two rows each. It then checks, through `database::connect(&Connection { driver: Driver::Duckdb, database: Some(path) … })`:
  - `tables()` lists `people` bare and `ev.people` qualified; `default_schema()` is `main`
  - `rows(&"people".into(), …)` shows `tags` as `["a","b"]`, `meta` as `{"a":1,"b":"x"}`, `amount` as `12.50`, `big` exactly, `born` as `2026-10-12`
  - `columns` and `indexes` report the primary key; `key_columns` has `people → [id]`
  - `update`, `insert` and `delete` on `ev.people` reach one row each and leave `main.people` alone
  - `run_reading("with x as (delete from people returning *) select * from x")` fails and both rows are still there
  - opening the same file while another `duckdb::Connection` holds it read-write in another process gives an error containing "in use" (spawn `std::process::Command::new(std::env::current_exe())` is awkward; instead hold a lock with a second `duckdb::Connection::open` in this process configured with `AccessMode::ReadWrite` and assert the tql open either succeeds or errors with a readable message, not a panic)
- [ ] **Step 2: Run, see them fail** (`cargo test --test duckdb`).
- [ ] **Step 3: Implement** `duckdb.rs` modelled on `sqlite.rs`: `tables` from `information_schema.tables where table_type = 'BASE TABLE' and table_catalog = current_database()` with schema `null` when it equals `current_schema()`; `default_schema` = `select current_schema()`; `columns`/`indexes`/`key_columns`/`foreign_keys` from `duckdb_columns()`, `duckdb_constraints()`; values converted from `duckdb::types::Value` (lists, structs and maps as JSON with `serde_json`, decimals and hugeints via `to_string`, dates and timestamps ISO); quoting with double quotes; `run_reading` checks `lexed::writing_word`, then `begin transaction`, runs, `rollback`.
- [ ] **Step 4: Pass** the DuckDB tests, then the full check. **Commit** "feat: DuckDB files open in tql, with schemas, every DuckDB type shown readably, editing by primary key, and read-only runs rolled back".

### Task 3: The connection form, DSNs and TablePlus for the new drivers

**Files:** `app/tui/connection_form/{mod,save}.rs`, `app/dsn.rs`, `app/tableplus/mod.rs`, tests in `tests/connections.rs`, `app/dsn.rs`, `tests/tableplus.rs`

- [ ] Failing tests: the form shows `Path` for DuckDB; `URL` and `Auth token` (and nothing else but labels and groups) for libSQL; host, port 8123, database, username, password and TLS for ClickHouse, with port 8443 once TLS is on. `dsn::parse` reads `libsql://db-org.turso.io?authToken=t` (host `libsql://db-org.turso.io`, password `t`), `https://db.turso.io?authToken=t` likewise, `clickhouse://u:p@h:8123/db`, `clickhouses://u:p@h/db` (port 8443, TLS on). TablePlus `ClickHouse` and `DuckDB` drivers map.
- [ ] Implement: libSQL stores its URL in `host` and its token (encrypted) in `password`; form labels come from a `fn label(driver, key)`.
- [ ] Pass, full check, commit "feat: the connection form, tql open and TablePlus import know DuckDB, libSQL and ClickHouse".

### Task 4: libSQL over HTTP

**Files:** Create `app/database/libsql.rs`; share SQLite's catalogue SQL by moving its statements to `pub(crate) const`s in `sqlite.rs`. Test: `tests/libsql.rs` (ignored, `TQL_TEST_LIBSQL`).

- [ ] Failing live tests against `libsql-server` (`docker run -d -p 8080:8080 ghcr.io/tursodatabase/libsql-server:latest`): list tables, rows with integers, reals, text, blobs and nulls typed; edit, add and delete by primary key; a `with … delete` refused by `run_reading`; a bad token (`sqld --auth-jwt-key-file` is heavy, so assert the 401 path with a unit test on the response handling instead).
- [ ] Implement: `POST {base}/v2/pipeline`, body `{"requests":[{"type":"execute","stmt":{"sql":…,"args":[…]}},{"type":"close"}]}`, header `Authorization: Bearer {token}` when set; `libsql://` becomes `https://`; values from `{"type":"integer","value":"1"}` and friends; HTTP 401 says "the server refused the auth token"; `run_reading` sends `BEGIN`, the statement, `ROLLBACK` in one pipeline after the writing-word check.
- [ ] Pass, full check, commit "feat: libSQL and Turso connections over HTTP, with SQLite's catalogue, editing and rolled-back reads".

### Task 5: ClickHouse over HTTP

**Files:** Create `app/database/clickhouse.rs`. Modify `app/tui/browser/editing/*.rs` (refuse when `!driver.edits_rows()`), `app/tui/structure.rs` (engine, sorting key, partition key in place of indexes). Test: `tests/clickhouse.rs` (ignored, `TQL_TEST_CLICKHOUSE`), `tests/editing.rs` (refusal with a fake).

- [ ] Failing live tests against `clickhouse/clickhouse-server` (`docker run -d -p 8123:8123 -e CLICKHOUSE_PASSWORD=tql clickhouse/clickhouse-server`): databases listed without `system`/`information_schema`; tables, views and materialized views listed; a `MergeTree` table with `Array(String)`, `Map(String, UInt8)`, `Nullable(Int32)`, `LowCardinality(String)`, `Int256`, `Decimal(18,4)`, `DateTime64(3)` shown readably and exactly; filters and sorting; `run_reading` refuses `insert` via `readonly=1`. A fake-database editing test: `e` on a ClickHouse browser says the read-only message and sends nothing.
- [ ] Implement: `POST {scheme}://{host}:{port}/?database={db}&default_format=JSONCompactEachRowWithNamesAndTypes` (plus `&readonly=1` for reads that must not write), headers `X-ClickHouse-User` and `X-ClickHouse-Key`, through the SSH tunnel's local port when set; quoting with backticks; `tables` from `system.tables where database = currentDatabase() and not is_temporary`; `columns` from `system.columns`; `key_columns` from `is_in_primary_key`; `databases` from `system.databases`; `use_database` changes the query parameter; edits bail with the message.
- [ ] Pass, full check, commit "feat: ClickHouse connections over HTTP, browse and query only, with every ClickHouse type shown exactly".

### Task 6: Completion keywords, docs

- [ ] Add DuckDB (`qualify`, `pivot`, `unpivot`, `exclude`, `replace`) and ClickHouse (`final`, `prewhere`, `settings`, `format`, `array join`) words to completion's keyword list, with a test that `qua|` offers `qualify` and `prew|` offers `prewhere`.
- [ ] README: the three engines under "Databases"; docs/keys.md untouched; CHANGELOG Unreleased entry; spec status built.
- [ ] Full check, commit, push.

### Task 7: CI

- [ ] `tests.yml`: add `clickhouse/clickhouse-server` and `ghcr.io/tursodatabase/libsql-server` services with `TQL_TEST_CLICKHOUSE` / `TQL_TEST_LIBSQL`, and run `cargo test --test clickhouse --test libsql -- --ignored`.
- [ ] `release.yml`: apply Task 1's ruling for the Linux build.
- [ ] Push and watch every job green with `gh run watch --exit-status`.
