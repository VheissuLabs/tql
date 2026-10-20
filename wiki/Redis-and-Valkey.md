A Redis or Valkey connection opens a key browser in place of the table list.
Open one with a connection string or the connection form:

```bash
tql open redis://:password@127.0.0.1:6379/0
tql open rediss://:password@cache.example.com:6380    # TLS
tql open valkey://127.0.0.1:6379                       # valkeys:// for TLS
```

Valkey speaks the same protocol as Redis, so everything here works on both.

## Keys

Keys are read with `SCAN` in the background, never `KEYS`, so a server with
millions of keys stays responsive while the list fills in. They fold into
namespaces on `:`, so `user:1:profile` sits under `user` and `user:1`; `↵` or
`l` unfolds a namespace and `h` folds it. `/` narrows the list with a pattern
such as `user:*`, or plain words, which become `*words*`. `b` switches database
and `r` scans again.

## Values

Every type shows as itself:

| Type | Shown as |
| --- | --- |
| string | text; JSON pretty-printed, binary as hex |
| hash | fields and values, and on Redis 7.4 and Valkey 9 each field's TTL |
| list | its items, in order |
| set | its members |
| sorted set | members and scores |
| stream | its entries |

A long value pages with `n` and `p`.

## Editing

`e` edits a value, a field, an item, a member or a score, `N` adds one, and `d`
marks one for removal. On the key list, `N` makes a key, `d` deletes one, `T`
sets its TTL and `R` renames it. On a hash field, `T` sets or clears that
field's TTL (Redis 7.4 and Valkey 9). An empty TTL keeps the key forever.

Edits wait for `:w`, which sends them all in one transaction, so either every
one happens or none does; `u` drops them. A list item that moved since tql read
it is not removed by mistake. Editing a string keeps its TTL, and a binary
value is shown but not edited.

A read-only connection refuses every edit.

## The command pane

`s` opens a command pane for anything else, such as `HGETALL user:1` or
`ZRANGE leaders 0 9 WITHSCORES`; `ctrl+r` runs the line and `esc` closes it.

On a read-only connection it runs only commands the server says only read. It
never runs one that waits forever, such as `BLPOP` or `SUBSCRIBE`, or one that
would change the connection under tql, such as `SELECT` (use `b`). A password
in an `AUTH` command stays out of the history.

## The slow log

`:log` shows the server's recent slow commands: Valkey's command log, or Redis's
slow log.

Every key is on [Keys](Keys#the-key-browser).
