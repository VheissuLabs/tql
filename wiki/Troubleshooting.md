A few things that go wrong, and where the answer is.

## My clicks land a row out

Inside a multiplexer whose tab bar or status sits above the pane, tql's row zero
is not the terminal's row zero. Set an offset in [`[ui]`](Configuration#ui):

```toml
[ui]
mouse_row_offset = 1
```

The same for `mouse_column_offset` and a sidebar to the left of the pane. A
terminal that pads one side more than the other is evened out with `left_margin`
or `right_margin`.

## Shift+enter does not make a new line

Terminals send the same byte for enter and shift+enter, so shift+enter only
arrives as its own key when the terminal is told to send one. In Ghostty:

```
keybind = shift+enter=csi:13;2u
```

Alt+enter works without any configuration, if you would rather not set that.

## `a` says there is no model to ask

tql could not find a url or a key it can use, and it says what to set. Set a
provider key such as `ANTHROPIC_API_KEY` or `OPENAI_API_KEY`, or point `[ai]` at a
local model. If it fails at the network, the server is not running or the port is
wrong, and a `curl` against the endpoint is the quickest way to tell which. See
[Asking for SQL](Asking-for-SQL).

## I cannot edit a table

Editing requires a single-column primary key, which tql uses to target the row.
Tables without one are read-only, and so are connections flagged **read only**. A
connection tagged `production` is not read-only by itself, but writing needs the
usual `:w` after review, and some commands ask first. See
[Tags and read only](Connecting#tags-and-read-only) and
[Editing data](Editing-Data).

## My filter or my pending changes disappeared

Both are tied to the table. Change table and the filter is dropped, because a
column that exists in one table usually does not in another, and pending edits
and marks are dropped, because a mark means nothing in a table where that id is a
different row. Switching database with `b` drops them too. See
[Filtering rows](Browsing-Data#filtering-rows) and
[Pending changes](Editing-Data#pending-changes).

## Relation rows are missing from the inspector

Relations are found by following foreign keys, both ways. `ui.inspect_related`
caps how many rows are loaded (10 by default, 0 turns it off); when there are
more, the heading says `(10 of 47)`. A **join table is read through**, so a table
that only holds two foreign keys shows as `has many through`, not as a relation of
its own. See [The row inspector](Row-Inspector).

## My icons are boxes or question marks

The defaults are Nerd Font devicons. Install a Nerd Font in your terminal, or set
plain characters in [`[icons]`](Configuration#icons): `mysql = "M"`, or `""` for
nothing at all.

## SQL Server will not connect

tql's SQL Server driver needs TLS 1.2 or newer, which is every SQL Server from
2016 on and the older ones that have had their TLS 1.2 update. It is encrypted by
default without checking the server's certificate, which is what a local or
self-signed server needs; `ssl_mode` `verify-ca` or `verify-full` checks it, and
`disable` turns encryption off. See [TLS](Connecting#tls).

## A dump or load failed

- **The drive looks too small.** tql checks free space before it dumps; `--force`
  overrides it. Postgres dumps are compressed, MySQL dumps are not, so allow about
  the size of the data for MySQL.
- **The load stops at a table that already exists.** That is deliberate; add
  `--drop` to replace the tables.
- **A MySQL user without the snapshot privileges.** Add `--no-lock`; tables are
  then read at slightly different moments.
- **A write into production was refused.** `tql load` and `tql sync` ask before
  writing to a connection tagged production; in a script, pass `--force`.
- **It moved nothing between engines.** None of them convert between database
  engines, and SQL Server is not supported yet.

See [Dump, load and sync](Dump-Load-and-Sync).

## `tql database.sqlite` ran a command called `database.sqlite`

It did not. A first argument that exists on disk, contains a `/`, ends in
`.sqlite`, `.sqlite3` or `.db`, or looks like a connection string is treated as a
path rather than a command name. A file that exists is never a command name,
which makes the rewrite unambiguous.

## My connection string is in my shell history

It is: a connection string on the command line is passed to the shell like any
other argument. Paste it once, then use the connection list, where the password is
encrypted at rest. Prefixing the command with a space keeps it out of history in
zsh if `HIST_IGNORE_SPACE` is set.

## A key above the first `[section]` does nothing

Every key belongs to a section. A key written above the first `[section]` belongs
to no section, is never read, and quietly does nothing. tql notices and tells you
which ones. Run `tql config` to see whether the file is in order, and
`tql config --tidy` to put it back.

## tql started on the defaults and said so

A config file that cannot be parsed does not stop tql: it starts on the defaults
and says so in the status line. Run `tql config` to find the file, fix or delete
the offending line, or `tql config --tidy` to reorder it. A `.bak` is written
first.

## `tql -v` printed the version instead of being verbose

`tql -v` on its own prints the version, as `-V` and `--version` do. After a
command, `-v` still means verbose.

## Where is everything?

| | |
| --- | --- |
| `~/.config/tql/config.toml` | your settings |
| `~/.config/tql/tql.sqlite` | saved connections and query history |
| `~/.config/tql/key` | the encryption key for stored passwords, `0600` |
| `~/.config/tql/exports/` | where exports go unless you say otherwise |

`XDG_CONFIG_HOME` moves all of it.
