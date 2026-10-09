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
