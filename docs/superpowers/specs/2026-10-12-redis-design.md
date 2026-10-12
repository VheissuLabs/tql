# Redis and Valkey

Date: 2026-10-12
Status: proposed
Release: 0.14.0, third of three parts (schemas, then the new SQL drivers, then
this)

## Goal

Browse, edit and query Redis and Valkey from tql, with the same connection
list, palette, settings, tags, SSH tunnels and status bar as the SQL engines,
and a key browser built for what Redis holds.

Valkey speaks the Redis protocol, so one driver serves both. The connection
says which it is from the server's `INFO`.

## Decisions already made

- Keys are shown as a tree of folding namespaces, split on `:`.
- Every type can be viewed. Strings, hashes, lists, sets and sorted sets can be
  edited, keys added and deleted, and TTLs set. Edits wait for `:w`, as row
  edits do, and a read-only connection refuses them.
- A command line runs raw commands.

## Out of scope

- Redis Cluster and Sentinel. tql connects to one server.
- Pub/sub, `MONITOR` and other commands that stream forever. The command line
  refuses them.
- Module types (RedisJSON, RediSearch, time series, bloom filters). They are
  listed with their type and their value is not shown.
- Editing streams. Streams can be viewed.
- MCP and agent access to Redis. `tql query`, `describe`, `tables`, `export`,
  `dump`, `load` and `sync` say a Redis connection is not a SQL database.

## What the user sees

### Adding a connection

The connection form gets a Redis driver, labelled "Redis / Valkey":

| Field | Notes |
| --- | --- |
| host, port | 6379 by default |
| username | optional, for ACL users |
| password | optional |
| database | the number, 0 by default |
| TLS | off by default |

SSH tunnels work as they do for the SQL engines. `tql open` and `:open` take
`redis://`, `rediss://` (TLS), `valkey://` and `valkeys://` URLs, with the
database number as the path (`redis://:pass@host:6379/2`). TablePlus import
brings over its Redis connections.

### The key browser

Opening a Redis connection opens the key browser in place of the table
browser. It has the same layout: keys on the left, the value on the right, the
status bar and hotkey bar below.

```
╭─ [1] keys · db0 ─────────╮ ╭─ [2] user:1 · hash · 4 fields · TTL none ───────╮
│ ▾ user           (3)     │ │ field        value                             │
│     user:1        hash   │ │ name         Ada Lovelace                      │
│     user:2        hash   │ │ email        ada@example.com                   │
│     user:sessions zset   │ │ plan         pro                               │
│ ▸ cache          (1204)  │ │ created_at   2026-10-12T09:14:00Z              │
│ ▸ queue          (6)     │ │                                                │
│   config         string  │ │                                                │
╰──────────────────────────╯ ╰────────────────────────────────────────────────╯
```

- **Loading:** keys are read with `SCAN`, never `KEYS`, a batch at a time in
  the background. The list fills as batches arrive, and the header says how
  many keys have been read out of `DBSIZE`. Scanning stops at 100,000 keys and
  says so; `/` with a pattern narrows the scan on the server.
- **Namespaces:** a key is split on `:` into folding groups, nested as deep
  as the key goes. A namespace shows how many keys it holds and starts folded,
  except when the whole database has fewer than 200 keys. Folding works as on
  connection groups. A key with no `:` sits at the top level.
- **Types:** each key shows its type: `string`, `hash`, `list`, `set`, `zset`,
  `stream` or the module's own type name. Types are read with `TYPE` for the
  keys on screen, in a pipeline.
- **`/`** takes a Redis glob pattern (`user:*`, `*session*`). A pattern with no
  glob characters is matched as `*text*`.
- **`b`** switches database, listing `db0` to `db15` (or as many as the server's
  `databases` setting allows) with the key count of each from
  `INFO keyspace`.
- **Reload:** `r` rescans.

### Values

The value pane's title says the key, its type, its size and its TTL.

