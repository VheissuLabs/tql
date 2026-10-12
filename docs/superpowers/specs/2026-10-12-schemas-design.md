# Schemas: every schema's tables, and completion that knows them

Date: 2026-10-12
Status: approved, not built
Issue: VheissuLabs/tql#5
Release: 0.14.0, first of three parts (schemas, then DuckDB, libSQL/Turso and
ClickHouse, then Redis/Valkey, each with its own spec)

## Goal

On Postgres and SQL Server, tql lists only the tables in the current schema
(`current_schema()`, `schema_name()`). A table in any other schema can be
reached only by typing its SQL, and completion does not know it exists. The
issue asks for `ev.` to complete the tables in schema `ev`.

Make every user schema's tables first-class: listed, browsed, filtered,
edited, inspected, exported and completed, under their schema.

## Decisions already made

- The table list groups tables under a folding header per schema.
- Completion offers everything, qualified: schema names, the default schema's
  tables plain, and other schemas' tables as `schema.table`.
- A table's name carries its schema in a type, not in a dotted string.

## Out of scope

- MySQL and ClickHouse: a schema there is a database, which the `b` switcher
  already covers.
- SQLite attached databases.
- Creating, renaming or dropping schemas.
- Views. Schemas list the same kinds of relation the current schema lists
  today.

## What the user sees

### The table list

On Postgres and SQL Server, each user schema with at least one table is a
header with its table count:

```
▾ public
    customers
    orders
▾ ev
    category
    evaluation
    evaluation_type
▸ cp            (12)
▸ audit         (4)
```

- The default schema comes first: `current_schema()` on Postgres, the user's
  default schema on SQL Server (usually `dbo`). The others follow in
  alphabetical order.
- The default schema starts unfolded and the others start folded. Folding is
  remembered per connection and database, the way connection groups are
  remembered, so a schema stays open or shut across runs.
- When only one schema has tables, there are no headers. The list looks as it
  does today.
- System schemas are never listed: on Postgres `pg_catalog`,
  `information_schema`, `pg_toast` and every `pg_temp_*` and `pg_toast_temp_*`;
  on SQL Server `sys`, `INFORMATION_SCHEMA`, `guest` and the `db_*` role
  schemas.
- The header keys work as they do on connection group headers: `↵`, `h` and
  `l` fold and unfold, `j` and `k` move over headers as rows, and a click on a
  header folds it.
- `/` filters across every schema. A schema with a match unfolds while the
  filter is on, and a schema with no match is hidden.
- The table count on a folded header is the number of tables in that schema.

### Names

A table in the default schema keeps its bare name everywhere. A table in any
other schema is written `schema.table` wherever tql names it:

- the grid's title and the status line
- the ctrl+k palette (tables are listed qualified and found by either part)
- the record inspector's related tables
- export file names (`shop-ev.evaluation-20261012-120000.sql`) and the
  `-- ev.evaluation` section comments in a whole-database export
- the agent and MCP tools

The shell commands and MCP tools take the same names: `tql describe shop
ev.evaluation`, `tql export shop ev.evaluation`, and the `table` argument of
the MCP describe tool. `tql tables` and the MCP table list return qualified
names. An unqualified name means the default schema, as today. When the
default schema has no such table, but exactly one other schema does, the name
means that table.

### Completion

```
select * from ev
  ev                 schema
  evaluation         ev
  evaluation_type    ev
  events             table
```

- Schema names are offered with the kind `schema`.
- The default schema's tables are offered plain, with the kind `table`.
- Other schemas' tables are offered by their own name, with their schema as the
  hint, and are inserted qualified: choosing `evaluation` above inserts
  `ev.evaluation`.
- After `ev.` only the tables in `ev` are offered, inserted bare after the dot.
- After `ev.evaluation.` the columns of `ev.evaluation` are offered.
- An alias on a qualified table completes: after `from ev.evaluation e`, `e.`
  offers its columns.
- A table qualified with the default schema (`public.orders`) completes like the
  bare name.
- Quoted parts are understood: `"Ev"."Evaluation".` and
  `[ev].[evaluation].` complete columns too.

## Design

### The table name type

A `Table` replaces the bare `String` wherever a table is named in the database
layer and the browser:

```rust
pub struct Table {
    pub schema: Option<String>,
    pub name: String,
}
```

- `schema` is `None` for the default schema, and on every driver without
  schemas. A table in the default schema is therefore named exactly as tql
  names it today, so anything keyed by a table name before keeps working.
- `Display` writes `name` or `schema.name`.
- `quoted(&self, quote: impl Fn(&str) -> String)` writes `quote(name)` or
  `quote(schema).quote(name)`, so each driver keeps its own quoting.
- `parse(text)` reads a typed name, `schema.table` with each part optionally
  quoted, for the shell commands, MCP and completion.

### The database layer

- `Database::tables` returns `Vec<Table>`. Every method that takes a table name
  takes `&Table`: rows, filtered rows, columns, indexes, column names, highest,
  insert, update and delete. Primary keys, key columns and foreign keys are
  keyed by `Table`, and a foreign key names the schema of the table it points
  to, so a key from `ev.evaluation` to `public.users` is followed.
- Postgres lists tables from `pg_class` joined to `pg_namespace`, over every
  schema except the system ones, and marks those in `current_schema()` as the
  default. Keys and foreign keys are read for every listed schema in one query
  each, as today.
- SQL Server does the same from `sys.tables` and `schema_name(schema_id)`
  against `schema_name()`.
- MySQL and SQLite return every table with `schema: None`, and their SQL does
  not change.

### The browser

- The sidebar's rows become headers and tables, built the way the connection
  list builds group headers and connections. A sidebar with one schema has
  table rows only.
- The fold state is remembered in the store under a key per connection id and
  database, written when a header is folded or unfolded.
- The browser keeps `Vec<Table>` for its tables, its open table and its
  history of tables visited (`ctrl+o`).

### Completion

- The completion's schema gains the list of schemas and the default schema, and
  its tables become `Table`s.
- The word before the cursor is split on unquoted dots into up to three parts.
  One part completes schemas and tables. Two parts complete the tables in a
  schema, or, when the first part is a table or alias, its columns. Three parts
  complete the columns of `schema.table`.
- `referenced`, which finds the tables and aliases in the statement, reads
  qualified names in `from` and `join`.

## Testing

- Unit: `Table` display, quoting for each driver, parsing (bare, qualified,
  quoted with a dot inside the quotes).
- Completion: schema names; a non-default table inserted qualified; `ev.`
  narrowing; `ev.evaluation.` columns; an alias on a qualified table; the
  default schema written out; quoted parts.
- Live Postgres and SQL Server (the existing live kinds suite): a fixture with
  `public` and `ev`, a foreign key from `ev.evaluation` to `public.users`, and
  a table of the same name in both. Listing, browsing, filtering, editing,
  deleting, inserting, inspecting the cross-schema relation, and exporting
  `ev.evaluation`.
- Browser: headers appear with two schemas and not with one; folding is
  remembered; `/` unfolds a matching schema; the palette finds `ev.evaluation`.
- Shell: `tql tables` lists qualified names; `tql describe` and `tql export`
  take `ev.evaluation`; an unqualified name found only in one other schema is
  that table.
- MySQL and SQLite suites pass unchanged.
