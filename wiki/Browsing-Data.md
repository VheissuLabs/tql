Open a connection and you land in the browser: a table list on the left, a grid
of rows on the right, and a status line under both. `\` hides the table list so
the grid gets the whole width, and `tab` moves between the panes. Each pane's
title shows its number, and `alt+1`, `alt+2` and `alt+3` jump to the table list,
the rows and the SQL editor. On macOS, turn on your terminal's "Option as Meta"
or "Option as Alt" for the alt keys.

Moving around is vim's where vim has an opinion:

| Key | Action |
| --- | --- |
| `↑ ↓` / `j k` | move the cursor; `5j` moves five rows, `3l` three columns |
| `← →` / `h l` | move between columns; `→` from the table list moves to the grid |
| `n` / `p` | next or previous page, 100 rows |
| `↵` | open the table, or edit the cell; on a schema header, fold or unfold it |
| `r` | reload the table |
| `y` / `Y` | yank this value, or the whole row as an object |

`Y` copies the row as JSON, which is the shape you want for a bug report or a
test fixture:

```json
{
    "id": 1,
    "name": "cherry",
    "qty": 3
}
```

`esc` is contextual, in this order: go back where you followed a link from,
clear the filter, then put the table back after a hand-written query replaced
the grid with its results. It never quits. `q` and `:q` do that.

The status line under the hotkeys says what just happened. It **lights up when it
changes** and settles back to dim on your next key press, in the color of what it
is about: red while rows are marked for deletion, yellow while edits are pending,
otherwise the focus color.

Beside the connection's name, the status line says where the cursor is among
all the table's rows: `8,501 of 50,000`. A filtered table, or one small enough
to count quickly, is counted exactly. A big Postgres, MySQL or SQL Server table
shows the database's own estimate instead, `~4.9M`, rather than wait on a full
count; ClickHouse knows its exact count. The count is taken once per table and
filter, and again on `r` or after a write.

The full key list is on [Keys](Keys).

## Mouse

On by default; `mouse = false` in [`[ui]`](Configuration#ui) gives the terminal
its selection and scrollback back.

| | |
| --- | --- |
| click | select a table, a row or a cell, and focus that pane; a click on a pane's border focuses it too |
| double click | edit the cell |
| click a header | sort by that column |
| drag a header border | resize the column |
| wheel | scroll the focused pane |

Inside a multiplexer whose tab bar sits above the pane, set `mouse_row_offset = 1`
or every click lands a row out.

## Sorting

Press `o` on a column, or click its header, to sort by it. The first press sorts
ascending, the second descending, and the third clears the sort. The header shows
`▲` or `▼`, and the `order by` appears in the SQL pane, so the sort teaches the
clause that produced it.

Sorting applies to a table, not to query results; those are ordered by whatever
your query says.

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

`f` opens on the column you are on, with `contains`, already typing. The column
and operator are guesses, the value never is. `↵` keeps the value and `↵` again
applies it; `esc` steps back into the form to move around.

`tab` and `shift+tab` move between the three cells. On the column or operator,
`↵` opens a type-to-filter list (start typing to narrow it, arrows to move, `↵`
to pick) and `← →` step through the options without opening it. On the value, `↵`
types. `+` and `-` add and remove conditions, `o` switches the whole bar between
`and` and `or`, `ctrl+s` applies and `esc` clears.

The list is drawn inside the frame, so picking from it never leaves the
interface.

Operators: is, is not, contains, starts with, ends with, is greater than, is at
least, is less than, is at most, is empty, is not empty, is one of (a
comma-separated list).

The filter becomes a `where` clause on the query, so the SQL pane shows exactly
what ran, which is the point. **Values are bound, never interpolated**, so a
value containing a quote is a value rather than SQL. The pane shows the statement
with the values filled in for reading; that form is never sent to the database.

Filters are dropped when you change table, since a column that exists in one
table usually does not in another.

## Seeing the query behind the view

With `ui.sql_always` on, the SQL pane shows the statement that produced what you
are looking at, and updates as you change table or page:

```
┌─ SQL ────────────────────────────────────────────┐
│ select * from "tracks" limit 100 offset 100      │
└──────────────────────────────────────────────────┘
```

Press `s` and that statement is handed to you to edit. Change the `limit`, add a
`where`, press `ctrl+r`, and the grid shows your version.

The pane always mirrors what you are looking at: change table, sort, or page and
it rewrites itself to the query that produced the rows on screen, discarding an
edit you never ran. While you are typing in it, nothing overwrites you.

## Following a link

While the cursor is on a foreign key, the status line says where it goes:
`L → artists`.

With the cursor on a foreign key, `L` opens the table it points at, filtered to
the row it points to. From anywhere else on the row, `L` goes the other way: the
tables that reference this one. If more than one does, it offers a list.

`esc` goes back where you came from, and so does `ctrl+o` if vim's jumplist is in
your fingers. The stack is as deep as you followed, so a chain of links unwinds
one step at a time.

The jump is an ordinary filter, so the SQL pane shows the `where` clause that
made it. Following a link teaches you the query you would have written.

## Structure

`t` shows the table's structure: every column with its type, which one is the
primary key, which are foreign keys and where they point, what is not null, what
auto-increments, and the defaults, then the indexes.

```
STRUCTURE  ·  albums

  AlbumId    integer    primary key  ·  not null  ·  auto
  Title      text       not null
  ArtistId   integer    → artists.ArtistId  ·  not null

  indexes
    IFK_AlbumArtistId  (ArtistId)
```

`j`/`k` scroll it, `t`, `q` or `esc` close it.

## Column widths

Column widths you set are remembered per column name, so several columns keep
their sizes at once and survive paging and switching tables.

| Key | Action |
| --- | --- |
| `,` `.` or `<` `>` | narrow or widen the column, or the table list when you are in it |
| `=` | reset the width |

With the mouse, **drag a column border in the header row** to resize it, as you
would in a spreadsheet.

## Viewing a value

`I` opens just the value under the cursor in a modal, read only, for a single
enormous blob. The inspector for a whole row is on [The row inspector](Row-Inspector).

### JSON columns

A cell holding JSON opens in a full-width modal when you press `i`: pretty
printed, with line numbers and syntax highlighting, with keys, strings, numbers
and literals each colored. `↑↓` scrolls a line at a time, `n`/`p` a page, `esc`
closes.

Detection is by parsing, not by column type, so JSON stored in a `text` column is
recognised too.

## The table list

The left pane lists the connection's tables. `/` filters it as you type, and `↵`
opens the highlighted table in the grid. Its width follows `sidebar_width` in
[`[ui]`](Configuration#ui), or drag its edge.

On Postgres, SQL Server and DuckDB, the table list shows every schema, not only
your default one. Each schema is a header you fold with `↵`, `h` and `l`; yours
comes first and open, the rest folded, and tql remembers what you fold per
connection and database. A table outside your default schema is called
`schema.table` everywhere: in the grid, the palette, links, exports,
`tql tables`, `tql describe`, `tql export` and the MCP tools, and completion
offers it that way. `/` searches every schema at once.