| Type | Shown as |
| --- | --- |
| string | the text, wrapped; JSON pretty-printed with the JSON viewer; binary as hex with its length |
| hash | rows of field and value, read with `HSCAN` |
| list | rows of index and value, read with `LRANGE` a page at a time |
| set | rows of member, read with `SSCAN` |
| zset | rows of member and score in score order, read with `ZRANGE … WITHSCORES` a page at a time |
| stream | rows of entry id and its fields as JSON, read with `XRANGE` a page at a time |

Rows use the grid, so moving, paging, yanking, narrowing and widening, `I` to
view a value, and the inspector behave as they do on a table.

### Editing

Edits mark the row the way row edits do, and nothing reaches the server until
`:w`. `:w` sends every pending change in one `MULTI` … `EXEC`, so either all of
them happen or none do. `u` drops the changes, as it does on a table.

| Key | On a string | On a hash | On a list | On a set | On a zset |
| --- | --- | --- | --- | --- | --- |
| `e` | edit the value | edit the field's value | edit the item (`LSET`) | replace the member | edit the score |
| `N` | | add a field | push an item at the end | add a member | add a member and score |
| `d` | | delete the field | remove the item | remove the member | remove the member |

On the key list:

- `N` adds a key: its name, its type, and a first value.
- `d` marks a key for deletion.
- `T` sets the TTL in seconds. An empty TTL removes the expiry.
- `R` renames the key (`RENAME`, refused when the new name exists).

A list item is removed by its index and value together, so an item that has
moved since it was read is not removed by mistake. When it has moved, `:w`
says so and keeps the change pending.

### Read-only

A read-only connection refuses edits, marks and `:w`, as it does for SQL, and
its command line runs only commands that the server flags `readonly` in
`COMMAND INFO`. tql asks the server once per
connection, so it stays correct for whatever version and modules are there.

### The command line

`s` opens the Redis command pane where the SQL editor is on a SQL connection.
It takes one command per line, such as `HGETALL user:1` or `ZRANGE leaders 0 9
WITHSCORES`. Strings are quoted as in `redis-cli`. `ctrl+r` runs the line
under the cursor and the reply shows in the value pane:

- a list as rows
- a map as field and value rows
- a scalar as text
- an error in red

Commands are remembered in the query history and completed from the server's
`COMMAND` list. A command that blocks (`BLPOP`, `XREAD BLOCK`) or streams
(`SUBSCRIBE`, `MONITOR`) is refused.

`:` keeps tql's own commands, so `:w`, `:q`, `:open`, `:settings` and the
rest work as everywhere else. `:sql` is called `:cli` on a Redis connection.

## Design

- `Driver` gains `Redis`, named `redis`, labelled "Redis / Valkey", port 6379.
- `database::connect` stays SQL only. A new `keystore` module holds a
  `KeyStore` trait (scan, type, read a value a page at a time, write a batch,
  TTL, rename, run a command, and command flags) and its Redis implementation.
  It uses the `redis` crate synchronously with its rustls TLS, over the
  existing SSH tunnel when one is set.
- A new `tui/keys` browser implements `Tui` beside `tui/browser`. It reuses the
  chrome, the grid, the status bar, the hotkey bar, the palette, the settings
  modal, the value viewer and the line editor. `browse.rs` opens it for a Redis
  connection and handles its exits the same way.
- The registry marks each command with the screens it works on. Commands that
  only make sense on a SQL connection, such as `:export` and
  `:create database`, say so on a Redis connection.
- Scanning and value paging run on a thread and report back through the idle
  hook, as asking the AI does now, so a large keyspace never freezes the
  screen.

## Testing

- The Redis implementation runs against a live Redis and a live Valkey in the
  live kinds suite, skipped when neither is reachable. CI runs
  `redis:7` and `valkey/valkey:8` as services.
- Each type is read, paged and edited, `:w` is atomic (one failing change sends
  none), TTL and rename work, and a moved list item is not removed.
- The command pane runs a read command, refuses a write on a read-only
  connection, and refuses a blocking command.
- The key browser on a fake key store covers: namespace folding; a scan
  arriving in batches; `/` patterns; database switching; edits marked and
  dropped.
- `dsn`, `opener`, the connection form and TablePlus import cover Redis.
- The SQL-only shell commands refuse a Redis connection.
