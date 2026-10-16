# Changelog

What changed in each release, newest first, grouped into New Features,
Enhancements and Bug Fixes. Newer tagged releases also have a fuller
announcement under `release-notes/` for GitHub.

## Unreleased

### New Features

- Snowflake connections sign in with a key pair: the user's login name and
  the path to their RSA private key, PKCS#8 (encrypted or not) or PKCS#1, with
  an encrypted key's passphrase saved as the password. tql signs a fresh JWT
  before the last one runs out. `authenticator=snowflake_jwt&private_key_path=…`
  in a connection string.
- Snowflake connections sign in through the browser with SSO
  (`authenticator=externalbrowser`): tql opens your identity provider and
  takes the session it hands back. The session is kept in the store,
  encrypted, and renewed without the browser while Snowflake allows, so
  `tql query`, the MCP server and the next launch reuse it. Large answers that
  Snowflake keeps in cloud storage are gathered in parallel.
- The connection form asks a Snowflake connection how to sign in, and offers
  the PEM private keys in `~/.snowflake`, `~/.ssh` and the current directory.
- A copied Snowflake connection string keeps its warehouse, role, sign-in and
  private key path.

---

## 0.15.0

### New Features

- Snowflake connections over its SQL API with a programmatic access token:
  browse and query, schemas, declared keys for links and paging, and every
  type shown as Snowflake writes it. Tables, columns and keys come from `SHOW`
  commands, which do not wake the warehouse. Tables are read only in the grid
  ([#8](https://github.com/VheissuLabs/tql/pull/8)).
- The connection form asks a Snowflake connection for its Account, Database,
  Access token, Warehouse and Role; `tql open` and `:open` take
  `snowflake://user:token@account/DATABASE?warehouse=WH&role=ROLE`.

### Bug Fixes

- Opening a connection string tql has already saved keeps the password it
  carries, and a Snowflake warehouse and role, rather than the ones saved
  before ([#10](https://github.com/VheissuLabs/tql/pull/10)).
- A connection with no port, libSQL or Snowflake, no longer shows a stray
  colon after its host.

---

## 0.14.2

### New Features

- `tql connect <connection>` opens a saved connection by name, skipping the
  list; `--db=<database>` opens another database on the same server for that
  session. A path or connection string still goes to `tql open`
  ([#7](https://github.com/VheissuLabs/tql/pull/7)).

### Bug Fixes

- `tql connect` with a database file that is not there yet says No such file.

---

## 0.14.1

### Bug Fixes

- SQL Server: a read-only connection and the MCP `run-query` tool refuse
  `use`, `commit`, `kill`, `dbcc` and other T-SQL carried after a select.
- SQL Server: queries slower than 30 seconds finish, and a dropped connection
  reconnects.
- SQL Server: exports load back, leaving out computed and rowversion columns,
  keeping identity values and writing huge floats with an exponent.
- SQL Server: column names complete in the SQL editor; hex typed into a
  varbinary column is written as bytes; contains, starts with and ends with
  match datetime and money as the grid shows them; filters keep `N''` on text;
  the editor keeps `[a;b]` names, splits on `GO` and keeps procedure bodies
  whole; types show their declared sizes.
- ClickHouse: tables page in a stable order in the grid and `tql export`.
- ClickHouse: read-only users connect, answers over 10 MB come through, and
  an error partway through a query keeps ClickHouse's message.
- ClickHouse: `ssl_ca`, client certificates and the TLS modes are honoured;
  temporary tables and `SET` last between statements; written rows are
  reported; IPv6 hosts connect; `tql load` replays statement by statement.
- Redis/Valkey: `:w` writes nothing if a key is not the type the changes
  expect, and drops changes it may have half-written instead of writing them
  twice.
- Redis/Valkey: `N` refuses a key that already exists.
- Redis/Valkey: a silent server times out, and a dropped connection
  reconnects.
- Redis/Valkey: `ssl_ca`, client certificates and the TLS modes are honoured.
- Redis/Valkey: binary key names stay apart; ACL users see real types; a
  rename with changes pending is refused; very large strings show in part;
  read-only connections run `INFO` and `CONFIG GET` but not scripts; `T` says
  when the key does not exist; streams keep repeated field names.

### Changes

- ClickHouse arrays and maps show in ClickHouse's own syntax, not JSON.

---

## 0.14.0

### New Features

- Postgres and SQL Server list the tables of every schema under folding
  headers, remembered per connection and database, and name a table outside
  your default schema `schema.table` everywhere: the grid, the palette, links,
  exports, `tql tables`, `tql describe`, `tql export` and the MCP tools.
- Completion offers schemas and the tables in them, narrows to a schema after
  `schema.`, and completes the columns of `schema.table` and its alias
  ([#5](https://github.com/VheissuLabs/tql/issues/5)).
- DuckDB files open in tql, with their schemas, every DuckDB type shown
  readably, and editing by primary key.
- libSQL and Turso connections, by URL and auth token, over Turso's HTTP API,
  with editing as SQLite has.
- ClickHouse connections over its HTTP interface, browse and query only, with
  every ClickHouse type shown exactly.
- Redis and Valkey connections open a key browser: folding `:` namespaces
  scanned in the background, a view for every type, edits written together on
  `:w`, TTL, rename, database switching and a command pane. On Redis 7.4 and
  Valkey 9 a hash shows each field's TTL and `T` sets it; `:log` shows the
  command log (Valkey) or slow log (Redis).
- `tql open` and `:open` take a DuckDB file, `libsql://` and
  `https://…?authToken=` URLs, `clickhouse://`, `redis://` and `valkey://`
  URLs and their TLS forms; TablePlus import brings ClickHouse and DuckDB
  connections over.

### Enhancements

- The status line shows the cursor's row among all the table's rows, `105
  of 5,000,000`, in place of the column position, and loading a page says
  only how long it took. Filtered tables and small ones are counted exactly;
  a big Postgres, MySQL or SQL Server table shows the database's own
  estimate, `~4.9M`, rather than wait on a full count.
- `sql_keyword_case` (in `[ui]` and the settings screen) sets the case
  completion writes keywords in: `"match"` (how you type, as before), `"upper"`
  or `"lower"`. A config file from an earlier tql is offered it on first run.
- `↵` takes a completion as `tab` does.

### Bug Fixes

- The insert-mode cursor in the SQL editor and the Redis command pane sits
  right after the text, not a row down and a column over.

---

## 0.13.2

### New Features

- `:update` checks for a newer tql and installs it in the background, on either
  screen, with the result in the status bar.
- `:open <path or connection string>` opens a SQLite file or a connection
  string, saving it as `tql open` does and reusing one already saved.
- `:export database` writes every table in the current database to one `.sql`
  file.
- `:v` / `:version` say which version of tql is running.
- `:help` / `:h` open help.

### Enhancements

- The new commands are in the ctrl+k palette, the help screen and
  `docs/keys.md`.

---

## 0.13.1

### New Features

- Show or hide the SQL editor from the ctrl+k palette or with `|`, next to `\`
  for the table list. Hiding it holds even with `sql_always` on, until you show
  it again or quit; rebind it as `toggle_sql`.

### Enhancements

- Editing a setting starts with its value selected, so typing replaces it,
  backspace clears it and ← → keep it to edit in place.
- The palette lists tables, rows, reload and query history once each, as the
  keyed action with its shortcut.

### Bug Fixes

- Editing a setting shows the terminal's own cursor after the value, on a row
  that keeps its highlight, so it shows whatever the theme colours.
- The record inspector lines up wide characters (Chinese, Japanese, emoji) by
  the columns they take.
- A setting's description breaks a word too long for the line.

---

## 0.13.0

### New Features

- A settings screen: `tql settings`, `:settings`, or Settings in the ctrl+k
  palette. Every setting sits under a section that folds (UI, Theme, Updates,
  AI, MCP, Icons), takes effect at once, and is written to `config.toml` straight
  away, that one line, your comments and order kept.
- Settings are edited by kind: enter flips on/off, ← → step through choices,
  numbers are typed and held to their range, `x` restores the default and `/`
  finds one. Changed values are drawn in your `edited` colour, the AI key is
  never shown, and start-only settings such as `mouse` say so.
- A ctrl+k palette on the connection list, with its commands and every
  connection by name.
- `tql new`, `tql sql [connection]` and `tql history [connection]` open tql on
  the screen the command belongs to, asking for a connection when none is named.

### Enhancements

- One command registry behind `:`, both palettes and the shell, so a command has
  one name everywhere, and a command typed on the wrong screen says where it
  works.
- The palette matches a command by its name too, so `settings` runs the command
  ahead of a table called `settings`.

---

## 0.12.2

### Enhancements

- One column of space between the SQL editor’s border and its text, with the
  cursor, clicks and the completion list moved to match.
- The driver is a type now, so a connection saved as `mariadb` is MySQL
  everywhere, including dump, a stored driver tql no longer knows is skipped
  when the connection list is read, and the default ports come from one place
  (MySQL and MariaDB 3306, Postgres 5432, SQL Server 1433).

### Bug Fixes

- Read-only means read-only: a write hidden in a `WITH` is refused by the
  database (SQLite `query_only`, MySQL and Postgres read-only transactions)
  or, on SQL Server, refused for any writing word outside its strings and
  comments and run in a transaction tql always rolls back — for agent
  commands, MCP query, and a read-only connection’s SQL editor.
- Wide characters such as Chinese, Japanese and emoji take two columns in the
  SQL editor, the row form, the filter value, the value viewer, the connection
  form and group rename, so they wrap, scroll, place the cursor and take clicks
  without eating a border or losing their end.
- An export pages through a table in the order of its whole primary key,
  composite keys too, and a failed key lookup stops the export instead of
  paging in no order.
- An edit or delete that reaches no row says so and stays pending instead of
  reporting it was written, MySQL counts matched rows, and sorting a query
  result only touches the statement’s own `order by` and `limit`, never words
  inside strings, subqueries or comments.
- A MariaDB export loads back, Postgres shows the first result when several
  statements return rows, MySQL shows dates and times the same with or without
  a filter, and a whole real stays a real when shown or exported.
- MySQL `BIT` columns show as numbers, and an edited or added bit is written as
  a number.
- `sync` knows a database is itself however its host, port or MySQL flavour is
  written, so two connections that differ only in `127.0.0.1` versus
  `localhost`, or a missing port, are not taken for one another.
- The record inspector measures related rows at full width, so it widens once
  when a relation opens instead of two columns on every redraw.
- The production safeguards for creating a database and sharing the screen read
  the tag whatever its case, and a huge `status_seconds` no longer panics.
- A mouse offset or a column drag can no longer push a position out of range,
  and a nonsense `mouse_row_offset` or `mouse_column_offset` in the config is
  held to ±1000.
- A corrupt stored password, an oversized MCP cursor, a nonsense last-update
  time or an extreme mouse offset no longer panics; ssh reads the host as the
  destination; a DSN takes a bracketed IPv6 host and decodes only real percent
  escapes; and the AI answer is found after braces in its reasoning.
- The SQL editor’s text wrap returns early for a zero width instead of looping.

---

## 0.12.1

### Bug Fixes

- `tql -v` on its own prints the version, as `-V` and `--version` do. After a
  command, `-v` still means verbose.

---

## 0.12.0

tql is rewritten in Rust. It is the same tql: the same screens, keys,
commands, config and connections, checked frame by frame against 0.11. It is
also a third of the size (11 MB on a Mac, down from 35), starts in about 7 ms
instead of 170, and needs nothing else installed.

### Moving over

- Your connections, passwords, settings and history carry over as they are,
  from the same `~/.config/tql`. New settings are added to `config.toml` with
  a notice.
- Installed with the install script or by hand, tql updates itself to 0.12.0
  as usual. Homebrew, `.deb`, `.rpm` and PKGBUILD upgrade the usual way.
- `tql.phar` is gone. If you ran the phar, install one of the binaries
  instead.

### New Features

- SQL Server is built in on macOS and Linux. Microsoft’s ODBC driver and
  OpenSSL are no longer needed. It needs a server with TLS 1.2 or newer.
- Let an agent work alongside you without letting it run wild. The MCP
  server’s write-sql-tool drops a statement into the SQL editor of the tql you
  have open, for you to read, edit and run, never on its own. Turn on
  `[mcp] read_screen` and its read-screen-tool lets an agent see your screen,
  with `read_production` gating production connections and a status-line note
  every time it looks.
- `e` on a group header in the connection list renames the group.
- Creating a database on a connection tagged production asks first
  (`[ui] confirm_create_on_production` turns it off).
- `[ui] bottom_margin`, `left_margin` and `right_margin`, beside `top_margin`.
  All four default to 0, and the frame now reaches the last row and column.

### Enhancements

- Asking for SQL with `a` no longer freezes tql while the model thinks, and
  `esc` cancels it. `TQL_AI_*` variables work, and with nothing configured
  `a` says what to set.
- Opening a connection keeps tql on screen and says it is connecting.

### Bug Fixes

- A MySQL connection through an SSH tunnel stays on TCP, instead of trying a
  socket path from the server on your machine.
- Errors are in the database’s own words, without PDO’s SQLSTATE prefix.
- The SQL editor on a read-only connection runs only statements that read.
- A `:w` where one write fails keeps only what was not written.

Everything that behaves differently from 0.11 on purpose is listed in
`parity/deliberate.md`.

---

## 0.11.0

### New Features

- Copy a whole database, schema and data, from the command line. `tql dump`
  and `tql load` (or `import`) drive mydumper and myloader or `pg_dump` and
  `pg_restore` in parallel where they can, handle SQLite by themselves, keep
  passwords off the command line, and replay a tql export file in one
  transaction.
- `tql sync` makes one database a copy of another, local or remote, in a
  single step, under a new name with `--into`. All three show a progress bar
  from the start, and `-v` prints everything the tools say.
- `tql connect` reads a TablePlus url, `mysql+ssh://` and all, saving its
  tunnel, name and environment as a tag, and keeps two servers apart that both
  sit at 127.0.0.1 behind ssh.
- Leaving TablePlus behind costs you nothing. `tql tableplus` imports your
  saved connections with their groups, environments as tags, SSH and TLS
  settings, and passwords read from the Keychain, macOS asking before handing
  them over. It is safe to run twice, skips what you already have, and answers
  an agent in JSON.

### Bug Fixes

- Say MySQL dumps are not compressed, and why.
- Make the folder above a dump before mydumper runs.
- Give every file that carries a password its own name, so a dump and a load
  prepared together each reach their own server.
- Keep every SSH tunnel open until tql exits, so a load into a server behind
  SSH is not left pointing at one that closed after the dump.

---

## 0.10.0

### New Features

- `tql connect`, the same as `tql open`, since connect is the word you reach
  for with a connection string.
- Group connections under headers that fold and stay folded, yank a
  connection’s database name with `y` or its connection string with `Y`, and
  `d` says Delete on the connection list.

---

## 0.9.0

### New Features

- SQL Server is built into the macOS binary, encrypted the way each SSL mode
  means. Paging uses offset and fetch, insert writes a row of defaults the
  server accepts, and Microsoft’s driver is pointed at OpenSSL 3.
- Give an agent read-only eyes on your databases. `query`, `tables`, `describe`
  and `connections` answer from the shell, in JSON when piped, with the same
  answers the MCP server gives.
- Create and drop databases from the database list or the command line,
  stepping off the one in use to drop it. Every letter types into a list, so
  `j`, `k` and `q` narrow it too.

### Bug Fixes

- Draw a whole frame in its pane’s border color, separators and rules
  included, and drop the grid setting that could split it in two.
- Find the standalone binary by its real path, stage updates beside it and
  switch with a symlink at the next start, so a running tql keeps working.
- Create and drop SQL Server databases from `master`, the only place Azure SQL
  Database takes them.

---

## 0.8.0

### New Features

- Check for a new release once a day, update a script or by-hand install by
  itself after checking its checksum, and say what to run for Homebrew and
  packages.
- Publish SHA256SUMS and a filled-in PKGBUILD with every release, and check
  both when verifying it.

### Bug Fixes

- Run the migrations a newer release brings on a store that already exists,
  not only on the first run.

---

## 0.7.0

### New Features

- Browse the query history with `H` or `alt+h`, narrow it as you type, and put
  a statement back in the SQL editor with enter.
- Complete tables, columns and keywords as you type in the SQL editor, with
  tab to take one, in both the simple and vim editors.
- Show a line cursor in vim insert mode and a block in normal and visual mode.

### Bug Fixes

- Read an escape and the key right behind it as one alt key, so alt keys
  survive a terminal that sends them in two pieces.

---

## 0.6.1

### New Features

- Drop the hotkey bar with `hotkey_bar = false` and give its row to the frame.

### Bug Fixes

- Round every frame’s corners and join its column ticks to it.

---

## 0.6.0

### New Features

- Focus the pane you click, including its border, and leave the SQL editor
  when you click the table list or the rows.
- Bring your vim fingers to your SQL, or don’t. Set `sql_editor = "vim"` and
  the editor has counts, motions, operators, text objects, visual mode, a
  register, undo and `.` repeat, with `:r` to run the statement or the
  selection under the cursor. Keep it simple and enter adds a line, tab indents
  and ctrl+z undoes. Both keep the indentation on a new line and run only the
  statement under the cursor.

---

## 0.5.7

### New Features

- Inspect a related record with `i` from the inspector, and step back with
  escape.

---

## 0.5.6

### Bug Fixes

- Fit the connection screen to any window 60 columns and up, and redraw it
  when the window is resized.

---

## 0.5.5

### Bug Fixes

- Open the inspector at once with related tables collapsed, load one only when
  you open it, and keep `j`, `k` and tab inside the box you are in.

---

## 0.5.4

### Bug Fixes

- Keep one connection open and read the whole schema in one go on MySQL, so a
  remote table opens in a single round trip.

---

## 0.5.3

### Bug Fixes

- Ask for a bigger window below 60 by 12 instead of drawing a layout that
  wraps.

---

## 0.5.2

### Bug Fixes

- Redraw when the terminal is resized, and read its real size instead of the
  one Symfony pinned at startup.

---

## 0.5.1

### Bug Fixes

- Draw only the pane you are in as focused, and switch panes with alt and a
  number from the SQL editor.

---

## 0.5.0

### New Features

- A command palette, pane keys, a table list you can hide and resize, and a
  status line that says what matters.

---

## 0.4.3

### New Features

- Title the table list with the database, and keep every pane title inside its
  border.

---

## 0.4.2

### Bug Fixes

- Scroll a field while it is typed, so a long path shows its end and the
  cursor.
- Save a sqlite path from the form as an absolute one, with `~` expanded and a
  relative path resolved from where tql started.

---

## 0.4.1

### New Features

- Add and edit a whole row in a form, and keep modals inside their borders.
- Help that fits: short descriptions, merged keys, side-by-side columns.

---

## 0.4.0

### New Features

- Install tql with one command, package it as a deb, an rpm, a formula or a
  PKGBUILD, and ship a standalone binary that does not need PHP.
- `tql config --tidy`, and settings you delete stay deleted.
- One keymap, rebindable in the config.
- Add a row with `N`, and put a new row on top of the grid.
- ctrl+t and `now()` fill in a time the way the column takes it, written in
  UTC with the zone named.
- Show an error in a modal, not on the status line.

### Enhancements

- Remember where the last export was saved.
- Ring a modal with its own border, and let it be turned off in the config,
  the row inspector included.
- Move any picker with `j` and `k`, and keep the filter form’s own keys out of
  the value.
- Say what each relation is, read a pivot through to the far side, keep a
  related row readable instead of showing every column, clamp a related column
  rather than dropping it, and lay related columns out to the width there is.
- Light the status line up when it changes.
- Escape puts the table back after a query, and the config file keeps its
  order.
- Keep the cursor visible inside a pending row, and fill a key nothing else
  will.

### Bug Fixes

- Repaint the frame where it sits, not at the top of the screen.
- Stop a modal walking the app up a row.
- `--name` names a connection, `--tag` says what it is.
- Let a local endpoint answer when the named provider has no key, and make the
  ask work at all, including with a local reasoning model.

---

## 0.3.0

### New Features

- Ask which connection and table when export is given neither, ask where to
  save an export, and ask which database when exporting from a server
  connection.
- Name exports after the database, offer the path with a tilde, and drop the
  `all` segment from a whole-database export name.

### Bug Fixes

- Let every table be chosen from the export table list.

---

## 0.2.0

### New Features

- Meet the row inspector, the part of tql you will not find anywhere else.
  Press `i` and the record floats over the grid with two foldable sections:
  its own columns with their types, and every row it relates to, each relation
  a box you open in place rather than a query you have to write. Follow a link
  to the record it points at, follow it back the other way, and press escape to
  retrace your steps. It is the join you would have written, walked with the
  arrow keys.
- `y` yanks the value, `Y` yanks the row as an object.
- `ui.mouse` turns the mouse off for people who would rather select text.
- Enter runs and saves everywhere, shift+enter adds a line.
- Reach a database through an ssh tunnel, and `c` returns to the connections.
- SSL, ssh password, colour, tag, read only, and a picker for key files, with
  the connection form grouped, an Over SSH switch and ssh in one namespace.
- Tags carry their color, chosen from a list.
- Switch databases on a server connection.

### Bug Fixes

- Say what `L` does in the status line instead of cluttering the header.
- Follow a link to a table the sidebar filter is hiding.
- Opaque modal backdrops, repaint on shape change, help keeps the panes, and
  filter defaults.
- Fit filter text before drawing the cursor so nothing cuts an escape sequence.
- The cell cursor belongs to the focused pane, and the tables list keeps its
  selection.
- Four crashes in the connection form and key picker.
- Run the picker highlight the full width of the list.
- Scope the table list to the database in use on MySQL.

---

## 0.1.0

The first release.

### New Features

- A full-screen browser with a sidebar, a ruled grid, a vim command line, a
  connection picker, and mouse clicks and scroll wheel.
- A SQL editor (ctrl+r to run) with syntax highlighting, the query behind the
  current view in its own pane, and a pane title that follows the table a
  query selects from.
- Edit cells inline or in a modal, resize columns by dragging their borders,
  inspect a cell or a whole row as an object with `i`, view table structure
  with `t`, and follow foreign keys with `L`. JSON columns open in a
  highlighted modal with line numbers, with vim line selection and yank.
- Export a table or a whole database to re-importable SQL.
- Create, edit and delete connections in a modal, remember what you open,
  name a connection with `--name` and tag it with `--tag`, group them, and
  open a SQLite file or a connection string straight from the command line.
- Sort by clicking any column header, filter the tables list with `/`, and
  narrow the grid to the rows you want without writing a WHERE. The filter
  form stacks conditions, with a type-to-filter picker for the column and the
  operator, and every value is bound rather than pasted into the query.
- Ask for a query in plain English and get SQL back. Press `a`, describe what
  you want in a modal text area, and the statement is put in the SQL editor for
  you to review and run, with any provider key or a local endpoint.
- An MCP server that exposes the same engine as the browser, so an agent works
  with your tql’s connections, schema and queries instead of a second copy of
  them.
- A user config in TOML over the shipped defaults: top margin, sidebar width,
  selected row style, focus border colour, SQL editor position, and Nerd Font
  devicons per driver.
- Mark rows with `d` and write the deletions with `:w`, and hold edits until
  `:w` with edited rows highlighted in yellow.
- An in-app help pane with worked examples.
