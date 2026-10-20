Running `tql` with nothing after it opens the connection list, which is how you
will use it nearly all the time. From there, `n` adds a connection and `↵` opens
one.

| key | what it does |
| --- | --- |
| `↵` | open it |
| `e` | edit it in place |
| `n` | add one |
| `d` | mark it for deletion |
| `u` | clear every mark |
| `:w` | write the marked deletions |

Adding and editing happen in a modal over the list, never by dropping out to a
prompt sequence. `↑↓` picks a field, `↵` edits it with a real cursor (arrows,
home, end, backspace, delete, paste), `ctrl+s` saves and `esc` cancels. Nothing
is written until you save. The driver is cycled with `← →`, through all nine.

## Opening by path or connection string

```bash
tql open <path-or-dsn>        # save it and open it
tql connect <path-or-dsn>     # the same as open
tql connect <connection>      # a saved connection, by name
tql connect prod --db=analytics  # another database on prod's server, this once
tql <path-or-dsn>             # the short form, same thing
```

`open` takes a SQLite or DuckDB file or a connection string, remembers it, and drops you
straight into it. Opening one you already have reuses that connection rather
than saving a second.

```bash
tql open ~/Code/app/database/database.sqlite
tql open "mysql://root@127.0.0.1:3306/shop" --name="Shop" --tag=local
tql open "$DATABASE_URL" --peek          # use it without saving it
tql connect 'mysql+ssh://forge@203.0.113.7/forge:secret@127.0.0.1?name=app&env=production'
```

