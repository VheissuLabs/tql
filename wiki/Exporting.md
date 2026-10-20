`tql export` writes rows as re-importable SQL — `insert` statements you can read.
For a whole database, schema and all, and fast enough for tables in the tens of
gigabytes, use [Dump, load and sync](Dump-Load-and-Sync) instead.

## From the command line

This is the scriptable way:

```bash
tql export                           # asks which connection, then which table
tql export prod orders --limit=1000 --sql=./orders.sql
tql export prod --sql=./prod.sql     # every table, one file
tql export prod --list               # what tables are there
tql export prod --database=shop      # pick the database on a server connection
```

A connection that names a server rather than a database asks which one first, or
takes `--database=`; without either it stops and says so rather than exporting
every schema on the box.

Name a connection and it goes straight through; name nothing and it asks three
questions: which connection (most recently used first), which table (the whole
database is the first answer), and where to save it, offering the auto-named file
in the export directory. Answer that last one with a folder and it names the file
for you; `--sql` skips the question. Piped or with `--no-interaction` it never
asks. It says what it needed and stops with a non-zero exit.

`--sql` takes a file or a directory; omit it and the file is named automatically
in the export directory, after the database, the table and the time
(`karlm_v2-users-20260922-231302.sql`, or `karlm_v2-20260922-231302.sql` for the
whole thing), since the connection's own name says nothing about what is in the
file. `--limit` caps rows per table, which is how you pull a slice of production
rather than all of it.

Save one somewhere else, `~/Downloads` say, and that becomes the folder the next
export is offered in. `ui.export_path` in the [config](Configuration#ui)
overrides it, and if the remembered folder is gone tql falls back to its own.

## From inside the interface

`:export` writes what you are looking at to a `.sql` file of `insert` statements.
On a table that is every row, read in chunks so a large table does not go through
memory at once; after a query it is the rows you have loaded.

`:export database` writes every table in the current database to one `.sql` file.

Files land in `~/.config/tql/exports` (override with `ui.export_path`), named
`connection-table-YYYYMMDD-HHMMSS.sql`. The status line reports the row count,
file size and path.

## What it is not

Data only, no schema. Your migrations own the schema; this is for pulling rows
from one database into another. A `.sql` file from `tql export` can be replayed
with [`tql load`](Dump-Load-and-Sync#tql-load), all or nothing, into the same
kind of database it came from.
