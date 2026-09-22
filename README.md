# dotsql

A database client for the terminal, built with Laravel Zero, Laravel Prompts and Laravel MCP.

The same engine drives two faces: a full-screen terminal interface for you, and an
MCP server for an AI agent. Both call the same `QueryRunner`, and every statement
either of them runs is recorded in one shared history.

## Running it

```bash
php dotsql
```

`browse` is the default command. On first run dotsql creates `~/.config/dotsql/`
containing `dotsql.sqlite` (connections and query history) and `key` (the
encryption key), both `0600`.

## Keys

| Key | Action |
| --- | --- |
| `tab` | switch between the table list and the grid |
| `↑ ↓` / `j k` | move the cursor |
| `← →` / `h l` | move between columns |
| `↵` | open a table, or edit the selected cell |
| `i` | view the selected value full screen, read-only |
| `e` | edit the selected value, `ctrl+s` saves |
| `< >` | narrow or widen the selected column |
| `=` | reset the column width |
| `n` / `p` | next or previous page (100 rows) |
| `r` | reload the current table |
| `o` | sort by the column the cursor is on |
| `s` | open the SQL editor |
| `:` | command line — `:q`, `:tables`, `:rows`, `:reload`, `:sql` |
| `q` / `esc` | quit |

Mouse works too: click a table or a row, scroll with the wheel, and **drag a
column border in the header row to resize it**, as you would in a spreadsheet.

Column widths you set are remembered per column name, so several columns keep
their sizes at once and survive paging and switching tables.

## Sorting

Click a column header, or press `o` on a column, to sort by it: first click
ascending, second descending, third clears it. The header shows `▲` or `▼`, and
the `order by` appears in the SQL pane — so the sort teaches the clause that
produced it.

Sorting applies to a table, not to query results; those are ordered by whatever
your query says.

## Seeing the query behind the view

With `ui.sql_always` on, the SQL pane shows the statement that produced what you
are looking at, and updates as you change table or page:

```
┌─ SQL ────────────────────────────────────────────┐
│ select * from "tracks" limit 100 offset 100      │
└──────────────────────────────────────────────────┘
```

Press `s` and that statement is handed to you to edit — change the `limit`, add
a `where`, press `ctrl+r`, and the grid shows your version. Clear the pane and
the running query comes back.

## Running SQL

The SQL pane is syntax highlighted — keywords, quoted identifiers, strings,
numbers and comments each coloured, the same tokeniser approach as the JSON
viewer and equally careful never to drop a character while you type.



Press `s` (or `:sql`) for the SQL editor, which opens above the results.
`ctrl+r` runs what you have typed, `esc` returns to browsing. Results replace
the grid and are read-only, since they have no primary key to write back
through — open a table to edit.

## JSON columns

A cell holding JSON opens in a full-width modal when you press `i`: pretty
printed, with line numbers and syntax highlighting — keys, strings, numbers and
literals each coloured. `↑↓` scrolls a line at a time, `n`/`p` a page, `esc`
closes.

Detection is by parsing, not by column type, so JSON stored in a `text` column
is recognised too.

## Opening a value

`i` views the value under the cursor full screen and never writes. `e` opens
the same view for editing. JSON is pretty printed with line numbers and syntax
highlighting; anything else is shown as it is.

Where the value can be written back — a real table row with a single-column
primary key, on a connection that is not read-only — it is editable in place:
`ctrl+s` saves, `esc` closes. JSON is validated before it is written.

Where it cannot, the same view opens read-only and the title and status say
why, rather than the key doing nothing.

In the read-only view the usual vim motions work: `j`/`k` move a line and take
a count (`3j`), `g` and `G` jump to the top and bottom, `12G` jumps to line 12,
`V` starts a line selection that `j`/`k` extend, and `y` yanks the selected
lines to the clipboard. `esc` clears a selection, and closes the view when
there is none.

The mouse works too: click a line to put the cursor on it, and drag to select a
range.

Yanking uses `pbcopy`, `wl-copy`, `xclip` or `xsel` when one is present, and
falls back to the OSC 52 terminal escape — which means it also works over
SSH.

## Exporting

From the command line, which is the scriptable way:

```bash
dotsql export prod orders --limit=1000 --sql=./orders.sql
dotsql export prod --sql=./prod.sql     # every table, one file
dotsql export prod --list               # what tables are there
```

`--sql` takes a file or a directory; omit it and the file is named
automatically in the export directory. `--limit` caps rows per table, which is
how you pull a slice of production rather than all of it.

Inside the interface, `:export` writes what you are looking at to a `.sql` file of `insert`
statements. On a table that is every row, read in chunks so a large table does
not go through memory at once; after a query it is the rows you have loaded.

