# Editing data

Select a cell and press `e` or `↵`. `↵` saves, `esc` cancels. An empty value
writes `NULL`. `E` edits the whole row in a form, see
[Editing a row in a form](#editing-a-row-in-a-form).

Editing requires a single-column primary key, which tql uses to target the row.
Tables without one are read-only, as are connections flagged `read_only`
(see [Tags and read only](Connecting#tags-and-read-only)).

## Pending changes

Nothing you do to a row reaches the database until you ask for it.

| key | what it does |
| --- | --- |
| `e` | edit the value; `ctrl+s` keeps the edit, pending |
| `E` | edit the whole row in a form; `ctrl+s` keeps it, pending |
| double click | the same, with the mouse |
| `d` | mark the row for deletion, and move down |
| `u` | drop every pending change |
| `:w` | write them all |

Edited rows are highlighted in `theme.edited` (yellow) and show the value you
typed rather than what is still on disk. Rows marked for deletion are highlighted
in `theme.deleted` (red). The status line counts both.

Changes are keyed by primary key, so sorting, filtering or reloading keeps them
on the rows you picked, and they are dropped when you change table, because a
mark means nothing in a table where that id is a different row. Quitting with
unwritten changes drops them and says so; `:q` again leaves.

## Adding a row

`N` opens a form over the grid, one field a line:

```
┌─ NEW ROW  ·  film ───────────────────────────────────────────────┐
│                                                                  │
│   film_id (integer): auto                                        │
│                                                                  │
│   title (varchar): required                                      │
│                                                                  │
│   rating (varchar): G                                            │
│                                                                  │
│   last_update (timestamp): now()                                 │
│                                                                  │
│   token (text): default gen_random_uuid()                        │
│                                                                  │
│                                                                  │
│  ↑↓ move  ↵ edit  ctrl+n null  ⌫ reset  ctrl+s keep  esc cancel  │
└──────────────────────────────────────────────────────────────────┘
```

`↵` types into a field and `↵` or `tab` keeps it and moves on; json, and a value
too long for its line, open in the value editor instead. `ctrl+s` keeps the row
and puts it on top of the grid, where you are already looking, drawn in the added
color. Nothing has happened until `:w`.

The form fills in what it can, so you can see it before you keep it:

- **A plain default** is the field's value, so `rating` starts as `G`. Change it
  or leave it.
- **A time default** (`current_timestamp`, `now()`, `datetime('now')`) starts as
  `now()`, which becomes the time when you keep the row, the way the column
  writes it.
- **A key the database will not give out** (a schema converted from somewhere
  that lost its auto increment, a table keyed by hand) is filled with the next
  number going, because otherwise you are looking up a value the database already
  knows.
- **Anything the database works out itself** (an auto-increment key, a sequence,
  a generated uuid) is left out of the insert and says so, dimmed.

A field that is not null and has nothing to fall back on says **required**, and
the cursor starts on the first field that is yours to fill in. `ctrl+n` sets a
field to `NULL` (an explicit one, written as `NULL` rather than left to the
default) and `⌫` puts a field back to how it started.

The row is a row like any other once it is kept: `e` changes one cell of it, `E`
opens it in the form again, and `u` drops it. The cursor is visible inside it. A
marked, edited or added row is drawn as one bar, and the cell you are on is a span
of its own inside it.

## Editing a row in a form

`e` edits one value. `E` opens the whole row in the same form, on the field for
the column you were on, with the row's values in it. `ctrl+s` keeps **only the
fields you changed**, as pending edits, and the row goes yellow in the grid. The
primary key is shown but not editable, because it is how tql names the row.

`esc` closes the form. If you changed something it asks first: `esc` again throws
the changes away, any other key keeps you in the form.

### The time

Any date, time or timestamp column takes **`ctrl+t`** while editing: it types the
current time into the editor, in the format that column wants (`Y-m-d` for a
date, `H:i:s` for a time, `Y-m-d H:i:s` for a datetime or timestamp), so you can
see it before keeping it. The status line says so while you are in one.

Typing **`now()`** does the same thing without the shortcut, wherever it is
typed, since an explicit `now()` is an instruction rather than a value. That is
how `created_at` and `updated_at` get filled in without looking up a format.

It writes **UTC**, because that is what a database column almost always holds and
a row written in local time is wrong in a way nobody notices for months. The
status line names the zone (`ctrl+t now UTC`) and `[ui] time_zone` in the config
changes it:

```toml
[ui]
time_zone = "America/Toronto"
```

`u` drops it, a reload keeps it, and changing table forgets it. It works on a
table with no primary key, which editing does not: a row that is not in the table
yet is written by position rather than by key.

## Deleting

`d` marks the row under the cursor and moves down, so a run of rows is `ddd`.
Nothing is written yet: marked rows are highlighted in `theme.deleted` (red by
default), `d` again unmarks, and `u` clears every mark.

`:w` writes them, all in one transaction. Until then the database is untouched.

Marks follow the row, not its position, so sorting or reloading keeps them on the
rows you picked. Quitting with unwritten marks drops them and tells you, rather
than either losing them silently or writing something you did not ask for. Press
`:q` again to leave.

A table with no single-column primary key cannot be deleted from, because there is
no safe way to name the row; it says so rather than guessing.

## When something goes wrong

A database refusing a write is the most important thing on the screen, so it
takes the screen: the error opens as a modal over whatever you were doing, with
the statement it refused on its own line and what to do next under it.

```
┌─ COULD NOT ADD THE ROW ──────────────────────────────────────┐
│                                                              │
│  UNIQUE constraint failed: people.email                      │
│                                                              │
│  insert into "people" ("name", "email") values (…)           │
│                                                              │
│  The row is still here. Fix it and :w again, or u to drop it.│
│                                                              │
│  y copies it    esc closes                                   │
└──────────────────────────────────────────────────────────────┘
```

The error is in the database's own words, with nothing of tql's wrapped around
it. `j`/`k` scroll a long one, `y` copies it, any other key closes it. **Your
pending changes are still pending**: nothing is dropped because a write failed.

The full key list for editing, forms and the time is on [Keys](Keys).
