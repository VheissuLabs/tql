# Where tql-rust does not copy tql 0.11.0

tql-rust matches tql 0.11.0 screen by screen (see `tests/parity.rs`). Where a frame
differs on purpose, `DELIBERATE`, `LOOKS_DELIBERATE` and `CONFIG_DELIBERATE` in that
file say why. These differences do not show in any frame, so they are listed here.
Each is a place where tql has a bug or a sharp edge that tql-rust leaves out.

- **The SQL editor on a read-only connection runs only reads.** tql runs writes there
  too. `tests/sql_editor.rs` a_read_only_connection_refuses_to_write
- **`d` on a read-only connection marks nothing.** tql lets `:w` delete the row.
  `tests/editing.rs` a_read_only_connection_refuses_every_write
- **A `:w` where one write fails keeps only what was not written.** tql keeps everything,
  so the next `:w` adds the rows that did go in a second time.
- **Quitting with only unwritten new rows asks again.** tql quits and drops them.
- **Unwritten changes stay with their table.** Filtering the table list onto another
  table, switching database or following a link drops them; tql keeps them and would
  write them into the new table.
- **Pasted text undoes in one step of its own**, not with the word typed before it.
- **Clearing a sort on SQL Server keeps `order by (select null)`**, which `offset` needs;
  tql leaves T-SQL that does not run.
- **A SQLite foreign key that names only a table points at its primary key**, so links
  show the column and follow it; tql shows `artists.` and filters on nothing.
  `tests/links.rs`
- **Only the current schema's tables are listed on Postgres and SQL Server.** tql lists
  every schema's, by name alone, and those from another schema do not open.
- **Exports re-import on every driver.** Postgres values keep their types, SQLite blobs
  are written as `X'..'`, and infinity is written as a number SQL can read.
  `tests/export.rs`
- **`~` works in `ui.export_path` and in TLS file paths.**
- **The suggested next key never collides**: whole numbers are read exactly, however
  large.
- **An SSH tunnel is only called up while ssh is still running**, so a port another
  program holds is not mistaken for it.
- **The connection list stays on screen while a connection opens**, saying
  `connecting to {name}…`, and the browser takes its place. tql leaves the screen blank
  in between.
- **`[ui] bottom_margin`**, 0 by default, sets how many blank rows sit under the frame. tql
  always leaves one. The frame comparisons run tql-rust with `bottom_margin = 1`.
- **The frame reaches the last column**, and `[ui] left_margin` and `right_margin`, 0 by
  default, set blank columns beside everything tql draws. tql's panes stop one column short
  of the right edge; the frame comparisons give tql-rust the same one-column gap.
- **`e` on a group header renames the group** for every connection in it; a name already
  in use merges the two. tql has no way to rename a group.
- **A connection that fails from the connection list comes back as a notice on the list**
  rather than outside the browser with a pause.
- **Errors give the database's own words**, without PDO's `SQLSTATE[...]` prefix.

## The agent commands and the MCP server

`tests/agent.rs` and `tests/mcp.rs` hold them to tql 0.11.0's answers. Where they differ:

- **Errors give the database's own words** here too, without PDO's prefix or Laravel's
  `(Connection: …, SQL: …)` suffix.
- **`query --table` shows a Postgres boolean as `true` or `false`.** tql shows `1`, or
  nothing for false. The JSON is the same in both: `true` and `false`.
- **`mcp:start` with a name other than `tql`** says tql's one server is named `tql`, not
  to register it with Laravel's `Mcp::local()`.
- **There is a fifth tool, `write-sql-tool`.** It puts SQL in the SQL editor of the tql
  the user has open on the connection it names, in place of what is there; one undo
  brings back what was. Nothing runs. When several tqls are open on that connection, the
  one used last gets it. The server's instructions add a sentence about it.

## The command line

- **`tql list` and `--help` are laid out by clap**, not Symfony: the same commands, options
  and descriptions, in clap's usual order and wording for its own flags.
- **A `~/.config/dotsql` folder from before tql was named tql is left alone.** tql moved it
  to `~/.config/tql` on start.