Files land in `~/.config/dotsql/exports` (override with `ui.export_path`), named
`connection-table-YYYYMMDD-HHMMSS.sql`. The status line reports the row count,
file size and path.

Data only — no schema. Your migrations own the schema; this is for pulling rows
from one database into another.

## Editing

Select a cell and press `e` or `↵`. `↵` saves, `esc` cancels. An empty value
writes `NULL`.

Editing requires a single-column primary key, which dotsql uses to target the
row. Tables without one are read-only, as are connections flagged `read_only`.

## MCP

The MCP server is registered as a local (stdio) server named `dotsql`:

```bash
php dotsql mcp:start dotsql
```

Tools: list connections, list tables, describe a table, and run a query.
Queries through MCP are **read-only** — only `select`, `show`, `explain`,
`describe`, `pragma` and `with` are accepted, and statements containing a
second statement are rejected. Writes happen in the interface, not through
an agent.

## Rendering

Laravel Prompts repaints by erasing the frame and rewriting it, which flickers.
Every repaint is wrapped in synchronized output (`\e[?2026h` / `\e[?2026l`) so
the terminal presents the update atomically and the erase is never shown.
Terminals that do not support it ignore the sequence. Set `NO_SYNC_OUTPUT=1` to
turn it off.

## Storage

Connection passwords are encrypted with Laravel's encrypter using a key at
`~/.config/dotsql/key`. The key sits beside the database, so this protects
against casual reading of the file, not against someone with access to your
account.

## Tests

```bash
./vendor/bin/pest
```

Tests use `tests/.scratch` as their config directory and never touch your real
connections.

## Configuration

On first run dotsql writes `~/.config/dotsql/config.toml` containing every
setting at its default, each with a comment. When a later version adds a
setting, it is appended to your file on the next run — your values and your own
comments are left alone — and the status line says which ones arrived.

Defaults ship in `config/dotsql.php`. Machine-specific overrides go in
`~/.config/dotsql/config.toml`, which is merged over them — so settings that
depend on where dotsql runs stay out of the repo:

```toml
# dotsql configuration
# Anything omitted falls back to the shipped defaults.

[ui]

# Where the SQL editor sits when you press s: "top" or "bottom"
sql_position = "bottom"

# Keep the SQL editor on screen rather than only after pressing s
sql_always = true

# How many rows it takes, 0 picks a third of the frame
sql_height = 8

# Blank rows above the frame
top_margin = 0

# Subtract this from reported mouse rows.
# Set to 1 inside a multiplexer whose tab bar sits above the pane.
mouse_row_offset = 1
```

A file that cannot be parsed does not stop dotsql — it starts on the defaults
and reports the problem in the status line.

| Key | Default | Meaning |
| --- | --- | --- |
| `ui.sql_position` | `top` | `top` or `bottom` — where the SQL editor sits |
| `ui.sql_always` | `false` | keep the SQL editor on screen instead of only after `s` |
| `ui.sql_height` | 0 | rows for the SQL editor, 0 picks a third of the frame |
| `ui.row_style` | `marker` | how the current row is shown: `marker`, `dim-others`, `bold`, `inverse`, `underline` |
| `ui.top_margin` | 1 | blank rows above the frame |
| `ui.sidebar_width` | 24 | width of the tables pane |
| `ui.export_path` | `~/.config/dotsql/exports` | where `:export` writes files |
| `ui.mouse_row_offset` | 0 | rows to subtract from reported mouse coordinates |
| `ui.mouse_column_offset` | 0 | columns to subtract from reported mouse coordinates |

## Islands

The interface is composed of islands: independent bordered panes that each own
a rectangle on screen, render their own content, and answer hit tests for it.
`Screen` places them and composes the frame row by row.

This matters for correctness, not just layout. Screen geometry lives in one
place — the island — so clicking asks the island that drew the pixels rather
than re-deriving coordinates. Every mouse bug in this project came from having
two copies of that maths.

Adding a pane means adding an island.

## Notes on the stack

Laravel Zero strips `illuminate/encryption`, so it is required explicitly.

Laravel MCP's service provider registers web routes when `routes/ai.php` exists,
which needs a `router` binding Laravel Zero does not have. The server is
therefore registered from `AppServiceProvider::boot()` with `Mcp::local()` and
no routes file.

Three gaps found in `joetannenbaum/chewie` 0.1.11:

- `RegistersRenderers` resolves `Chewie\Theme::$namespace`, but `src/Theme.php`
  is not in the release, so calling `registerRenderer()` with no argument fatals.
  Pass the renderer class explicitly.
- `Input\Mouse` is three constants with no implementation.
- There is no mouse sequence parsing, so `App\Tui\Mouse` does it.

`Prompt::handleKeyPress()` is private, so it cannot be overridden. Keys are
handled by registering `$this->on('key', ...)`, and a prompt exits by setting
`$this->state = 'submit'`.
