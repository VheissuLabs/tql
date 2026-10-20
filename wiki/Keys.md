Every key tql answers to, by where you are. `?` shows an abridged version of this
inside the application.

The bindings are vim's where vim has an opinion, and a letter that says what it
does where it does not. **They can be changed**, see [Rebinding](#rebinding).

## Browsing

| Key | Action |
| --- | --- |
| `tab` / `shift+tab` | next or previous pane; in the SQL editor they indent instead |
| `alt+1` / `alt+2` / `alt+3` | go to the table list, the rows or the SQL editor. Each pane's title shows its number. On macOS, turn on your terminal's "Option as Meta" or "Option as Alt" |
| `\` | hide or show the table list, so the grid has the whole width |
| `↑ ↓` / `j k` | move the cursor; a count before it moves that far, so `5j` is five rows and `3l` three columns |
| `← →` / `h l` | move between columns; `→` from the table list moves to the grid |
| `n` / `p` | next or previous page, 100 rows |
| `↵` | open the table, or edit the cell; on a schema header, fold or unfold it |
| `h` / `l` on a schema header | fold or unfold the schema; `h` on a table folds the schema it is in |
| `i` | inspect the row: its fields and its related records |
| `I` | view just this value in a modal, read only |
| `e` | edit the value |
| `E` | edit the whole row in a form |
| `o` | sort by this column: ascending, descending, primary key |
| `r` | reload the table |
| `b` | switch database on this server |
| `t` | structure: columns, types, keys and indexes |
| `f` | filter the rows |
| `/` | filter the table list |
| `L` | follow a link: the key under the cursor, or what points here |
| `esc` / `ctrl+o` | go back where you followed from |
| `a` | ask for a query in plain english |
| `s` | the SQL editor |
| `H` / `alt+h` | the query history; `alt+h` works from inside the SQL editor too |
| `y` / `Y` | yank this value, or the whole row as an object |
| `N` | add a row, in a form; `:w` writes it |
| `d` / `u` | mark the row for deletion, or clear every mark |
| `,` `.` or `<` `>` | narrow or widen the column, or the table list when you are in it |
| `=` | reset the width |
| `c` | back to the connection list |
| `ctrl+l` | redraw the screen |
| `ctrl+k` | the command palette: every action, the tables, databases and connections |
| `?` | help |
| `:` | the command line |
| `q` | quit |

`Y` copies the row as JSON, which is the shape you want for a bug report or a test
fixture:

```json
{
    "id": 1,
    "name": "cherry",
    "qty": 3
}
```

`esc` is contextual, in this order: go back where you followed a link from, clear
the filter, then put the table back after a hand-written query replaced the grid
with its results. It never quits. `q` and `:q` do that.

## The command palette

`ctrl+k` opens one list of everything you can do from here: every action, with the
key that does it, the `:` commands that have no key, the tables, the other
databases on a server, and your other connections. Type to narrow it (exact and
prefix matches first, then words, then letters in order) and `↵` runs the one you
are on, exactly as its key would.

| Key | Action |
| --- | --- |
| type | narrow the list |
| `↑ ↓` / `ctrl+p` `ctrl+n` | move |
| `↵` | run it |
| `esc` | close |

The key shown beside an action is the one it is bound to, so a key you rebind is
the one it offers. `palette = "ctrl+p"` in `[keys]` moves the palette itself.

## The command line

`:` opens it. Commands are exact, with short forms where it helps. The full list
is on [Commands](Commands#in-the-interface).

| Command | Action |
| --- | --- |
| `:q` `:quit` | quit |
| `:c` `:connections` | back to the connection list |
| `:w` `:write` | write pending edits and deletions |
| `:r` `:reload` | reload the table; from the SQL editor, `:r` runs the statement |
| `:sql` | open the SQL editor |
| `:history` | the query history |
| `:tables` | focus the table list |
| `:rows` | focus the rows |
| `:export` | write this table to a `.sql` file |
| `:export database` | write every table in this database to one `.sql` file |
| `:create database <name>` | create a database on this server and switch to it |
| `:drop database <name>` | drop a database on this server; drop the one in use and the database list opens |
| `:settings` | change your settings |
| `:v` `:version` | show which version of tql this is |
| `:update` | check for a newer tql and install it, while you keep working |
| `:open <path or connection string>` | open a SQLite or DuckDB file or a `mysql://`, `pgsql://`, `sqlsrv://`, `libsql://`, `clickhouse://`, `snowflake://`, `redis://` or `valkey://` URL, saved to the connection list as `tql open` does |
| `:h` `:help` | the keys and commands |
| `:sign-out` | on the connection list, forget the Snowflake browser sign-in saved with the highlighted connection |

A command that belongs to the other screen says so: `:sql` on the connection list
says it works once a connection is open.

## Viewing a value

`i` on a row opens the inspector; `I` opens the value under the cursor.

| Key | Action |
| --- | --- |
| `j` / `k` | move a line, so `3j` moves three |
| `g` / `G` | top or bottom |
| `12G` | jump to line 12 |
| `↵` | fold or unfold a section |
| `tab` / `⇧tab` | switch between the record and its related records |
| | each relation says what it is: belongs to, has one, has many, or has many through a join table |
| | related tables start collapsed, and load their rows when you open them |
| `i` | on a related record, inspect that record; `esc` comes back |
| `V` | start a line selection |
| `y` | yank the selection, or the line |
| `e` | edit *that* field, not whichever the grid cursor was on |
| `esc` | clear the selection, then close |

## The SQL editor

`↵` never runs the statement, so a long query can take as many lines as it needs.
In the simple style `ctrl+r` runs it; in the vim style `:r` does, and `ctrl+r` is
redo. The full guide is on [The SQL editor](SQL-Editor).

In vim insert mode the cursor is a line, like vim's, and back to a block in normal
and visual mode. A terminal that ignores cursor shapes shows its own.

With several statements in the editor, separated by `;`, only the one the cursor
is in runs, and the status line says which: `statement 2 of 3`. A `;` inside
quotes, a comment or a Postgres `$$` body does not count.

### `sql_editor = "simple"`

| Key | Action |
| --- | --- |
| `ctrl+r` | run the statement under the cursor |
| `↵` | add a line |
| `ctrl+z` / `ctrl+y` | undo, redo, a word at a time |
| `←` `→` `↑` `↓` | move the caret |
| `ctrl+b` / `ctrl+f` | left or right a character |
| `ctrl+a` / `ctrl+e` | start or end of the line |
| `tab` / `shift+tab` | indent or outdent the line two spaces |
| `esc` | back to the grid |

### `sql_editor = "vim"`

The editor opens in normal mode, and its title says which mode you are in.
`:r` or `:run` runs the statement under the cursor. The full motion, operator,
text object, visual and insert tables are on
[The SQL editor](SQL-Editor#sql_editor--vim).

| Key | Action |
| --- | --- |
| `u` / `ctrl+r` | undo, redo |
| `i` / `a` | insert before or after the cursor |
| `v` / `V` | select characters, or lines |
| `:` | the command line |
| `tab` / `shift+tab` | leave for the next pane |
| `esc` | back to the grid |

### While editing a value

| Key | Action |
| --- | --- |
| `↵` | keep it |
| `⇧↵` | add a line |
| `ctrl+t` | type the time, in the format the column takes |
| `esc` | cancel |

`now()` typed into a cell means the same as `ctrl+t`. Both write UTC unless
`[ui] time_zone` says otherwise, and the status line names the zone.

### Inside a row form

`N` opens one for a new row, `E` for the row under the cursor.

| Key | Action |
| --- | --- |
| `↑ ↓` / `j k` / `tab` | move between fields |
| `g` / `G` | the first or last field |
| `↵` / `e` | start typing; json and long values open in the value editor |
| `ctrl+n` | set the field to `NULL` |
| `⌫` | put the field back the way it was |
| `ctrl+s` | keep the row, pending, from anywhere |
| `esc` | cancel, twice if you changed something |

While typing, `↵` or `tab` keeps the field and moves on, `shift+tab` moves back,
`ctrl+t` types the time and `esc` puts the field back.

## Filtering rows

`f` opens the filter, already typing in the value, on the column you were on, with
`contains` as the operator.

| Key | Action |
| --- | --- |
| `↵` | leave the input; a second `↵` applies the filter |
| `esc` | leave the input, then close the filter |
| `h` / `l` | move between column, operator and value |
| `j` / `k` | move between conditions |
| `tab` / `shift+tab` | move between cells |
| `↵` on a column or operator | open a list you can type to narrow |
| `n` / `+` | another condition |
| `d` / `-` | drop this one |
| `o` | toggle `and` / `or` |
| `ctrl+s` | apply from anywhere |

Any other letter typed on the value goes straight into it. On a filtered table,
`esc` clears the filter. See [Filtering rows](Browsing-Data#filtering-rows).

## An error

It opens over everything and takes every key until it is closed, so nothing
happens behind it.

| Key | Action |
| --- | --- |
| `j` / `k` | scroll a long message |
| `y` | copy it |
| anything else | close it |

## Lists

Every list answers the same way, whether it holds columns, operators, databases,
links, files or tags.

| Key | Action |
| --- | --- |
| `↑ ↓` / `ctrl+p` `ctrl+n` | move |
| type | narrow the list. Every letter types, `j`, `k` and `q` included |
| `↵` | take it |
| `esc` | close it |

The database list does two things more, unless the connection is read-only. Type
a name that is not there and the last row offers `+ create` it. On a connection
tagged `production`, taking it puts `:create database` and the name on the command
line instead, so nothing is created until `↵` there (`confirm_create_on_production`
in [the config](Configuration#ui) turns that off). `ctrl+d` on a database puts
`:drop database` and its name on the command line, so nothing is dropped until `↵`
there.

## The key browser

A Redis or Valkey connection opens the key browser. See
[Redis and Valkey](Redis-and-Valkey).

| Key | Action |
| --- | --- |
| `j k` / `↑ ↓` | move |
| `↵` / `l` | open a key, or unfold a namespace |
| `h` | fold the namespace |
| `tab` | between the keys and the value |
| `/` | find keys by pattern: `user:*`, or words, which become `*words*` |
| `b` | switch database |
| `r` | scan again |
| `N` | on the keys, a new key; on a value, a new field, item or member |
| `e` | edit the value, field, item, member or score |
| `d` | mark a key, or a value's field, item or member, for removal |
| `T` | set the key's TTL, or on a hash field, that field's (Redis 7.4, Valkey 9); empty keeps it forever |
| `:log` | the server's recent slow commands |
| `R` | rename the key |
| `n` / `p` | next or previous page of a value |
| `:w` | write every pending change in one transaction |
| `u` | drop the pending changes |
| `s` | the command pane; `ctrl+r` runs the line, `esc` closes it |
| `ctrl+k` | the command palette, keys included |

## The connection list

| Key | Action |
| --- | --- |
| `↑ ↓` / `j k` | move |
| `↵` | open it, or fold a group on its header |
| `space` | fold or unfold the group |
| `h` / `l` | fold the group you are in, or unfold the one under the cursor |
| `n` | a new connection |
| `e` | edit this one, or rename the group on its header |
| `y` | yank the database name, or the path for SQLite |
| `Y` | yank the connection string, password and all |
| `d` / `u` | mark for deletion, or clear the marks |
| `:w` | write the marked deletions |
| `:sign-out` | forget the Snowflake browser sign-in saved with this connection |
| `ctrl+k` | the command palette: these commands and every connection, by name |
| `q` / `esc` | quit |

Give a connection a **Group** in its form and it sits under that group's header,
which folds shut; tql remembers which ones you folded. The number in brackets on a
header is how many connections the group holds.

`e` on a header renames the group for every connection in it: `↵` renames, `esc`
cancels. A name another group already has merges the two.

### Inside a connection form

| Key | Action |
| --- | --- |
| `j` / `k` | move between fields |
| `↵` / `i` | start typing, or open the list on a field that has one |
| `h` / `l` | change a fixed value: the driver, SSL mode, over SSH, read only |
| `ctrl+s` | save, from anywhere, keeping what you just typed |
| `esc` | cancel |

## Settings

`:settings`, `tql settings` or Settings in the palette.

| Key | Action |
| --- | --- |
| `↑ ↓` / `j k` | move |
| `↵` / `space` | fold a section, flip a setting that is on or off, step a choice, or type a value |
| `← →` / `h l` | step through a setting's choices, or fold and unfold a section |
| `x` | put the setting back to its default |
| `/` | find a setting by name; `esc` clears it |
| `esc` / `q` | close |

A value you have changed from the default is drawn in the `edited` colour.

## Rebinding

Every action in the grid has a name, and `[keys]` in `~/.config/tql/config.toml`
says which key asks for it:

```toml
[keys]
filter_rows = "F"
new_row = "ctrl+n"
ask = "?"
```

A key is a single character (`F` is not `f`), `ctrl+<letter>`, `alt+<key>`, or one
of `tab`, `shift+tab`, `enter`, `escape`, `space`, `backspace`, `delete`, `up`,
`down`, `left`, `right`, `home`, `end`. A list means **several keys that all do
the same thing**. `yank_value = ["y", "ctrl+y"]` makes both yank, the way `,` and
`<` both narrow a column by default. The first one is what help and the hotkey bar
show.

Help and the hotkey bar read the same list, so a rebound key is the key they
offer. A key two actions both want goes to the first one and tql says so, in the
status line when it starts and in `tql config`. A key it cannot make sense of is
ignored and the default stands.

| Action | Default | |
| --- | --- | --- |
| `inspect_row` | `i` | inspect the row |
| `view_value` | `I` | view just this value |
| `edit_value` | `e` | edit the value |
| `edit_row` | `E` | edit the row in a form |
| `sort_column` | `o` | sort this column |
| `mark_delete` | `d` | mark the row for deletion |
| `clear_marks` | `u` | drop every pending change |
| `new_row` | `N` | add a row |
| `ask` | `a` | ask for a query |
| `filter_rows` | `f` | filter the rows |
| `filter_tables` | `/` | filter the table list |
| `structure` | `t` | structure |
| `databases` | `b` | switch database |
| `sql` | `s` | the SQL editor |
| `history` | `H` `alt+h` | the query history |
| `follow_link` | `L` | follow a link |
| `jump_back` | `ctrl+o` | go back |
| `next_page` / `previous_page` | `n` / `p` | paging |
| `reload` | `r` | reload |
| `connections` | `c` | the connection list |
| `yank_value` / `yank_row` | `y` / `Y` | yank |
| `narrow` / `widen` | `,` `<` / `.` `>` | column width |
| `reset_width` | `=` | reset the column width |
| `palette` | `ctrl+k` | the command palette |
| `focus_tables` / `focus_rows` / `focus_sql` | `alt+1` / `alt+2` / `alt+3` | go to a pane |
| `toggle_tables` | `\` | hide or show the table list |
| `toggle_sql` | `\|` | hide or show the SQL editor |
| `help` | `?` | help |
| `quit` | `q` | quit |

Movement (`j k h l`, the arrows), `↵`, `esc`, `tab`, `:` and `ctrl+l` are fixed:
they are what a terminal and a vim user both already assume, and rebinding them
breaks more than it fixes.

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

Inside a multiplexer whose tab bar sits above the pane, set
`mouse_row_offset = 1` or every click lands a row out.
