# Dump, load and sync

`tql export` writes rows as SQL you can read. For a whole database, schema and
all, and fast enough for tables in the tens of gigabytes, there are three
commands: `tql dump` writes a database to a folder, `tql load` reads one back, and
`tql sync` does both in one step. They hand the work to the tools built for it,
and give them your saved connection, password, TLS and SSH tunnel included:

| Database | Dumps with | Loads with | Install |
| --- | --- | --- | --- |
| MySQL | `mydumper` | `myloader` | `brew install mydumper` |
| Postgres | `pg_dump` | `pg_restore` | `brew install libpq` |
| SQLite | tql itself | tql itself | nothing |

All three work in parallel, from a consistent snapshot, and split big tables into
pieces. While they run you see a progress bar counting tables, with the one being
worked on under it; `-v` shows everything the tools say instead. `--dry-run`
shows the exact command without running it, and the password is never on it: tql
hands it to the tool in a private file that is gone when the command ends.

## `tql dump`

```bash
tql dump notarydash-prod                         # every table, into a new folder
tql dump notarydash-prod orders users --to=./nd  # two tables, into a folder you name
tql dump mysql-dev --database=shop               # a server connection that names no database
```

The dump goes in a new folder beside your [exports](Exporting) unless `--to` says
where, and tql checks the drive has room first. Postgres dumps are compressed;
MySQL dumps are not, because myloader 1.0.5 hangs loading compressed files on
macOS, so allow about the size of the data. A `tql.json` in the folder records
where the dump came from, which `tql load` reads.

| Argument or flag | What it does |
| --- | --- |
| `<connection>` | the saved connection to dump |
| `[tables…]` | only these tables; every table when none are named |
| `--to=<folder>` | where to write the dump; it must be new or empty |
| `--database=<name>` | which database on the server, for a connection that names none |
| `--threads=<n>` | how many tables, or pieces of a table, to dump at once; 4 unless you say |
| `--data-only` | rows only, for loading into tables that already exist |
| `--no-lock` | skip the consistent snapshot, for a MySQL user without the privileges it needs; tables are then read at slightly different moments |
| `--force` | dump even when the drive looks too small |
| `--dry-run` | show the command tql would run, and stop |
| `-v` | show everything the tool says instead of the progress bar |

## `tql load`

`tql import` is the same command.

```bash
tql load ./nd notarydash-local                   # a dump into tables that do not exist yet
tql load ./nd notarydash-local --drop            # replacing tables it already has
tql load ./nd mysql-dev --database=nd_copy       # into a database it creates
tql load ./orders.sql notarydash-local           # replay a tql export, all or nothing
```

A dump folder is loaded with myloader or pg_restore, and a load stops at a table
that already exists unless `--drop` says to replace it. A `.sql` file from
`tql export` is replayed in one transaction: if a statement fails, nothing is
loaded. A dump only loads into the same kind of database it came from.

| Argument or flag | What it does |
| --- | --- |
| `<source>` | a folder from `tql dump` (or straight from mydumper or pg_dump), or a `.sql` file from `tql export` |
| `<connection>` | the saved connection to load it into |
| `--database=<name>` | which database on the server, for a connection that names none; MySQL creates it if it is not there |
| `--drop` | drop and recreate tables that already exist; for a SQLite connection, replace the file |
| `--threads=<n>` | how many tables, or pieces of a table, to load at once; 4 unless you say |
| `--force` | load into a connection tagged production without being asked |
| `--dry-run` | show the command tql would run, and stop |
| `-v` | show everything the tool says instead of the progress bar |

## `tql sync`

```bash
tql sync notarydash-prod local --database=notarydash                  # same name on both sides
tql sync notarydash-prod local --database=notarydash --into=nd_copy   # another name locally
tql sync notarydash-prod local --database=notarydash users orders     # just these tables
tql sync notarydash-prod staging --database=notarydash                # remote to remote
```

A sync dumps from the first connection and loads into the second, replacing the
tables it brings over and leaving the rest alone. The first connection is only
ever read. Either side can be local or remote; between two servers, the data
passes through your machine. The dump goes through a folder that is deleted
afterwards, unless `--keep` keeps it as a backup.

The database on each side: the source uses `--database`, or the one its
connection names. The target uses `--into`, or the one its connection names, or
else the same name as the source.

| Argument or flag | What it does |
| --- | --- |
| `<from>` | the saved connection to copy from |
| `<to>` | the saved connection to copy into |
| `[tables…]` | only these tables; every table when none are named |
| `--database=<name>` | the database to copy, for a connection that names none |
| `--into=<name>` | what the database is called on the other side, when it is not the same |
| `--threads=<n>` | how many tables, or pieces of a table, to copy at once; 4 unless you say |
| `--no-lock` | skip the consistent snapshot, as for `tql dump` |
| `--via=<folder>` | the folder the copy passes through; beside your exports unless you say |
| `--keep` | keep that folder afterwards, as a backup |
| `--force` | copy into a connection tagged production, or past the free space check |
| `--dry-run` | show both commands without running them |
| `-v` | show everything the tools say instead of the progress bars |

## What they will not do

`tql load` and `tql sync` refuse a read-only connection, and ask before writing
to one tagged production: in a script, where there is no one to ask, they need
`--force`. A sync will not copy a database onto itself. None of them move data
between engines, and SQL Server is not supported yet.
