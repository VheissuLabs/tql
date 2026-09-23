# Configuration

Every setting, what it does, and what it is if you never touch it.

There is nothing to configure to start. On first run tql writes
`~/.config/tql/config.toml` with all of this in it, in this order, one short
line of comment above each key — the long version stays here rather than in
your file. When a later version adds a setting it is
written into the place it belongs on the next run — your values and your own
comments are left alone — and the status line names what arrived.

Defaults live in `config/tql.php` and your file is merged over them, so a
setting you never touch follows the application instead of freezing at the value
it had the day you installed it. A file that cannot be parsed does not stop tql:
it starts on the defaults and says so in the status line.

## Keeping it yours

The shape of the file is tql's: which sections exist, and the order the settings
come in. What is in it is yours — the values, which settings you keep, and any
comment you write.

```bash
tql config           # where it is, and whether it is in order
tql config --tidy    # put it back in order, keeping all of that
```

`--tidy` reorders and nothing else. Values, your own comments, sections tql has
never heard of: all carried across, and a comment written above a setting moves
with it. It writes a `.bak` beside the file first.

**A setting you delete stays deleted.** The first line records the version the
file was written for, and only settings that shipped after that are ever added
back. Deleting something to take the default is a decision, not an accident.

**Every key belongs to a section.** A key written above the first `[section]`
belongs to no section, is never read, and is a genuinely confusing afternoon —
tql notices and tells you which ones.

## `[ui]`

| Key | Default | What it does |
| --- | --- | --- |
| `sql_position` | `"top"` | where the SQL editor sits: `"top"` or `"bottom"` |
| `sql_always` | `false` | keep it on screen instead of only after `s` |
| `sql_height` | `0` | rows it takes; 0 picks a third of the frame |
| `row_style` | `"marker"` | how the current row is shown: `"marker"`, `"dim-others"`, `"bold"`, `"inverse"`, `"underline"` |
| `top_margin` | `1` | blank rows above the frame |
| `sidebar_width` | `24` | width of the table list |
| `time_zone` | `"UTC"` | the zone `ctrl+t` and `now()` write in |
| `modal_ring` | `true` | ring a modal with a border as well as the box itself |
| `inspect_related` | `10` | related rows to load into the inspector; 0 turns it off |
| `export_path` | `""` | where exports go; empty uses the last folder you saved one in |
| `mouse` | `true` | click, drag and scroll inside tql |
| `double_click_ms` | `400` | how close two clicks must be to open the editor |
| `mouse_row_offset` | `0` | subtract this from reported mouse rows |
| `mouse_column_offset` | `0` | subtract this from reported mouse columns |

```toml
[ui]
sql_position = "bottom"
sql_always = true
sql_height = 8
top_margin = 0
mouse_row_offset = 1
```

`mouse_row_offset = 1` is the one to reach for inside a multiplexer whose tab
bar sits above the pane: without it every click lands a row out. The same for
`mouse_column_offset` and a sidebar to the left of the pane.

`row_style` is a matter of terminal and taste. `marker` puts a `▸` beside the
row, `dim-others` fades everything else, and `inverse` is the loud one.

## `[theme]`

Colors are names, not hexes — `dim`, `default`, `black`, `red`, `green`,
`yellow`, `blue`, `magenta`, `cyan`, `white`, `gray` — so tql wears the palette
your terminal is already themed with, and follows it when you change it.

| Key | Default | What it colors |
| --- | --- | --- |
| `border` | `"dim"` | a pane border that is not focused |
| `focus_border` | `"cyan"` | the border of the pane you are in |
| `focus_title` | `"cyan"` | its title |
| `grid` | `"dim"` | column separators, and the rule under the header |
| `cursor` | `"default"` | the block you are on: the selected cell, the caret |
| `selection` | `"default"` | highlighted, but not where you are |
| `edited` | `"yellow"` | a row you have changed, before `:w` |
| `added` | `"green"` | a row you have added, before `:w` |
| `deleted` | `"red"` | a row marked for deletion, before `:w` |
| `modal_border` | `"gray"` | a modal's border, and the ring around it |
| `modal_focus_border` | `"cyan"` | the same, focused |
| `modal_title` | `"white"` | a modal's title |
| `modal_focus_title` | `"cyan"` | the same, focused |

Two special values:

- `grid = "inherit"` ties the grid to the pane border, so a focused table tints
  all the way through rather than growing a colored outline.
- `cursor = "default"` and `selection = "default"` swap the terminal's own
  colors instead of painting one, which is what a terminal cursor has always
  done and what looks right in any theme.

```toml
[theme]
grid = "inherit"
cursor = "cyan"
selection = "cyan"
border = "dim"
```

## `[icons]`

The glyph beside a connection name, by driver. The defaults are Nerd Font
devicons, written here as escapes because they are private-use codepoints that
only a Nerd Font draws; your file can hold the escape or the glyph itself.

| Key | Default | |
| --- | --- | --- |
| `mysql` | `""` | nf-dev-mysql |
| `pgsql` | `""` | nf-dev-postgresql |
| `sqlite` | `""` | nf-dev-sqllite |
| `sqlsrv` | `""` | nf-fa-database |
| `default` | `""` | anything else |

No Nerd Font? Any character works — `mysql = "M"` — or `""` for nothing.

## `[ai]`

What answers when you press `a`. Only table and column names are sent, never
rows.

| Key | Default | What it does |
| --- | --- | --- |
| `provider` | `"auto"` | `auto` takes the first provider you have a key for |
| `model` | `""` | empty picks a sensible default for that provider |
| `timeout` | `60` | seconds to wait for an answer |
| `url` | `""` | an OpenAI-compatible endpoint to use instead |
| `key` | `""` | bearer token for that endpoint, if it wants one |

`provider` takes `anthropic`, `openai`, `gemini`, `groq`, `mistral`,
`deepseek`, `xai`, `openrouter` or `ollama`, and each reads its own environment
variable — `ANTHROPIC_API_KEY`, `OPENAI_API_KEY` and so on. With `auto`,
whichever key you already have exported is the one that answers.

A `url` beats all of that: it points somewhere deliberate, so it wins over any
key in the environment. That is how you use a local model — see
[A local model, with LM Studio](../README.md#a-local-model-with-lm-studio).

```toml
[ai]
url = "http://localhost:1234/v1"
model = "qwen2.5-coder-7b-instruct"
```

## `[keys]`

What key asks for what, by action name:

```toml
[keys]
filter_rows = "F"
new_row = "ctrl+n"
```

The list of actions, what a key may be written as, and which keys are fixed:
[keys.md](keys.md#rebinding).

## What is not configurable

**Tags.** `production`, `staging`, `dev` and `local`, red, yellow, blue and
green. The point of a tag is that production looks the same in your terminal and
in the next person's screenshot, so it is a fixed set rather than a list you can
add to.

**Movement.** `j k h l`, the arrows, `↵`, `esc`, `tab`, `:` and `ctrl+l` are
what a terminal and a vim user both already assume. Everything else is in
`[keys]`.

## Where things live

| | |
| --- | --- |
| `~/.config/tql/config.toml` | this file |
| `~/.config/tql/tql.sqlite` | saved connections and query history |
| `~/.config/tql/key` | the encryption key for stored passwords, `0600` |
| `~/.config/tql/exports/` | where exports go unless you say otherwise |

`XDG_CONFIG_HOME` moves all of it, which is how the tests keep out of your real
connections.
