# Keys

Every key tql answers to, by where you are. `?` shows an abridged version of
this inside the application.

The bindings are vim's where vim has an opinion, and a letter that says what it
does where it does not. **They can be changed** — see [Rebinding](#rebinding).

## Browsing

| Key | Action |
| --- | --- |
| `tab` / `shift+tab` | next or previous pane; in the SQL editor they indent instead |
| `alt+1` / `alt+2` / `alt+3` | go to the table list, the rows or the SQL editor — each pane's title shows its number. On macOS, turn on your terminal's "Option as Meta" or "Option as Alt" |
| `\` | hide or show the table list, so the grid has the whole width |
| `↑ ↓` / `j k` | move the cursor; a count before it moves that far, so `5j` is five rows and `3l` three columns |
| `← →` / `h l` | move between columns; `→` from the table list moves to the grid |
| `n` / `p` | next or previous page, 100 rows |
| `↵` | open the table, or edit the cell |
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
| `y` / `Y` | yank this value, or the whole row as an object |
| `N` | add a row, in a form; `:w` writes it |
| `d` / `u` | mark the row for deletion, or clear every mark |
| `,` `.` or `<` `>` | narrow or widen the column — or the table list, when you are in it |
| `=` | reset the width |
| `c` | back to the connection list |
| `ctrl+l` | redraw the screen |
| `ctrl+k` | the command palette: every action, the tables, databases and connections |
| `?` | help |
| `:` | the command line |
| `q` | quit |

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
the grid with its results. It never quits — `q` and `:q` do that.

## The command palette

`ctrl+k` opens one list of everything you can do from here: every action, with
the key that does it, the `:` commands that have no key, the tables, the other
databases on a server, and your other connections. Type to narrow it — exact and
prefix matches first, then words, then letters in order — and `↵` runs the one
you are on, exactly as its key would.

| Key | Action |
| --- | --- |
| type | narrow the list |
| `↑ ↓` / `ctrl+p` `ctrl+n` | move |
| `↵` | run it |
| `esc` | close |

The key shown beside an action is the one it is bound to, so a key you rebind is
the one it offers. `palette = "ctrl+p"` in `[keys]` moves the palette itself.

## The command line

`:` opens it. Commands are exact, with short forms where it helps.

| Command | Action |
| --- | --- |
| `:q` `:quit` | quit |
| `:c` `:connections` | back to the connection list |
| `:w` `:write` | write pending edits and deletions |
| `:r` `:reload` | reload the table; from the SQL editor, `:r` runs the statement |
| `:run` | run the SQL editor's statement |
| `:sql` | open the SQL editor |
| `:tables` | focus the table list |
| `:rows` | focus the rows |
| `:export` | write this table to a `.sql` file |

## Viewing a value

`i` on a row opens the inspector; `I` opens the value under the cursor.

| Key | Action |
| --- | --- |
| `j` / `k` | move a line — `3j` moves three |
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

`↵` never runs the statement, so a long query can take as many lines as it
needs. In the simple style `ctrl+r` runs it; in the vim style `:r` does, and
`ctrl+r` is redo.

With several statements in the editor, separated by `;`, only the one the
cursor is in runs, and the status line says which: `statement 2 of 3`. A `;`
inside quotes, a comment or a Postgres `$$` body does not count.

A new line starts at the indentation of the one before it.

The editor has two styles, picked with `sql_editor` in `[ui]`. `"simple"` is
the default.

#### `sql_editor = "simple"`

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

`tab` stays in the editor. Leave it with `esc`, `alt+1` / `alt+2`, or a click
on another pane.

#### `sql_editor = "vim"`

The editor opens in normal mode, and its title says which mode you are in:
`SQL · NORMAL`, `SQL · INSERT`, `SQL · VISUAL` or `SQL · VISUAL LINE`.

`:r` or `:run` runs the statement under the cursor. With a selection, `:r`
runs only what is selected. Away from the SQL editor `:r` still reloads the table, and the
command line says which one it is about to do.

Most keys take a count: `3j`, `5w`, `2dd`. An operator takes one on either
side, and they multiply, so `2d3w` deletes six words.

| Moving | To |
| --- | --- |
| `h` `j` `k` `l` | a character or a line; the arrows too |
| `w` `b` `e` | the next word, the start of this or the last one, the end of one |
| `W` `B` `E` | the same, where only spaces break a word |
| `0` `^` `$` | the line's start, its first non-blank, its last character |
| `gg` / `G` | the first or last line; `5G` goes to line 5 |
| `f` `t` + a character | onto, or just before, the next one on the line |
| `F` `T` + a character | the same, going back |
| `;` / `,` | the last `f` `t` `F` `T` again, the same way or the other |
| `%` | the bracket that matches this one |
| `{` / `}` | the blank line before or after this paragraph |
| `↵` | the first non-blank of the next line |

| Operator | Does | On the line |
| --- | --- | --- |
| `d` + a motion | delete | `dd` |
| `c` + a motion | delete and insert | `cc` |
| `y` + a motion | copy, to the clipboard as well | `yy` |
| `>` / `<` + a motion | indent or outdent two spaces | `>>` / `<<` |

An operator also takes a text object: `iw` `aw` a word, `iW` `aW` a word up
to spaces, `i(` `a(` (or `ib`) brackets, `i[` `a[`, `i{` `a{` (or `iB`), `i"`
`a"` `i'` `a'` ``i` `` ``a` `` quotes on the line, and `ip` `ap` a paragraph.
`i` is inside, `a` takes the edges too. `ci(` rewrites a subquery, even one
written over several lines.

| Key | Action |
| --- | --- |
| `x` / `X` | delete the character under or before the cursor |
| `D` / `C` / `Y` | delete, change or copy to the end of the line (`Y` the whole line) |
| `s` / `S` | change the character, or the line |
| `r` + a character | replace the one under the cursor |
| `J` | join the next line on, with a space |
| `~` | swap the case, and move on |
| `p` / `P` | put after or before; lines go below or above |
| `u` / `ctrl+r` | undo, redo |
| `.` | do the last change again, with a new count if you give one |
| `i` / `a` | insert before or after the cursor |
| `I` / `A` | insert at the start or end of the line |
| `o` / `O` | open a line below or above, and insert |
| `v` / `V` | select characters, or lines |
| `:` | the command line |
| `tab` / `shift+tab` | leave for the next pane |
| `esc` | back to the grid |

| Visual mode | Action |
| --- | --- |
| a motion or text object | grow the selection |
| `o` | go to the other end |
| `d` `x` / `y` / `c` | delete, copy or change it |
| `>` `<` / `~` / `J` | indent, outdent, swap case, join |
| `:r` | run only the selection |
| `v` / `V` | switch kind, or leave when it is the same |
| `esc` | back to normal mode |

| Insert mode | Action |
| --- | --- |
| `↵` | add a line |
| `tab` / `shift+tab` | indent or outdent the line two spaces |
| `ctrl+w` / `ctrl+u` | delete the word before the cursor, or back to the line's start |
| `esc` | back to normal mode |

An insert counts as one change, so `u` takes back everything typed since `i`.

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
| `esc` | cancel — twice, if you changed something |

While typing, `↵` or `tab` keeps the field and moves on, `shift+tab` moves back,
`ctrl+t` types the time and `esc` puts the field back.

## Filtering rows

`f` opens the filter, already typing in the value, on the column you were on,
with `contains` as the operator.

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
`esc` clears the filter.

## An error

It opens over everything and takes every key until it is closed, so nothing
happens behind it.

| Key | Action |
| --- | --- |
| `j` / `k` | scroll a long message |
| `y` | copy it |
| anything else | close it |

## Lists

Every list — a column, an operator, a database, a link, a file, a tag —
answers the same way.

| Key | Action |
| --- | --- |
| `j` / `k` | move |
| type | narrow the list |
| `↵` | take it |
| `esc` | close it |

## The connection list

| Key | Action |
| --- | --- |
| `↑ ↓` / `j k` | move |
| `↵` | open it |
| `n` | a new connection |
| `e` | edit this one |
| `d` / `u` | mark for deletion, or clear the marks |
| `:w` | write the marked deletions |
| `q` / `esc` | quit |

### Inside a connection form

| Key | Action |
| --- | --- |
| `j` / `k` | move between fields |
| `↵` / `i` | start typing, or open the list on a field that has one |
| `h` / `l` | change a fixed value: the driver, SSL mode, over SSH, read only |
| `ctrl+s` | save, from anywhere, keeping what you just typed |
| `esc` | cancel |

## The status line

The line under the hotkeys says what just happened. It **lights up when it
changes** and settles back to dim on your next key press, in the color of what
it is about: red while rows are marked for deletion, yellow while edits are
pending, otherwise the focus color.

## Rebinding

Every action in the grid has a name, and `[keys]` in `~/.config/tql/config.toml`
says which key asks for it:

```toml
[keys]
filter_rows = "F"
new_row = "ctrl+n"
ask = "?"
```

A key is a single character (`F` is not `f`), `ctrl+<letter>`, `alt+<key>`, or one of `tab`,
`shift+tab`, `enter`, `escape`, `space`, `backspace`, `delete`, `up`, `down`,
`left`, `right`, `home`, `end`. A list means **several keys that all do the same thing** —
`yank_value = ["y", "ctrl+y"]` makes both yank, the way `,` and `<` both narrow
a column by default. The first one is what help and the hotkey bar show.

Help and the hotkey bar read the same list, so a rebound key is the key they
offer. A key two actions both want goes to the first one and tql says so, in
the status line when it starts and in `tql config`. A key it cannot make sense
of is ignored and the default stands.

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
| `help` | `?` | help |
| `quit` | `q` | quit |

Movement (`j k h l`, the arrows), `↵`, `esc`, `tab`, `:` and `ctrl+l` are fixed:
they are what a terminal and a vim user both already assume, and rebinding them
breaks more than it fixes.

## Mouse

On by default; `mouse = false` in `[ui]` gives the terminal its selection and
scrollback back.

| | |
| --- | --- |
| click | select a table, a row or a cell, and focus that pane; a click on a pane's border focuses it too |
| double click | edit the cell |
| click a header | sort by that column |
| drag a header border | resize the column |
| wheel | scroll the focused pane |

Inside a multiplexer whose tab bar sits above the pane, set
`mouse_row_offset = 1` or every click lands a row out.
