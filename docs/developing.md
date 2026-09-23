# Developing tql

tql is a Laravel Zero application. The interface is a single Laravel Prompts
prompt that draws a full-screen frame; the MCP server and the CLI commands are
ordinary Laravel commands over the same code. If you can write a Laravel app you
can work on this — the only unusual parts are the drawing rules, and they are
written down below because every one of them was learned by breaking something.

## Getting set up

```bash
git clone https://github.com/VheissuLabs/tql.git
cd tql
composer install
php tql
```

You need PHP 8.4 or newer with `pdo_sqlite`, and a Nerd Font in your terminal if
you want the driver icons to look like icons. A database to poke at:

```bash
curl -L -o chinook.db https://github.com/lerocha/chinook-database/raw/master/ChinookDatabase/DataSources/Chinook_Sqlite.sqlite
php tql chinook.db
```

Running from source uses your real `~/.config/tql`. To work against a throwaway
one, point `XDG_CONFIG_HOME` somewhere else:

```bash
XDG_CONFIG_HOME=/tmp/tql-scratch php tql
```

## Where things live

| | |
| --- | --- |
| `app/Commands/` | the CLI: `browse` (the default), `open`, `export` |
| `app/Tui/Browser.php` | the interface itself: state, keys, and what each one does |
| `app/Tui/Islands/` | the boxes on screen, and `Screen`, `Modal`, `Styler` |
| `app/Prompts/Renderers/` | how a frame is composed from islands |
| `app/Database/` | `QueryRunner`, `ConnectionManager`, filters, the exporter |
| `app/Ssh/` | tunnels and the SSH half of a connection |
| `app/Mcp/` | the server and its four tools |
| `app/Support/` | paths, the config file and its template, key discovery |
| `config/tql.php` | every setting's default |
| `database/migrations/` | tql's own store: connections and query history |

## How a frame is drawn

`Browser` is a `Prompt`. It holds the state — which table, which row, what is
filtered, which modal is open — and `BrowserRenderer` turns that state into a
string, once per key press.

The renderer never draws characters at coordinates. It builds **islands**:
independent bordered boxes that each own a rectangle, render their own content
to a width and a height, and answer hit tests for it. `Screen` places them and
composes the frame row by row, overlaying modals on top.

This matters for correctness, not just tidiness. Screen geometry lives in one
place — the island — so a click asks the island that drew those columns rather
than re-deriving where they were. Every mouse bug in this project came from
having two copies of that arithmetic.

A modal is a `Modal`: one or more islands, centred in the frame, on an opaque
backdrop with an optional ring around it. Adding a pane means adding an island;
adding a modal means adding two lines to the renderer.

`Theme` answers what colour a thing is, from the config. `Layout` answers how
big a thing is. `Styler` is the small bundle of closures an island uses to dim,
bold, pad and truncate without knowing about escape codes.

## The drawing rules

These are the ones that bite. Each has cost at least one evening.

**Build a highlighted row plain.** An escape sequence inside an inverse span
ends the highlight, so a row that sets its own colours and is then highlighted
tears in the middle. Decide the colour once, at the outside.

**Never truncate a string that already holds escapes.** Cutting by character
count slices a sequence in half and the rest of the line inherits it. Fit the
plain text first, then colour what survived.

**Every line must be within the terminal width.** A line one column too long
wraps, the terminal gains a row the frame does not know about, and the whole app
walks. Clamp the status line and the hotkey bar to the width like everything
else.

**A repaint must not home the cursor.** The frame does not always start on the
screen's first row — the alt screen keeps the cursor row it was handed, and
inside a multiplexer pane the row above belongs to something else. To force a
full redraw, hand Prompts a blank previous frame as tall as the taller of the
two frames and let its own relative erase do the work. Writing `\e[H` walks the
app to the top of the screen; a one-line previous frame makes Prompts write
`\e[0A`, which terminals read as "up one", which walks it a row at a time.

**Repaint inside the synchronized-output block.** `\e[?2026h` … `\e[?2026l`
makes the terminal present the erase and the redraw together. Clearing outside
it shows a blank screen for a frame, which reads as a flash. `NO_SYNC_OUTPUT=1`
turns it off for terminals that mishandle it.

**A paste is one key event.** `Terminal::read()` is a single `fread` of 1024
bytes, so pasted text arrives as one multi-character "key". Anything that
switches on `$key` needs to cope with more than one character.

## Adding things