| Argument or flag | What it does |
| --- | --- |
| `<path-or-dsn>` | a SQLite or DuckDB file, or a `mysql://`, `pgsql://`, `postgres://`, `sqlsrv://`, `mssql://`, `libsql://`, `clickhouse://`, `snowflake://`, `redis://` or `valkey://` string (`clickhouses://`, `rediss://` and `valkeys://` for TLS); `mysql+ssh://` and the others with `+ssh` go through an SSH server first |
| `--name=<name>` | what to call it in the connection list; on one already saved, a rename |
| `--tag=<tag>` | what it *is*: `production`, `staging`, `dev` or `local`, which decides the color it wears; see [Tags and read only](#tags-and-read-only) |
| `--peek` | open it without saving it |

Quote a connection string: `?` and `&` mean something to your shell. Use single
quotes when the password has a `$`, `!` or backtick in it.

From a TablePlus URL, tql takes `name` as the name and `env` as the tag
(`production`, `staging`, `local`, and `development` or `testing` as `dev`), and
ignores the rest, such as `statusColor` and `safeModeLevel`.

The `mysql+ssh://` form is the one TablePlus copies: the SSH server first, then
the database behind it. tql saves the tunnel with it, takes `name` as the name
and `env` as the tag, and leaves the SSH key to your agent and `~/.ssh/config`.

### What is remembered

Anything you open is **remembered**, so you only ever paste a connection string
once. `--name=` names it in the list; without one it is named after the file, or
`database on host`. `--peek` opens without remembering, for a database you are
only glancing at. Re-opening somewhere you already have saved reuses that
connection rather than making a second, and `--name=` on it is a rename. Names
are unique, so a second `database.sqlite` becomes `database.sqlite (2)`.

**A connection string on the command line goes into your shell history.** Paste
it once, then use the connection list, where the password is encrypted at rest.
Prefixing the command with a space keeps it out of history in zsh if
`HIST_IGNORE_SPACE` is set.

A first argument that exists on disk, contains a `/`, ends in `.sqlite`,
`.sqlite3` or `.db`, or looks like a connection string is treated as a path
rather than a command name. A file that exists is never a command name, which
makes the rewrite unambiguous. On first run tql creates `~/.config/tql/`
containing `tql.sqlite` (connections and query history) and `key` (the
encryption key), both `0600`.

### Schemes and defaults

`mysql`, `mariadb`, `pgsql`, `postgres`, `postgresql`, `sqlsrv`, `mssql`,
`sqlite`, `libsql`, `clickhouse`, `clickhouses`, `snowflake`, `redis`, `rediss`,
`valkey` and `valkeys` schemes are understood, and an `https://` URL with an
`authToken=` is libSQL. The default port is filled in per driver (MySQL and
MariaDB 3306, Postgres 5432, SQL Server 1433, ClickHouse 8123 or 8443 over TLS,
Redis and Valkey 6379), credentials are
percent-decoded, and `?name=` sets the label shown in the title bar.

## Tags and read only

A connection can carry a **tag**, and the tag decides its color:

| tag | color |
| --- | --- |
| production | red |
| staging | yellow |
| dev | blue |
| local | green |

The tag colors the driver icon and the tag itself in the connection list, and
the connection name in the status line while you are in it, so the screen tells
you where you are before you press `d`.

It is a fixed set rather than a configurable one, on purpose: the point of a tag
is that production looks the same in your terminal and in someone else's
screenshot.

**Read only** refuses every write on that connection: no edits, no marks, no
`:w`, and the SQL editor runs only statements that read.

## Groups

A connection can also carry a **group**, set in its form. The connection list
shows each group under a header, after the connections without one, with the
number of connections in it, and `space` folds it shut. Folded groups stay
folded the next time.

Press `e` on a group's header to rename it, for every connection in it. A name
another group already has merges the two.

## libSQL and Turso

A libSQL connection is a URL and an auth token: `tql open
"libsql://shop-you.turso.io?authToken=…"`, or the connection form's **URL** and
**Auth token**. tql speaks Turso's HTTP API, so a self-hosted `sqld` works too
(`http://127.0.0.1:8080`). Everything SQLite does works here, editing included.

## ClickHouse

ClickHouse connects over its HTTP interface, port 8123, or 8443 with an
`ssl_mode` set (`clickhouses://` in a connection string). Databases switch with
`b`, and every type shows as itself: arrays, maps and tuples as JSON, and big
integers and decimals exactly. Tables are edited like any other (see
[Editing data](Editing-Data)), with one difference: ClickHouse keys need not be
unique, so before each change tql counts the rows it would reach and changes
nothing if that is more than the rows you edited. The change then runs as a
mutation that tql waits for, so `:w` is done when it says so. Key columns cannot
be changed; ClickHouse does not allow it.

## Snowflake

Snowflake connects over its SQL API with its account, the `myorg-myaccount`
part of your Snowflake URL, signing in one of four ways, chosen with **Sign in
with** in the connection form or `authenticator` in a connection string:

| Sign in with | What it needs |
| --- | --- |
| Access token | a programmatic access token, saved as the password: `snowflake://user:token@myorg-myaccount/DATABASE?warehouse=WH&role=ROLE` |
| Key pair | the login name and the path to the RSA private key whose public key is the user's `RSA_PUBLIC_KEY`; an encrypted key's passphrase is the password: `?authenticator=snowflake_jwt&private_key_path=~/.snowflake/rsa_key.p8` |
| Browser sign-in | nothing: Snowflake's sign-in page opens, with your company's single sign-on behind it. A connection with no access token uses it: `snowflake://myorg-myaccount/DATABASE?role=ROLE` |
| SSO through the browser | the login name: your identity provider (Okta, Entra ID and the rest) opens directly, `?authenticator=externalbrowser`. It works on accounts without the built-in OAuth integration the browser sign-in needs |

Either browser sign-in is saved with the connection, encrypted like a password,
and renewed as it runs out, so the next launch, `tql dump`, `tql export` and the
MCP tools use it without opening the browser. `:sign-out` on the connection
list forgets the one saved with the highlighted connection. The browser sign-in
uses Snowflake OAuth for local applications; if Snowflake shows an error instead
of its sign-in page, the account does not have the `SNOWFLAKE$LOCAL_APPLICATION`
integration yet, and SSO through the browser or an access token works instead.
An access token from a person rather than a service user needs a network
policy before Snowflake accepts it.

Warehouse and role fall back to the user's defaults. Tables, columns and keys
come from `SHOW` commands, which do not wake the warehouse; writes do. Snowflake
does not enforce primary keys, so tql edits a row inside a transaction that
commits only if exactly that row changed: a key that matches two rows changes
neither, and tql says so.

## Redis and Valkey

A Redis or Valkey connection opens a key browser in place of the table list:
`tql open redis://:password@host:6379/0`, `rediss://` for TLS, or the
connection form. See [Redis and Valkey](Redis-and-Valkey).

## Several databases on one server

MySQL, Postgres, SQL Server, ClickHouse and Snowflake connections are a
server, not a single database, and a Redis or Valkey server has its numbered databases.
Press **`b`** to list the databases on that server and switch to one; the tables
pane titles itself with the database you are in.

Leave the **Database** field empty on a server connection and tql asks on
connect, opening the list as soon as it is in.

Type a name the list does not have and its last row offers to create it. On a
connection tagged `production`, `↵` puts `create database <name>` on the command
line for a second `↵` instead of creating it straight away;
`confirm_create_on_production = false` in
[`[ui]`](Configuration#ui) turns that off.

The switch lasts for the session only. It is never written back to the saved
connection, so the connection still opens on its own database next time. It also
drops the current filter, sort and any pending edits, since none of them mean
anything in another database.

A SQLite or DuckDB connection is one file, so `b` says as much and does nothing.

## After the laptop sleeps

A connection the server dropped while you were away, after a sleep or a change
of network, comes back on its own. MySQL and Postgres check a connection that
has sat idle for half a minute before using it and connect again if it has
gone, so your next statement just runs; nothing is sent twice. A connection
through SSH starts its tunnel again first. SQL Server reconnects after the
first statement that finds the connection broken.

## Databases behind SSH

A connection can reach its database through an SSH tunnel. Set **SSH host** on a
mysql, postgres or sqlsrv connection and the rest of the fields appear:

| field | what it is |
| --- | --- |
| SSH host | the machine you can reach, e.g. `bastion.example.com` |
| SSH port | 22 unless you say otherwise |
| SSH user | the user on that machine |
| SSH key | a key file; empty uses your agent and `~/.ssh/config` |
| SSH password | for a bastion that wants one instead of a key |

Press `↵` on **SSH key** and it lists the private keys it found in `~/.ssh`, so
there is no path to remember. The same goes for the SSL certificate fields, which
also look in `~/.postgresql`, `~/.mysql`, `~/certs`, `~/Downloads` and the
current directory. `type a path…` is always the last option.

tql opens `ssh -N -L <free port>:<db host>:<db port>` and points the driver at
that local port, so the database only ever sees a connection from the machine you
tunnelled through.

It shells out to your own `ssh` rather than speaking the protocol, which means
your agent, your keys and your `~/.ssh/config` all apply, including `Host`
aliases and jump hosts. A connection you can already make by typing `ssh prod`
works by putting `prod` in the SSH host field.

The tunnel is opened once per destination and reused, and stays open until tql
exits. tql waits for the forwarded port to accept a connection before running
anything, so the first query cannot race the tunnel coming up, and if ssh fails
it says what ssh said. While it connects, the connection list stays on screen and
says so.

## TLS

**SSL mode** on a server connection takes `disable`, `prefer`, `require`,
`verify-ca` or `verify-full`, and reveals the CA, cert and key fields. Managed
databases usually want `require` and a CA certificate. Only `verify-full` checks
the hostname.

SQL Server is encrypted by default without checking the server's certificate,
which is what a local or self-signed SQL Server needs; `verify-ca` or
`verify-full` checks it, and `disable` turns encryption off. It needs a server
that speaks TLS 1.2 or newer, which is every SQL Server from 2016 on and the
older ones that have had their TLS 1.2 update.

## Coming from TablePlus

```bash
tql tableplus --dry-run   # what would come over
tql tableplus             # bring it over
```

tql reads the connections TablePlus for Mac has saved and adds each one, with its
group, its environment as a tag, and its SSH tunnel and TLS settings. Passwords
come from the Keychain: macOS asks you to allow each one, and Deny leaves that
one out. Connections tql already has are left alone, so running it again is safe;
ones tql cannot open, such as MongoDB, are listed and skipped.

| Flag | What it does |
| --- | --- |
| `--dry-run` | list what would come over and what would be skipped, and change nothing; the Keychain is not touched |
| `--without-passwords` | bring the connections over but leave every password in the Keychain; add them later with `e` |
| `--from=<folder>` | read TablePlus's `Data` folder from somewhere other than `~/Library/Application Support/com.tinyapp.TablePlus` (or its Setapp twin) |
| `--json` | answer in JSON, which is what you get anyway when the output is piped |
| `--table` | answer in lines for a person, which is what you get anyway in a terminal |

A connection TablePlus is set to ask about every time keeps no password, and
neither does an SSH key's passphrase: tql leaves the key to `ssh`, your agent and
`~/.ssh/config`. TablePlus for Windows and Linux is not supported yet.

Or ask your agent to *move me from TablePlus to tql*: both runs answer in JSON
when an agent makes them, nothing in that JSON is a password, and
`tql tableplus --help` tells the agent to show you the dry run first.

## Connection tooling for scripts and agents

```bash
tql connections                 # list the saved connections
tql tables <connection>         # its tables
tql describe <connection> <table>
```

These never prompt or draw a screen, and answer in JSON when piped. See
[For agents](For-Agents) and [Commands](Commands).
