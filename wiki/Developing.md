# Developing tql

tql is a Rust program. The interface is drawn with [ratatui](https://ratatui.rs)
over crossterm; the CLI commands and the MCP server are thin layers over the same
database code the interface uses. The only unusual parts are the drawing rules,
and they are written down below because every one of them was learned by breaking
something.

## Getting set up

```bash
git clone https://github.com/VheissuLabs/tql.git
cd tql
cargo run
```

You need a recent stable Rust (edition 2024), and a Nerd Font in your terminal if
you want the driver icons to look like icons. Nothing else: the database drivers,
SQLite included, are compiled in. A database to poke at:

```bash
curl -L -o chinook.db https://github.com/lerocha/chinook-database/raw/master/ChinookDatabase/DataSources/Chinook_Sqlite.sqlite
cargo run -- chinook.db
```

Running from source uses your real `~/.config/tql`. To work against a throwaway
one, point `XDG_CONFIG_HOME` somewhere else:

```bash
XDG_CONFIG_HOME=/tmp/tql-scratch cargo run
```

The dev profile is optimised, so a debug build feels like the real thing.

## Where things live

| | |
| --- | --- |
| `main.rs` | starts tql: the staged update, the argv rewrite, the console |
| `app/console/` | the command layer: signatures, flags, prompts, the progress bar |
| `app/commands/` | one file per command: `browse`, `open`, `export`, `dump`, `load`, `sync`, `tableplus`, the agent commands, `mcp:start`, `update` |
| `app/tui/` | the interface: the connection list (`picker.rs`), forms, lists, the editor |
| `app/tui/browser/` | the browser itself: state, keys and what each one does, split by concern |
| `app/tui/chrome.rs` | islands, modals, the theme, the hotkey bar |
| `app/terminal/` | the event loop, keys, the mouse, margins, and `Fake` for tests |
| `app/database/` | one file per driver behind the `Database` trait, filters, sorting, the exporter, TLS |
| `app/dump/` | the engines behind `dump`, `load` and `sync` |
| `app/ai/` | asking for SQL: providers and their wire formats |
| `app/mcp/` | the MCP server and its tools |
| `app/agent.rs` | what the agent commands and the MCP tools share |
| `app/live.rs` | how an MCP server hands SQL to the tql you have open |
| `app/config_file/` | `config.toml`: the template, topping it up, `--tidy` |
| `app/settings.rs` | settings as tql reads them, `TQL_*` overrides included |
| `app/store.rs`, `app/store_writer.rs` | tql's own store: connections and query history |
| `app/tunnel.rs` | SSH tunnels |
| `app/updates/` | the daily check and the self-updater |
| `parity/` | the comparison with tql 0.11.0, and why it differs where it does |

## How a frame is drawn

The event loop in `app/terminal/mod.rs` reads keys and the mouse, hands them to
the app, and redraws when something changed. An app is anything that implements
`Tui`: `on_key`, `render`, and a few optional hooks (`on_mouse`, `on_paste`,
`idle`, the margins). The connection list and the browser are the two apps.

`render` never draws characters at loose coordinates. It builds **islands**:
bordered boxes that each own a rectangle, draw their own content into it, and
answer hit tests for it. A click asks the island that drew those columns rather
than re-deriving where they were. Every mouse bug in this project came from having
two copies of that arithmetic.

A modal is a `Modal`: one or more islands, centred in the frame, on an opaque
backdrop with an optional ring around it. Adding a pane means adding an island;
adding a modal means building one and drawing it last.

`theme` answers what colour a thing is, from the config. The top and side margins
are applied around the whole app in `app/terminal/margin.rs`, so an app draws as
if it had the window to itself.

`idle` runs between key presses. It is where work that finishes later lands: an
answer from the model, SQL handed over by an agent, a status message fading.

## The drawing rules

These are the ones that bite.

**Positions are 1-based in the browser's layout.** Islands are placed from column
1 and row 1, the way the frame was designed. Size a pane from `area.width + 1`,
not `area.width`, or the last column stays empty.

**Every line must fit the width.** ratatui clips, so nothing wraps, but a status or
hotkey line that was cut by characters rather than display width loses its end.
Truncate with `text::truncate`, which counts what the terminal will show.

**A paste is one event.** It arrives as `on_paste` with the whole text, never as a
run of keys, so the vim editor in normal mode does not run it as commands.

**Long work never blocks the loop.** A network call or anything else slow runs on a
thread and reports back through `idle`, so the frame keeps drawing and `esc` still
works.

## Adding things

**A key.** Actions and their default keys are in `app/tui/keymap.rs`; give the
action a name there and handle it in `app/tui/browser/keys.rs`. Help, the hotkey
bar, the palette and `[keys]` rebinding all read the keymap, so there is nothing
else to update but [Keys](Keys) and the README's key table.

**A pane or a modal.** Build an `Island` with its content and draw it from the
browser's `render.rs`; for a modal, wrap it in a `Modal`. Keys go to it while it is
open by checking for it at the top of `on_key`.

**A command.** A file in `app/commands/` implementing `Command`, registered in
`app/mod.rs`. Its signature is written Laravel style, so
`"tables {connection : The connection} {--json : JSON}"` declares an argument and a
flag with their help. `cargo run -- make:command Name` writes the skeleton (debug
builds only). Commands an agent runs answer in JSON when piped and as a table in a
terminal; `app/commands/answers.rs` does that for them.

**An MCP tool.** A variant of `Tool` in `app/mcp/tools.rs`, with its name,
description and schema. Queries through MCP are read-only and that is enforced in
`app/agent.rs`; keep it there.

**A config setting.** Three places: a field and its default in `app/settings.rs`,
with its key in `Settings::set`; an entry in `app/config_file/template.rs`, which
is what writes and tops up the user's file, with its comment and the version it
arrived in; and a row in the README and [Configuration](Configuration). A new entry
in the template bumps `template::VERSION` so existing files are topped up with it.

## Tests

```bash
cargo test
cargo test --test picker        # one file
cargo test group                # anything with "group" in its name
cargo fmt
cargo clippy --all-targets
```

The interface is tested headlessly with `Fake`, which drives an app the way a
terminal would and hands back the screen as text:

```rust
let mut fake = Fake::sized(Browser::new(connection, database).unwrap(), 120, 30);

fake.press("f");            // keys, as the bytes a terminal sends
fake.press("\x1b");         // escape
fake.click(5, 4);           // the mouse
fake.paste("select 1");     // a paste

assert!(fake.screen().contains("ROWS"));
```

Assert on the screen as text, and on the app's state where that is clearer. Tests
keep out of your real connections: settings are installed per test, and anything
that touches the store uses a scratch directory.

Some tests need things a laptop may not have, and are `#[ignore]`d unless you ask
for them with `--ignored`:

| | needs |
| --- | --- |
| `tests/parity.rs` | PHP, and a checkout of tql 0.11.0 with `composer install` at `TQL_PATH` (default `~/Code/tql`); `TQL_PARITY_MYSQL=1` and `TQL_PARITY_PGSQL=1` add MySQL and Postgres |
| `tests/kinds.rs` | a local MySQL and Postgres |
| `tests/live_tls_ssh.rs` | Docker, for throwaway MySQL, Postgres and sshd containers |
| `tests/sqlsrv.rs` | a SQL Server at `TQL_TEST_SQLSRV=host:port`, with `TQL_TEST_SQLSRV_PASSWORD` for `sa` |

**Parity** renders the same scenarios in tql 0.11.0 and in this tql and compares
every frame, text and colour. Where they differ on purpose, `tests/parity.rs` says
why and `parity/deliberate.md` lists what no frame shows.

## Building and releasing

```bash
cargo build --release
./target/release/tql --version
```

A release is one command, from a clean `main` that matches GitHub:

```bash
bin/release 0.12.1
```

It sets the version in `Cargo.toml` (the binary's `--version` comes from there, and
the updater only installs a download that says it is the version it was published
as), runs the tests, commits and pushes the bump, drafts the tag message from the
`feat:` and `fix:` commits since the last tag, opens it in your editor, and pushes
the tag. See [Packaging and releasing](Packaging-and-Releasing) for what the tag
triggers.

## Storage

Connection passwords and SSH passwords are encrypted with AES-256-CBC using a key
at `~/.config/tql/key`, `0600`, in the format tql has always written, so a store
from any earlier version opens. The key sits beside the database, so this protects
against casual reading of the file, not against someone with access to your
account.

Filter values are always bound, never interpolated. The SQL pane shows them filled
in for reading; that string is never what runs.
