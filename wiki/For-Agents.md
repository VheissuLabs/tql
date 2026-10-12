# For agents

An agent can reach your databases two ways, and both use the same engine, the same
connections and the same rules as you.

## Plain commands

Which any agent that can run a shell already knows how to use:

```bash
tql connections                          # what is saved
tql tables prod                          # its tables; --database= for another one on the server
tql describe prod orders                 # columns, types, primary key
tql query prod "select count(*) from orders where placed_at > now() - interval 1 day"
echo "select 1" | tql query prod -       # or pipe the statement in
```

They never prompt or draw a screen. In a terminal they print a table; piped or
captured, which is how an agent runs them, they print JSON. `--table` and `--json`
choose either way. A refusal or a database error goes to stderr with a non-zero
exit code. A query returns at most 200 rows unless you pass `--limit`, and says
when it stopped short.

A line in a project's `CLAUDE.md` (or `AGENTS.md`) is enough for an agent to find
them:

```markdown
Query the database read-only with `tql query <connection> "<sql>"`; `tql tables` and `tql describe` show the schema.
```

## The MCP server

For agents that prefer one:

```bash
claude mcp add --scope user tql -- tql mcp:start tql
```

Its tools (list connections, list tables, describe a table, run a query) give the
same answers as the commands.

## Read-only

Either way it is **read-only**: only `select`, `show`, `explain`, `describe`,
`pragma` and `with` are accepted, one statement at a time, and the database itself
holds tql to it: SQLite runs the statement with `query_only`, MySQL and Postgres
in a read-only transaction.

SQL Server has no read-only transaction, so tql refuses a statement with a writing
word anywhere outside its strings and comments, such as `update` or `into`, and
runs the rest in a transaction that is always rolled back. A write hidden inside a
`with` is refused.

Writes happen in the interface, not through an agent. Everything an agent runs
lands in the same history as your own statements, marked `cli` or `mcp`.

## Handing work back to you

The MCP server has one more tool, **`write-sql-tool`**, for the statements an agent
should not run itself. It puts SQL in the SQL editor of the tql you have open on
the connection it names, in place of what was there, and tql switches to the
editor and says the SQL came from your agent. Nothing runs: you read it, change it
if you like, and run it yourself. One undo brings back what was in the editor.
With more than one tql open on that connection, the one you used last gets it;
with none open, the agent is told to ask you to open it.

## Letting an agent see your screen

**`read-screen-tool`** is the other direction: the agent sees what you see in the
tql you have open, so *this row* and *these results* mean something to it. It gets
the table or query results on screen, the statement behind them, what is in the
SQL editor, the sort, the row and column under your cursor, the page of rows
loaded, how many changes are unwritten and the status line. That is row data, so
it is off until you turn it on, and a production connection needs its own say-so:

```toml
[mcp]
read_screen = true        # let an agent read your screen
read_production = false   # and a connection tagged production too
```

tql says *your agent looked at your screen* on its status line each time it does,
and with sharing off it says your agent asked, while the agent is told what you
would have to change. Restart tql after changing either setting.

The `[mcp]` section in [Configuration](Configuration#mcp) has the details.
