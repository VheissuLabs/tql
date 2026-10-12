# The row inspector

`i` floats two boxes over the grid: the record itself, and the records related to
it. Folding a box collapses it to its title bar, so the shape of the modal tells
you what is open, and the table stays visible behind.

```
┌─ RECORD  (3) ───────────────────────────────────────────────┐
│  AlbumId      1                                     integer │
│  Title        For Those About To Rock We Salute     text    │
│  ArtistId     1                                     integer │
└─────────────────────────────────────────────────────────────┘

┌─ RELATED  (2) ──────────────────────────────────────────────┐
│  ▾ artists  (1)                                             │
│      ArtistId  Name                                         │
│      1         AC/DC                                        │
│  ▸ tracks  (10 of 47)                                       │
└─────────────────────────────────────────────────────────────┘
```

Column, value, then the type as an annotation. Related records stay as
collections: one header for the relation, then a line per record.

| Key | Action |
| --- | --- |
| `↵` / `space` | fold whichever box or relation the cursor is on |
| `j` / `k` | move |
| `g` / `G` | jump to the top or bottom |
| `tab` / `⇧tab` | switch between the record and its related records |
| `V` | start a line selection |
| `y` | yank the selection, or the line |
| `e` | edit **that** field, not whichever one the grid cursor was on |
| `i` | on a related record, inspect that record; `esc` comes back |
| `esc` | clear the selection, then close |

Related tables start collapsed, and load their rows when you open them.

## Relations

Each relation says what it is: **belongs to** when this row holds the key, **has
many** for the rows pointing back, and **has one** when a unique key on the other
side means there can only be one. A single record reads as fields; a list reads
as a table.

A **join table is read through**, not shown: a film's `film_actor` rows are a
list of timestamps, so tql shows `actors · has many through film_actor` with the
actors in it. A join table that carries data of its own (a quantity, a price) is
a table in its own right and is shown as one.

Relations are found by following foreign keys both ways.

## Layout

Every column of a related row is shown, laid out to the width you actually have:
the columns that already fit are left alone and what is left over is shared
between the ones that do not, with an `…` where a value was cut. Nothing is
dropped, so the row you are reading is the row that is there, and `i` on the row
in its own table opens the whole of it. Widen the terminal and the columns widen
with it.

`ui.inspect_related` caps how many rows are loaded (10 by default, 0 turns it
off); when there are more, the heading says `(10 of 47)`.

`I` opens just the value under the cursor, for a single enormous blob.

## A JSON value

A cell holding JSON opens in a full-width modal when you press `i`: pretty
printed, with line numbers and syntax highlighting. `↑↓` scrolls a line at a
time, `n`/`p` a page, `esc` closes. Detection is by parsing, not by column type,
so JSON stored in a `text` column is recognised too. See
[Browsing data](Browsing-Data#json-columns).
