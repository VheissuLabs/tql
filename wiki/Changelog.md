# Changelog

What changed in each release. The full file lives in the repository and is always
up to date:

- [CHANGELOG.md](https://github.com/VheissuLabs/tql/blob/main/CHANGELOG.md)
  — every release, grouped into New Features, Enhancements and Bug Fixes.

Fuller announcements for the recent releases live under `release-notes/`:

| Release | |
| --- | --- |
| [0.13.2](https://github.com/VheissuLabs/tql/blob/main/release-notes/v0.13.2.md) | More of tql's shell commands, from inside tql (`:update`, `:open`, `:export database`, `:v`, `:help`) |
| [0.13.1](https://github.com/VheissuLabs/tql/blob/main/release-notes/v0.13.1.md) | Show or hide the SQL editor (`\|`, `toggle_sql`), and wide-character fixes |
| [0.13.0](https://github.com/VheissuLabs/tql/blob/main/release-notes/v0.13.0.md) | A settings screen, and a command palette everywhere |
| [0.12.2](https://github.com/VheissuLabs/tql/blob/main/release-notes/v0.12.2.md) | Read-only enforced in more places, wide characters, export paging |
| [0.12.1](https://github.com/VheissuLabs/tql/blob/main/release-notes/v0.12.1.md) | `tql -v` prints the version |
| [0.12.0](https://github.com/VheissuLabs/tql/blob/main/release-notes/v0.12.0.md) | The Rust rewrite: the same tql, a third of the size |
| [0.11.0](https://github.com/VheissuLabs/tql/blob/main/release-notes/v0.11.0.md) | `tql dump`, `tql load` and `tql sync`, and the TablePlus import |
| [0.9.0](https://github.com/VheissuLabs/tql/blob/main/release-notes/v0.9.0.md) | SQL Server on macOS, and the agent commands |
| [0.6.0](https://github.com/VheissuLabs/tql/blob/main/release-notes/v0.6.0.md) | The releases before 0.9 |

## 0.13.2

- `:update` checks for a newer tql and installs it in the background, on either
  screen, with the result in the status bar.
- `:open <path or connection string>` opens a SQLite file or a connection string,
  saving it as `tql open` does and reusing one already saved.
- `:export database` writes every table in the current database to one `.sql`
  file.
- `:v` / `:version` say which version of tql is running, and `:help` / `:h` open
  help.

## 0.13.0

- A settings screen: `tql settings`, `:settings`, or Settings in the ctrl+k
  palette. Every setting sits under a section that folds, takes effect at once,
  and is written to `config.toml` straight away, that one line, your comments and
  order kept.
- A ctrl+k palette on the connection list, with its commands and every connection
  by name.
- `tql new`, `tql sql [connection]` and `tql history [connection]` open tql on the
  screen the command belongs to.

## 0.12.0

The Rust rewrite. It is the same tql: the same screens, keys, commands, config and
connections, checked frame by frame against 0.11. It is also a third of the size
(11 MB on a Mac, down from 35), starts in about 7 ms instead of 170, needs nothing
else installed, and SQL Server is built in on macOS and Linux. Everything that
behaves differently from 0.11 on purpose is listed in `parity/deliberate.md`.

Bringing your existing install over takes nothing: connections, passwords, settings
and history carry over from the same `~/.config/tql`. See
[Installation](Installation#coming-from-tql-011-or-earlier).