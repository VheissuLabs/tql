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
| `i` | inspect the selected cell in full, wrapped |
| `e` | edit the selected cell |
| `< >` | narrow or widen the selected column |
| `=` | reset the column width |
| `n` / `p` | next or previous page (100 rows) |
| `r` | reload the current table |
| `s` | open the SQL editor |
| `:` | command line — `:q`, `:tables`, `:rows`, `:reload`, `:sql` |
| `q` / `esc` | quit |

Mouse works too: click a table or a row, scroll with the wheel, and **drag a
column border in the header row to resize it**, as you would in a spreadsheet.

Column widths you set are remembered per column name, so several columns keep
their sizes at once and survive paging and switching tables.

## Running SQL

Press `s` (or `:sql`) for the SQL editor, which opens above the results.
`ctrl+r` runs what you have typed, `esc` returns to browsing. Results replace
the grid and are read-only, since they have no primary key to write back
through — open a table to edit.

## Reading long values

Columns take the width they need when nothing competes for it, and leftover
space is handed back to the visible columns. When a value is still too long,
`i` opens it in full in its own pane, wrapped, with its length in the status
line. `esc` closes it.

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

`config/dotsql.php`:

| Key | Default | Meaning |
| --- | --- | --- |
| `ui.top_margin` | 1 | blank rows above the frame |
| `ui.sidebar_width` | 24 | width of the tables pane |
| `ui.mouse_row_offset` | 0 | rows to subtract from reported mouse coordinates |
| `ui.mouse_column_offset` | 0 | columns to subtract from reported mouse coordinates |

Set `mouse_row_offset` to `1` when running inside a multiplexer whose own chrome
(a tab bar, for instance) occupies rows above the pane and whose mouse
coordinates are not translated. `:mouse` inside the app shows the raw and
adjusted coordinates side by side so the right value is obvious.

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