- **A `config.php` beside the store is not read.** tql read it only when there was no
  `config.toml`, and every tql release already wrote `config.toml`.

## tableplus

`tests/tableplus.rs` follows tql 0.11.0's TablePlus tests, and a dry run and an import of
the same folder give the same lines, JSON and stored connections. Where it differs:

- **A `--from` folder with no `Connections.plist`** says so and asks for the TablePlus
  Data folder. tql shows PHP's `DOMDocument::loadXML(): Argument #1 ($source) must not be
  empty`.
- **Property lists are read without `plutil`**, so binary ones work on any system, and a
  file that is neither XML nor binary is read as an old-style (OpenStep) property list,
  as `plutil` reads it.
- **The Keychain is asked without tql's 120 second limit**; macOS's own prompt decides
  how long it waits.

## dump, load and sync

`tests/dump.rs` and `tests/dump_engines.rs` hold them to tql 0.11.0's tests, and dry runs,
real dumps, loads and syncs on MySQL and Postgres print what tql prints. Where they differ:

- **Errors give the database's own words** when a connection fails or a replayed
  statement fails, without PDO's `SQLSTATE[...]` prefix.
- **`--to=/` or `--via=/` is refused as a folder with something in it.** tql trims the
  slash away and fails with a PHP error.
- **`-q` still shows errors.** Symfony's `-q` hides them along with everything else; its
  own help says only errors are displayed, which is what tql-rust does. `--silent` hides
  everything.
- **`tql.json` writes names as they are.** tql escapes every character outside ASCII as
  `\uXXXX`; both read either.

## Ask in english (`a`)

`tests/ask.rs` holds it to tql 0.11.0's `AskTest.php`, and to the exact requests tql's AI
SDK sent to each provider (`tests/fixtures/ai-wire.json`). The `ask-*` frames match tql.
Where it differs:

- **Asking does not freeze tql.** The question goes out in the background; the box says
  `asking…    esc cancels`, and esc closes it and throws the answer away. tql blocked
  until the model answered or timed out, with no way out.
- **With nothing configured, tql says what to set.** tql reached for Ollama on
  `localhost:11434` without being told to, so the "no model to ask" message never showed
  and an `OPENAI_API_KEY` lost to an Ollama that was not running. Ollama answers only when
  `OLLAMA_URL` is set.
- **`auto` only picks providers that write SQL.** tql could pick Cohere, Jina, Voyage AI
  or ElevenLabs when theirs was the only key.
- **`TQL_AI_PROVIDER`, `TQL_AI_MODEL`, `TQL_AI_TIMEOUT`, `TQL_AI_URL` and `TQL_AI_KEY`
  work, and win over config.toml.** In tql they never did, because config.toml always
  carries every `[ai]` key.
- **Every provider's address can be moved with `{PROVIDER}_URL`** (`GROQ_URL`,
  `MISTRAL_URL`, `DEEPSEEK_URL`, `XAI_URL`, `OPENROUTER_URL` as well as tql's
  `ANTHROPIC_URL`, `OPENAI_URL`, `GEMINI_URL`, `OLLAMA_URL`), and `openai-compatible`
  needs `OPENAI_COMPATIBLE_URL`.
- **A failure shows on one line.** tql put the HTTP body's line breaks into the status
  line.
- **Only the comment the model signed off with is dropped**: a `/* … */` inside the query
  stays. tql cut from the first `/*` when the query had one of its own.
- **The answer replaces the SQL editor's contents as one edit**, so one undo brings back
  what was there.
- **The cursor in the question stays where it is when a long word is cut across lines.**
  tql drew it two places early.

## SQL Server

- **No driver to install.** tql-rust speaks to SQL Server itself, so Microsoft's ODBC
  driver and OpenSSL 3 are not needed, on macOS or Linux.
- **TLS 1.2 or newer only.** A SQL Server that has never had its TLS 1.2 update (2014 and
  older, unpatched) does not connect. tql reached those through Microsoft's driver.
