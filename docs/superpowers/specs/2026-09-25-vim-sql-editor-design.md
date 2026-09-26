# Vim mode for the SQL editor

Date: 2026-09-25
Status: built

## Goal

`sql_editor = "vim"` today covers `h j k l 0 $ g G x dd i a I A o O` and `↵`.
Grow it into an editor a vim user can write SQL in without thinking: motions
with counts, operators on any motion or text object, yank and paste, undo and
redo, `.`, and visual mode.

`sql_editor = "simple"` does not change.

## Decisions already made

- ctrl+r is **redo** in vim style. It still runs the query in simple style.
- The query runs with `:r` or `:run` while the SQL editor has focus. Anywhere
  else `:r` keeps meaning reload. The command line says which one `:r` will do.
- With a visual selection, `:r` runs only the selected SQL.
- One unnamed register. Yanks also go to the system clipboard through
  `Clipboard::copy`. `p` and `P` read the register, never the clipboard.
- A whole insert session is one undo step, as in vim.

## Out of scope

Named registers, macros, marks, search, undo in simple style, and vim keys
in the cell editor.

## Keys

The pending-key grammar is `[count] operator [count] (motion | text object)`,
or `[count] motion`, or a shortcut. Counts multiply: `2d3w` deletes six words.

### Motions

| Keys | Moves to |
| --- | --- |
| `h` `l` | a character left or right, not past the line |
| `j` `k` | a line down or up, keeping the column where it fits |
| `0` `^` `$` | line start, first non-blank, last character |
| `gg` `G` | first line, last line; `5G` and `5gg` go to line 5 |
| `w` `b` `e` | next word start, previous word start, word end |
| `W` `B` `E` | the same over WORDs, which break on whitespace only |
| `f` `t` `F` `T` + char | onto or before a character in the line, forward or back |
| `;` `,` | repeat the last `f t F T`, same or opposite direction |
| `%` | the bracket matching the one under or after the cursor, `()[]{}` |
| `{` `}` | the blank line before or after this paragraph |
| `↵` | the first non-blank of the next line |

A word is a run of `[A-Za-z0-9_]` or a run of other non-blank characters, as
in vim's default `iskeyword`.

Line-wise motions: `j k gg G ↵`. Everything else, `{ }` included, is
character-wise, as in vim.
Inclusive motions (the character at the target is included under an
operator): `e E f t F T % $`. The rest are exclusive. `cw` behaves as `ce`,
as in vim.

### Text objects

Only after an operator, or in visual mode.

| Keys | Range |
| --- | --- |
| `iw` `aw` | the word, and with trailing (or leading) space |
| `iW` `aW` | the same over a WORD |
| `i(` `a(` `ib` `ab` | inside or around the enclosing `( )` |
| `i[` `a[` `i{` `a{` `iB` `aB` | the same for `[ ]` and `{ }` |
| `i"` `a"` `i'` `a'` ``i` `` ``a` `` | inside or around quotes on this line |
| `ip` `ap` | the paragraph, and with the blank lines after it |

`i(` and the other bracket objects work across lines, because SQL
subqueries do.

### Operators

| Keys | Does |
| --- | --- |
| `d` | delete into the register |
| `c` | delete into the register and enter insert |
| `y` | copy into the register and the clipboard |
| `>` `<` | indent or outdent each line two spaces |
| `dd` `cc` `yy` `>>` `<<` | the same on the whole line, count lines |

### Shortcuts

| Keys | Same as |
| --- | --- |
| `x` `X` | `dl` `dh` |
| `D` `C` `Y` | `d$` `c$` `yy` |
| `s` `S` | `cl` `cc` |
| `r` + char | replace the character under the cursor, count times |
| `J` | join this line and the next with one space |
| `~` | swap the case of the character and move right |
| `p` `P` | put after or before the cursor; a line-wise register puts below or above |
| `u` | undo |
| ctrl+r | redo |
| `.` | repeat the last change, with a new count if given |
| `i a I A o O` | enter insert, as today |

### Visual mode

`v` starts a character selection and `V` a line selection at the cursor.
Motions and counts move the free end. `o` swaps the ends. `v`/`V` switch
between the two kinds, or leave when pressed again. `esc` leaves.

On the selection: `d x y c > < ~ J`. `:r` runs only the selected text. After
any of these the editor is back in normal mode.

### Insert mode

As today (`↵` new line, tab indent, shift+tab outdent, esc to normal), plus
ctrl+w deletes the word before the cursor and ctrl+u deletes to the start of
the line. Arrows move without leaving insert, and start a new undo step.

`esc` from insert moves the cursor one left, as vim does, so it sits on the
last character typed.

### Normal-mode cursor

In normal and visual mode the cursor sits *on* a character and never past
the last one. An empty line is the one exception.

## Structure

```
app/Tui/Vim/Vim.php          state: mode, pending keys, counts, register,
                             last change, last f/t, visual anchor; one entry
                             point, press(QueryEditor, key), which answers
                             with one of its constants: NOTHING, LEAVE,
                             NEXT_PANE, PREVIOUS_PANE, COMMAND
app/Tui/Vim/Motions.php      pure: (text, offset, count, …) → offset
app/Tui/Vim/TextObjects.php  pure: (text, offset, inner?) → ?Range
app/Tui/Vim/Operators.php    applies an operator to a Range on a QueryEditor
app/Tui/Vim/Range.php        start, end, line-wise?
app/Tui/Vim/Text.php         the buffer as characters, with line lookups
```

`VimKeys.php` is removed. Browser keeps `sqlMode` as a read of `Vim`'s mode
so the renderer and tests stay as they are.

`QueryEditor` gains:

- `text(Range)` and `replace(Range, string)`, which every operator uses
- an undo history of `[buffer, cursor]` snapshots with `checkpoint()`,
  `undo()` and `redo()`; a new change after an undo drops the redo half
- `moveTo(int)`

`Vim::selection()` gives the renderer the range to paint. `EditorIsland`
paints the selection in `Theme::selection()` and the title
reads `SQL · VISUAL` or `SQL · VISUAL LINE`.

Browser's `runCommand` sends `r`, `run` to `runQueryBuffer()` when the SQL
editor has focus, with the selection when there is one, and `reload()`
otherwise. The command line prompt reads `:r runs the query` or
`:r reloads` as you type it. The vim footer shows `:r Run` instead of
`ctrl+r Run`.

## Testing

- `tests/Unit/Vim/MotionsTest.php` and `TextObjectsTest.php`: text and an
  offset in, an offset or a range out, table-driven, including the edges
  (start and end of buffer, empty lines, unbalanced brackets, a quote with
  no partner).
- `tests/Feature/SqlEditorTest.php` grows a test per operator, shortcut,
  visual action, undo/redo, `.`, and `:r` in and out of the editor, all
  driven through `Browser::emit`.
- The whole suite stays green, simple style included.

## Docs

In `docs/keys.md`, the tables above replace the short vim table, and the
command table notes that `:r` runs the query from the SQL editor. The README
line on `sql_editor`, the `sql_editor` comment in `ConfigTemplate`, and
`docs/configuration.md` all stop saying "ctrl+r runs either way" and name
`:r` for vim.
