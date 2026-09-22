# tql

A database client for the terminal, built with Laravel Zero, Laravel Prompts and Laravel MCP.

The same engine drives two faces: a full-screen terminal interface for you, and an
MCP server for an AI agent. Both call the same `QueryRunner`, and every statement
either of them runs is recorded in one shared history.

## Installing

Download the binary from the [latest release](https://github.com/VheissuLabs/tql/releases/latest):

```bash
curl -L -o tql https://github.com/VheissuLabs/tql/releases/latest/download/tql
chmod +x tql
mv tql /usr/local/bin/
```

It needs PHP 8.4 or newer on the machine. On first run it creates
`~/.config/tql/` and migrates its own store, so there is nothing to set up.

Building it yourself:

```bash
php -d phar.readonly=0 tql app:build tql --build-version=dev
./builds/tql --version
```

Tagging `v*` builds and publishes a release from GitHub Actions.

## A database to try it on

The sample database used in development is [Chinook](https://github.com/lerocha/chinook-database),
which is not committed — a database file changes every time you edit a row, and
a repository that reports itself modified after every demo is no use.

```bash
curl -L -o chinook.db https://github.com/lerocha/chinook-database/raw/master/ChinookDatabase/DataSources/Chinook_Sqlite.sqlite
tql chinook.db
```

## Running it

```bash
php tql
```

`browse` is the default command.

To skip the connection list and open a SQLite file straight away, point it at
the file:

```bash
tql test.sqlite          # same as: tql open test.sqlite
tql ~/Sites/app/db.sqlite
tql test.sqlite --tag=scratch    # and call it "scratch" in the list
```

A connection string works the same way:

```bash
tql 'mysql://user:pw@db.example.com:3306/shop'
tql 'postgres://user:pw@host/shop?name=Staging'
```

`mysql`, `mariadb`, `pgsql`, `postgres`, `postgresql`, `sqlsrv`, `mssql` and
`sqlite` schemes are understood. The default port is filled in per driver,
credentials are percent-decoded, and `?name=` sets the label shown in the
title bar.

Anything you open is **remembered**, so you only ever paste a connection string
once. `--tag=` names it in the list; without one it is named after the file, or
`database on host`. `--peek` opens without remembering, for a database you are
only glancing at. Re-opening somewhere you already have saved reuses that
connection rather than making a second, and `--tag=` on it is a rename. Names
are unique, so a second `database.sqlite` becomes `database.sqlite (2)`.

**A connection string on the command line goes into your shell history.** Paste
it once, then use the connection list, where the password is encrypted at rest.
Prefixing the command with a space keeps it out of history in zsh if
`HIST_IGNORE_SPACE` is set.

The file is not added to your saved connections unless you pass `--save`, so
poking at a one-off database does not clutter the list. A first argument that
exists on disk, contains a `/`, or ends in `.sqlite`, `.sqlite3` or `.db` is
treated as a path rather than a command name. On first run tql creates `~/.config/tql/`
containing `tql.sqlite` (connections and query history) and `key` (the
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

## Structure

`t` shows the table's structure: every column with its type, which one is the
primary key, which are foreign keys and where they point, what is not null,
what auto-increments, and the defaults — then the indexes.

```
STRUCTURE  ·  albums

  AlbumId    integer    primary key  ·  not null  ·  auto
  Title      text       not null
  ArtistId   integer    → artists.ArtistId  ·  not null

  indexes
    IFK_AlbumArtistId  (ArtistId)
```

`j`/`k` scroll it, `t`, `q` or `esc` close it.

## Following a link

Columns you can follow are marked with `→` in the header.

With the cursor on a foreign key, `L` opens the table it points at, filtered to
the row it points to. From anywhere else on the row, `L` goes the other way:
the tables that reference this one. If more than one does, it offers a list.

`ctrl+o` goes back where you came from, vim style.

The jump is an ordinary filter, so the SQL pane shows the `where` clause that
made it — following a link teaches you the query you would have written.

## Filtering rows

`f` opens a filter bar, TablePlus style: a column, an operator and a value.

```
┌─ FILTER ───────────────────────────────────────────────────────────┐
│                                                                    │
│  where  city              is               Toronto                 │
│  and    age               is at least      18                      │
│                                                                    │
│  ← → changes it    ↑↓ moves    + adds    - removes    ctrl+s applies│
└────────────────────────────────────────────────────────────────────┘
```

`tab` and `shift+tab` move between the three cells. On the column or operator,
`↵` opens a type-to-filter list — start typing to narrow it, arrows to move, `↵` to pick —
and `← →` step through the options without opening it. On the value, `↵` types.
`+` and `-` add and remove conditions, `o` switches the whole bar between `and`
and `or`, `ctrl+s` applies and `esc` clears.

The list behaves like Laravel Prompts' `search`, but it is drawn inside the
frame: Prompts' own `select` and `search` block the loop and render a frame of
their own, so using one would mean leaving the TUI and flashing the screen.

Operators: is, is not, contains, starts with, ends with, is greater than, is at
least, is less than, is at most, is empty, is not empty, is one of (a
comma-separated list).

The filter becomes a `where` clause on the query, so the SQL pane shows exactly
what ran — which is the point. **Values are bound, never interpolated**, so a
value containing a quote is a value rather than SQL. The pane shows the
statement with the values filled in for reading; that form is never sent to the
database.

Filters are dropped when you change table, since a column that exists in one
table usually does not in another.

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
a `where`, press `ctrl+r`, and the grid shows your version.

The pane always mirrors what you are looking at: change table, sort, or page and
it rewrites itself to the query that produced the rows on screen, discarding an
edit you never ran. While you are typing in it, nothing overwrites you.

## Asking for SQL

Press `a` and ask in plain english. The answer lands **in the editor**, with the
explanation as `--` comments above it, and nothing runs until you press
`ctrl+r`.

```
-- Counts how many invoices each customer has. The join matches each invoice to
-- its customer on customer_id, group by makes count() run per customer, and
-- order by puts the busiest first.
--
select c.name, count(i.id) as invoices
from customers c
join invoices i on i.customer_id = c.id
group by c.id, c.name
order by invoices desc
limit 50
```

The model writes queries; it never runs them. It is asked for exactly one
statement, reads only, and is told to use nothing outside the schema — and it
still lands in front of you for review rather than in front of your database.

**Only table and column names are sent.** No row data ever leaves the machine,
so asking about a production table does not send its contents anywhere. The
table you are looking at is sent first so it survives the size limit.

Press `a` and a modal opens with a text area. `↵` asks, `⇧↵` starts a new line,
`esc` cancels. `ctrl+s` sends it too.

Terminals send the same byte for enter and shift+enter, so shift+enter only
arrives as its own key when the terminal is told to send one. In Ghostty:

```
keybind = shift+enter=csi:13;2u
```

Alt+enter works without any configuration, if you would rather not set that.

## Which model answers

Whatever you have a key for. `provider = "auto"` picks the first provider the
AI SDK finds a key for, so setting `ANTHROPIC_API_KEY` or `OPENAI_API_KEY` is
all it takes:

```toml
[ai]
provider = "auto"   # or anthropic, openai, gemini, groq, mistral, deepseek, xai, openrouter, ollama
model = ""          # empty uses a sensible default for that provider
timeout = 60
```

### A local model

Anything with an OpenAI-compatible API works, which includes **LM Studio**,
vLLM and local gateways. Point `url` at it and name the model you loaded:

```toml
[ai]
url = "http://localhost:1234/v1"
model = "qwen2.5-coder-7b"
key = ""            # only if your endpoint wants a bearer token
```

A `url` wins over any provider key, so a local endpoint is used even when you
have hosted keys in the environment. Nothing leaves your machine at all in that
setup — and only table and column names were ever being sent anyway.

With nothing configured, `a` says what to set rather than failing at the
network.

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

## Inspecting a row

`i` opens the row you are on as one object: every column on its own line,
pretty printed, read only. It is the answer to a row being wider than the
screen — nothing is truncated into a cell.

```json
{
    "id": 1,
    "name": "user.signed_up",
    "payload": {
        "user": {
            "id": 42,
            "email": "karl@example.com"
        }
    },
    "created_at": "2026-09-22T15:11:32+00:00"
}
```

A json column is decoded on the way in, so it reads as part of the object
rather than as a string of json hiding inside it.

**Relations come with it**, the way an API resource would return them: the
record this row belongs to, and the records that belong to it, both found by
following foreign keys.

```json
{
    "id": 1,
    "name": "AC/DC",
    "albums": [
        { "id": 1, "title": "For Those About To Rock We Salute You" },
        { "id": 4, "title": "Let There Be Rock" }
    ]
}
```

`ui.inspect_related` caps how many related rows are loaded (10 by default, 0
turns it off). When there are more, a `<table>_truncated_at` key says so rather
than quietly showing you part of the story.

`I` opens just the value under the cursor, which is what `i` used to do. Both
scroll with `j`/`k`, select with `V` and yank with `y`. From either, `e` leaves
the viewer and opens the editor on the cell you were on.


## Exporting

From the command line, which is the scriptable way:

```bash
tql export prod orders --limit=1000 --sql=./orders.sql
tql export prod --sql=./prod.sql     # every table, one file
tql export prod --list               # what tables are there
```

`--sql` takes a file or a directory; omit it and the file is named
automatically in the export directory. `--limit` caps rows per table, which is
how you pull a slice of production rather than all of it.

Inside the interface, `:export` writes what you are looking at to a `.sql` file of `insert`
statements. On a table that is every row, read in chunks so a large table does
not go through memory at once; after a query it is the rows you have loaded.

Files land in `~/.config/tql/exports` (override with `ui.export_path`), named
`connection-table-YYYYMMDD-HHMMSS.sql`. The status line reports the row count,
file size and path.

Data only — no schema. Your migrations own the schema; this is for pulling rows
from one database into another.

## Editing

Select a cell and press `e` or `↵`. `↵` saves, `esc` cancels. An empty value
writes `NULL`.

Editing requires a single-column primary key, which tql uses to target the
row. Tables without one are read-only, as are connections flagged `read_only`.

## Connections

The first screen lists your saved connections.

| key | what it does |
| --- | --- |
| `↵` | open it |
| `e` | edit it in place |
| `n` | add one |
| `d` | mark it for deletion |
| `u` | clear every mark |
| `:w` | write the marked deletions |

Adding and editing happen in a modal over the list, never by dropping out to a
prompt sequence. `↑↓` picks a field, `↵` edits it with a real cursor (arrows,
home, end, backspace, delete, paste), `ctrl+s` saves and `esc` cancels. Nothing
is written until you save. The driver is cycled with `← →` and only offers
drivers your PHP build actually has.

## Pending changes

Nothing you do to a row reaches the database until you ask for it.

| key | what it does |
| --- | --- |
| `e` | edit the value; `ctrl+s` keeps the edit, pending |
| double click | the same, with the mouse |
| `d` | mark the row for deletion, and move down |
| `u` | drop every pending change |
| `:w` | write them all |

Edited rows are highlighted in `theme.edited` (yellow) and show the value you
typed rather than what is still on disk. Rows marked for deletion are
highlighted in `theme.deleted` (red). The status line counts both.

Changes are keyed by primary key, so sorting, filtering or reloading keeps them
on the rows you picked, and they are dropped when you change table — a mark
means nothing in a table where that id is a different row. Quitting with
unwritten changes drops them and says so; `:q` again leaves.

## Deleting

`d` marks the row under the cursor and moves down, so a run of rows is `ddd`.
Nothing is written yet: marked rows are highlighted in `theme.deleted` (red by
default), `d` again unmarks, and `u` clears every mark.

`:w` writes them, all in one transaction. Until then the database is untouched.

Marks follow the row, not its position, so sorting or reloading keeps them on
the rows you picked. Quitting with unwritten marks drops them and tells you,
rather than either losing them silently or writing something you did not ask
for — press `:q` again to leave.

A table with no single-column primary key cannot be deleted from, because there
is no safe way to name the row; it says so rather than guessing.

## MCP

The MCP server is registered as a local (stdio) server named `tql`:

```bash
php tql mcp:start tql
```

To use it from Claude Code:

```bash
claude mcp add tql -- php /absolute/path/to/tql mcp:start tql
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
`~/.config/tql/key`. The key sits beside the database, so this protects
against casual reading of the file, not against someone with access to your
account.

## Tests

```bash
./vendor/bin/pest
```

Tests use `tests/.scratch` as their config directory and never touch your real
connections.

## Configuration

On first run tql writes `~/.config/tql/config.toml` containing every
setting at its default, each with a comment. When a later version adds a
setting, it is appended to your file on the next run — your values and your own
comments are left alone — and the status line says which ones arrived.

Defaults ship in `config/tql.php`. Machine-specific overrides go in
`~/.config/tql/config.toml`, which is merged over them — so settings that
depend on where tql runs stay out of the repo:

```toml
# tql configuration
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

A file that cannot be parsed does not stop tql — it starts on the defaults
and reports the problem in the status line.

| Key | Default | Meaning |
| --- | --- | --- |
| `ui.sql_position` | `top` | `top` or `bottom` — where the SQL editor sits |
| `ui.sql_always` | `false` | keep the SQL editor on screen instead of only after `s` |
| `ui.sql_height` | 0 | rows for the SQL editor, 0 picks a third of the frame |
| `ui.row_style` | `marker` | how the current row is shown: `marker`, `dim-others`, `bold`, `inverse`, `underline` |
| `ui.top_margin` | 1 | blank rows above the frame |
| `ui.sidebar_width` | 24 | width of the tables pane |
| `ui.export_path` | `~/.config/tql/exports` | where `:export` writes files |
| `ui.mouse_row_offset` | 0 | rows to subtract from reported mouse coordinates |
| `ui.mouse_column_offset` | 0 | columns to subtract from reported mouse coordinates |

### Driver icons

Connections show a Nerd Font devicon beside the name, one per driver.
Change them in `[icons]` if you want different glyphs:

```toml
[icons]
mysql = ""
pgsql = ""
sqlite = ""
sqlsrv = ""
default = ""
```

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
