# Keys

Every key tql answers to, by where you are. `?` shows an abridged version of
this inside the application.

Nothing here is rebindable yet. The bindings are vim's where vim has an opinion,
and a letter that says what it does where it does not.

## Browsing

| Key | Action |
| --- | --- |
| `tab` / `shift+tab` | next or previous pane |
| `↑ ↓` / `j k` | move the cursor |
| `← →` / `h l` | move between columns |
| `n` / `p` | next or previous page, 100 rows |
| `↵` | open the table, or edit the cell |
| `i` | inspect the row: its fields and its related records |
| `I` | view just this value, full screen, read only |
| `e` | edit the value |
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
| `N` | add a row: `e` fills it in, `:w` writes it |
| `d` / `u` | mark the row for deletion, or clear every mark |
| `,` `.` or `<` `>` | narrow or widen the column |
| `=` | reset the column width |
| `c` | back to the connection list |
| `ctrl+l` | redraw the screen |
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

## The command line

`:` opens it. Commands are exact, with short forms where it helps.

| Command | Action |
| --- | --- |
| `:q` `:quit` | quit |
| `:c` `:connections` | back to the connection list |
| `:w` `:write` | write pending edits and deletions |
| `:r` `:reload` | reload the table |
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
| | each relation says what it is: belongs to, has one, has many, or has many through a join table |
| `V` | start a line selection |
| `y` | yank the selection, or the line |
| `e` | edit *that* field, not whichever the grid cursor was on |
| `esc` | clear the selection, then close |

## The SQL editor

| Key | Action |
| --- | --- |
| `↵` | run the statement |
| `⇧↵` | add a line |
| `ctrl+r` | run it too, if your terminal eats `⇧↵` |
| `←` `→` `↑` `↓` | move the caret |
| `ctrl+b` / `ctrl+f` | left or right a character |
| `ctrl+a` / `ctrl+e` | start or end of the line |
| `tab` / `shift+tab` | leave for the next pane |
| `esc` | back to the grid |

`⇧↵` needs a terminal that distinguishes it. In Ghostty:

```
keybind = shift+enter=csi:13;2u
```

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

## Mouse

On by default; `mouse = false` in `[ui]` gives the terminal its selection and
scrollback back.

| | |
| --- | --- |
| click | select a table, a row or a cell |
| double click | edit the cell |
| click a header | sort by that column |
| drag a header border | resize the column |
| wheel | scroll the focused pane |

Inside a multiplexer whose tab bar sits above the pane, set
`mouse_row_offset = 1` or every click lands a row out.
