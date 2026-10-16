# Connecting

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
is written until you save. The driver is cycled with `← →`, through all four.

## Opening by path or connection string

```bash
tql open <path-or-dsn>        # save it and open it
tql connect <path-or-dsn>     # the same command
tql connect <connection>      # a saved connection, by name
tql connect prod --db=analytics  # another database on prod's server, this once
tql <path-or-dsn>             # the short form, same thing
```

`open` takes a SQLite path or a connection string, remembers it, and drops you
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
| `<path-or-dsn>` | a SQLite file, or a `mysql://`, `pgsql://`, `postgres://`, `sqlsrv://` or `mssql://` string; `mysql+ssh://` and the others with `+ssh` go through an SSH server first |
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

`mysql`, `mariadb`, `pgsql`, `postgres`, `postgresql`, `sqlsrv`, `mssql` and
`sqlite` schemes are understood. The default port is filled in per driver
(MySQL and MariaDB 3306, Postgres 5432, SQL Server 1433), credentials are
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

## Several databases on one server

MySQL, Postgres and SQL Server connections are a server, not a single database.
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

A SQLite connection is one file, so `b` says as much and does nothing.

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
ones tql cannot open, such as Redis, are listed and skipped.

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
