Press `s` (or `:sql`) for the SQL editor, which opens above the results.
`ctrl+r` runs what you have typed, `esc` returns to browsing. Results replace the
grid and are read-only, since they have no primary key to write back through.
Open a table to edit.

The editor is syntax highlighted: keywords, quoted identifiers, strings, numbers
and comments each colored, the same tokeniser approach as the JSON viewer and
equally careful never to drop a character while you type.

On a read-only connection the editor runs only statements that read.

## Running a statement

`↵` adds a line in the editor; it never runs the statement, so a long query can
take as many lines as it needs. `ctrl+r` runs it.

With several statements in the editor, separated by `;`, only the one the cursor
is in runs, and the status line says which: `statement 2 of 3`. A `;` inside
quotes, a comment or a Postgres `$$` body does not count.

Elsewhere in tql, `↵` does the thing and `⇧↵` adds a line. The SQL editor is the
exception, and only `ctrl+r` runs the statement under the cursor, so a long query
never goes off half written and a buffer of several statements runs one at a
time.

## Two styles

The editor has two styles, picked with `sql_editor` in
[`[ui]`](Configuration#ui). `"simple"` is the default; `"vim"` is vim.

In vim insert mode the cursor is a line, like vim's, and back to a block in
normal and visual mode. A terminal that ignores cursor shapes shows its own.

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

`tab` stays in the editor. Leave it with `esc`, `alt+1` / `alt+2`, or a click on
another pane.

### `sql_editor = "vim"`

The editor opens in normal mode, and its title says which mode you are in:
`SQL · NORMAL`, `SQL · INSERT`, `SQL · VISUAL` or `SQL · VISUAL LINE`.

`:r` or `:run` runs the statement under the cursor, and `ctrl+r` is redo. With a
selection, `:r` runs only what is selected. Away from the SQL editor `:r` still
reloads the table, and the command line says which one it is about to do.

Most keys take a count: `3j`, `5w`, `2dd`. An operator takes one on either side,
and they multiply, so `2d3w` deletes six words.

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

An operator also takes a text object: `iw` `aw` a word, `iW` `aW` a word up to
spaces, `i(` `a(` (or `ib`) brackets, `i[` `a[`, `i{` `a{` (or `iB`), `i"` `a"`
`i'` `a'` ``i` `` ``a` `` quotes on the line, and `ip` `ap` a paragraph. `i` is
inside, `a` takes the edges too. `ci(` rewrites a subquery, even one written over
several lines.

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

## Completion

As you type, a list offers what could finish the word: the columns of the tables
the statement names (or of the open table, before it names one), the tables, and
the keywords. After `from`, `join`, `into` or `update` it offers only tables, and
after `u.` only the columns of whatever `u` stands for.

Where a database has schemas, it offers them too, and the tables in each:
after `sales.` only the tables in `sales`, and the columns of `sales.orders`, or
of its alias, as for any other table. A table outside your default schema is
offered as `schema.table`.

`tab` or `↵` takes the highlighted one, `↑` `↓` or `ctrl+p` `ctrl+n` move, and
`esc` closes the list. In vim, a second `esc` leaves insert mode. With no list
open, `↵` adds a line and `tab` indents as before. `sql_complete = false` turns
it off.

Keywords are written in the case you type them, `sel` giving `select` and `SEL`
giving `SELECT`. `sql_keyword_case = "upper"` or `"lower"` writes them that way
whatever you type.

## Query history

`H` from the grid, or `alt+h` from the editor, opens the history: every statement
you have run from the editor on this connection, and every one your agent ran
with `tql query` or over MCP, newest first and once each. Type to narrow it; `↵`
puts the statement back in the editor, ready to change or run. What tql runs by
itself (paging the grid, the updates behind `:w`) is not in it.

From the shell there is `tql history [connection]`.

## Learning the SQL

The SQL pane that mirrors the grid is how you learn the query behind what you are
looking at: sorting shows the `order by`, filtering shows the `where`, and
following a link shows the `join`. See
[Seeing the query behind the view](Browsing-Data#seeing-the-query-behind-the-view).

To write a query from a description instead, see [Asking for SQL](Asking-for-SQL).