**A key.** `Browser::onKey()` dispatches by mode; the browse map is a `match`
near the top. Add the key, add a line to `HelpIsland`, and add it to the key
table in the README. If the key is a letter that the filter form also uses, add
it to `FILTER_KEYS` so it does not get typed into a value.

**A pane or a modal.** Add an island under `app/Tui/Islands/`, implementing
`content(int $innerWidth, int $innerHeight): array`. Place it in
`BrowserRenderer::__invoke()` — `$screen->add()` for a pane, `$this->modal(...)`
for a modal. Anything that changes the shape of the screen must also appear in
`Browser::shape()`, or the frame will not repaint when it opens.

**A CLI command.** An ordinary Laravel Zero command in `app/Commands/`. Ask for
what was left out with Laravel Prompts when the input is interactive, and fail
with a clear message when it is not — `ExportCommand` is the worked example.

**An MCP tool.** A class in `app/Mcp/Tools/` registered on `TqlServer`. Queries
through MCP are read-only and that is enforced in one place; keep it there.

**A config setting.** Three places: the default in `config/tql.php`, an entry in
`app/Support/ConfigTemplate.php` (which is what writes and tops up the user's
file, with its comment), and a row in the README table. Read it through `Layout`
or `Theme` rather than calling `config()` from a renderer.

## Tests

```bash
./vendor/bin/pest
./vendor/bin/pest --filter=filter
./vendor/bin/pint
```

Tests use `tests/.scratch` as their config directory and never touch your real
connections. `tests/Pest.php` sets `NO_ALT_SCREEN`, `NO_MOUSE` and
`NO_TTY_SETUP` so a prompt can be driven without a terminal.

The interface is tested headlessly, and it is more pleasant than it sounds:

```php
$browser = new Browser($connection, app(QueryRunner::class), app(RowFormatter::class));

$browser->emit('key', 'f');          // press a key
$browser->emit('key', "\e");         // escape

$render = new ReflectionMethod($browser, 'renderTheme');
$render->setAccessible(true);

$frame = preg_replace('/\e\[[0-9;]*m/', '', $render->invoke($browser));
```

Assert on the frame as text: what is in it, how wide the lines are, what sits
above a box. `tests/Feature/AlignTest.php` is the place for anything about
geometry.

Two things worth knowing:

- `Tests\TestCase` binds the environment to `testing`, which is what makes
  Laravel install the prompt fallbacks. Without it every `select()` in a command
  throws `NonInteractiveValidationException` instead of asking.
- Faked key presses drive a real prompt, but a `search()` prompt will hang
  waiting for reads if you queue too few. Prefer `expectsQuestion` for commands.

When something about cursor position is in doubt, write a throwaway terminal
emulator: process the output stream for `\n`, `\e[nA`, `\e[nG` and `\e[J`,
track the row, and assert where the frame lands. That is how the modal jump was
found.

## Building and releasing

```bash
php -d phar.readonly=0 tql app:build tql --build-version=dev
./builds/tql --version
```

`box.json` decides what goes into the phar. It has to include `database/`, or a
released binary cannot migrate its own store on first run.

Pushing a `v*` tag builds the phar, smoke-tests it, packages a `.deb` and an
`.rpm`, and publishes the release. See [packaging.md](packaging.md).

## Notes on the stack

Laravel Zero strips `illuminate/encryption`, so it is required explicitly.

Laravel MCP's service provider registers web routes when `routes/ai.php` exists,
which needs a `router` binding Laravel Zero does not have. The server is
therefore registered from `AppServiceProvider::boot()` with `Mcp::local()` and
no routes file.

Laravel Zero's `commands.remove` cannot remove Laravel's own commands: the
kernel applies it before the framework's providers register theirs, so they come
back. They are hidden instead.

Three gaps in `joetannenbaum/chewie` 0.1.11:

- `RegistersRenderers` resolves `Chewie\Theme::$namespace`, but `src/Theme.php`
  is not in the release, so calling `registerRenderer()` with no argument fatals.
  Pass the renderer class explicitly.
- `Input\Mouse` is three constants with no implementation.
- There is no mouse sequence parsing, so `App\Tui\Mouse` does it.

`Prompt::handleKeyPress()` is private, so it cannot be overridden. Keys are
handled by registering `$this->on('key', ...)`, and a prompt exits by setting
`$this->state = 'submit'`.

## Storage

Connection passwords and SSH passwords are encrypted with Laravel's encrypter
using a key at `~/.config/tql/key`, `0600`. The key sits beside the database, so
this protects against casual reading of the file, not against someone with
access to your account.

Filter values are always bound, never interpolated. The SQL pane shows them
filled in for reading; that string is never what runs.
