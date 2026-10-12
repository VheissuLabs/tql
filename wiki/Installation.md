# Installation

One file, no runtime to install. The binary has all four database drivers built
in, and on Linux it is statically linked, so it runs on any distribution. On
first run it creates `~/.config/tql/` and migrates its own store, so there is
nothing to set up.

Use your package manager if it is listed here: nothing runs but the package
itself, and it upgrades and removes tql like anything else. Installed any other
way, tql keeps itself up to date, see [Updates](Configuration#updates).

## macOS and Linux, with Homebrew

```bash
brew install vheissulabs/tap/tql
brew upgrade tql                          # later, for a new release
```

Apple Silicon Macs, and Linux on x86_64 and ARM. It installs the same standalone
binary, with no dependencies. Homebrew checks the binary against the checksum in
the formula before it installs it.

## Debian and Ubuntu, with apt

Every release carries a `.deb` for each architecture, with no dependencies.
Download it from [the latest release](https://github.com/VheissuLabs/tql/releases/latest),
or with the GitHub CLI:

```bash
gh release download --repo VheissuLabs/tql --pattern '*_amd64.deb'   # arm64: '*_arm64.deb'
sudo apt install ./tql_*_amd64.deb
```

Install a newer `.deb` the same way to upgrade; `sudo apt remove tql` takes it
off again.

## Fedora and RHEL, with dnf

The same again as an `.rpm`:

```bash
gh release download --repo VheissuLabs/tql --pattern '*.x86_64.rpm'  # ARM: '*.aarch64.rpm'
sudo dnf install ./tql-*.x86_64.rpm
```

A newer `.rpm` upgrades it; `sudo dnf remove tql` removes it.

There is no apt or dnf repository yet, so `apt upgrade` and `dnf upgrade` do not
see new releases. `tql --version` says which one you have.

## Arch

tql is not on the AUR yet. Releases after 0.7.0 carry a `PKGBUILD` filled in for
that release, so `makepkg` builds a proper pacman package from it. Read it first,
it is twenty lines:

```bash
mkdir tql && cd tql
gh release download --repo VheissuLabs/tql --pattern PKGBUILD
makepkg -si
```

Until then, use Homebrew, the binary by hand below, or the install script.

## The binary, by hand

The releases carry `tql-linux-x86_64`, `tql-linux-aarch64`, and
`tql-macos-aarch64` for Apple Silicon. Put the one for your machine anywhere on
your PATH:

```bash
gh release download --repo VheissuLabs/tql --pattern tql-macos-aarch64
chmod +x tql-macos-aarch64 && mv tql-macos-aarch64 /usr/local/bin/tql
```

## The install script

```bash
curl -fsSL https://raw.githubusercontent.com/VheissuLabs/tql/main/install.sh | sh
```

It picks the build for your platform, puts it somewhere on your PATH
(`/usr/local/bin` if it can write there, `~/.local/bin` if not) and tells you
where it went. `TQL_BIN_DIR` chooses the directory, `TQL_VERSION=v0.4.1` pins a
version.

Piping a script from the internet into a shell runs whatever it says, so if you
would rather read it first, and you should, download it, read it, then run it:

```bash
curl -fsSLO https://raw.githubusercontent.com/VheissuLabs/tql/main/install.sh
less install.sh
sh install.sh
```

## Checking a download

Releases after 0.7.0 carry `SHA256SUMS`, the checksum of every file in the
release. Download it next to what you downloaded and check:

```bash
gh release download --repo VheissuLabs/tql --pattern SHA256SUMS
sha256sum --check --ignore-missing SHA256SUMS          # macOS: shasum -a 256 --check --ignore-missing SHA256SUMS
```

`OK` beside your file means it is byte for byte what the release built.

## Updating

How a new release reaches you depends on how you installed tql. The one-line
script and the by-hand binary update themselves; a package manager owns its own
files and tql never replaces them.

```bash
tql update        # check now and install, while you keep working
```

Inside the interface that is `:update`, and the result lands in the status line.
The `[updates]` section in [Configuration](Configuration#updates) has the full
behaviour, including `check = false` and `automatic = false`.

## Coming from tql 0.11 or earlier

tql was written in PHP up to 0.11. 0.12 is the same tql rewritten in Rust, and
it moves over without you doing anything:

- **Your connections, settings and history stay.** It reads the same
  `~/.config/tql/`, passwords included, and adds any new settings to
  `config.toml` with a notice.
- **`tql update` or the automatic update brings it in**, if you installed with
  the install script or by hand. Homebrew, `.deb`, `.rpm` and `PKGBUILD` users
  upgrade the usual way.
- **`tql.phar` is gone.** If you ran the phar, install one of the binaries
  above instead; none of them needs PHP.
- **SQL Server works on Linux now**, and on macOS without Microsoft's ODBC
  driver.

## Coming from TablePlus

`tql tableplus` reads the connections TablePlus for Mac has saved and adds each
one. See [Connecting](Connecting#coming-from-tableplus) for the flags and what
comes over.
