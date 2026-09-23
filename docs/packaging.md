# Packaging tql

tql is a PHP application that ships as a binary with **no PHP required on the
target machine**. That is static-php-cli's `micro`: a statically linked PHP
interpreter with a self-extracting stub, which takes an appended phar and runs
it. A release binary is literally:

```bash
cat micro.sfx tql.phar > tql
```

The interpreter is built once per platform from a fixed extension list — the
three PDO drivers, openssl, curl, mbstring and the pieces Laravel expects — and
cached between releases, since it only changes when that list or the PHP version
does.

Everything below is driven by pushing a `v*` tag, which `bin/release` does.
`.github/workflows/release.yml` builds, checks and publishes. It also takes a
`workflow_dispatch`, which builds everything without publishing — use that to
try a change to the pipeline.

After it publishes, `.github/workflows/install.yml` installs the new release the
way people will — the one-line installer on Linux x86_64, Linux ARM and macOS,
and the AUR package built with `makepkg` in an Arch container — and fails if any
of them does not run and report the new version. It runs on its own too, whenever
`install.sh` or `packaging/` changes and once a week, so a broken installer is
caught before someone runs into it.

## What ships

| | |
| --- | --- |
| `tql-linux-x86_64`, `tql-linux-aarch64` | standalone, needs nothing |
| `tql-macos-aarch64` | the same, for Apple Silicon Macs |
| `tql.phar` | 8MB, for a machine that has PHP 8.4 already |
| `.deb`, `.rpm` | the matching binary, one per architecture, no dependencies |
| Homebrew | `packaging/homebrew/tql.rb`, pushed to the tap on release |
| AUR | `packaging/aur/PKGBUILD`, pushed to `tql-bin` on release |

`VERSION` and the `SHA_*` placeholders in the formula and the PKGBUILD are
filled in by the workflow from the tag and the published checksums.

To build a standalone binary locally:

```bash
curl -fsSL -o spc https://dl.static-php.dev/static-php-cli/spc-bin/nightly/spc-macos-aarch64
chmod +x spc
./spc doctor --auto-fix
./spc download --with-php=8.4 --for-extensions="$EXTENSIONS" --prefer-pre-built
./spc build "$EXTENSIONS" --build-micro

php -d phar.readonly=0 tql app:build tql --build-version=0.0.0
cat buildroot/bin/micro.sfx builds/tql > tql-local && chmod +x tql-local
```

`$EXTENSIONS` is in the workflow, in one place, because the binary and the
packages must agree about it.

## Homebrew

Formulae live in a tap, which is a repository named `homebrew-<tap>`:

1. Create `VheissuLabs/homebrew-tap`, public, with a `Formula/` directory.
2. Make a fine-grained token with **contents: write** on that repository and add
   it to this repository as the secret `HOMEBREW_TAP_TOKEN`.

Then every release updates `Formula/tql.rb` by itself, installs it from the tap
on a macOS runner to check it, and people install with:

```bash
brew install VheissuLabs/tap/tql
```

Until the secret exists the step skips itself and says so in the log, so the
release still succeeds.

## AUR

The package is `tql-bin` — the `-bin` suffix is the convention for a prebuilt
binary rather than one built from source on the user's machine.

1. Make an account at [aur.archlinux.org](https://aur.archlinux.org) and add an
   SSH public key to it.
2. Submit the package once by hand, from `packaging/aur/PKGBUILD` with the
   placeholders filled in:

   ```bash
   git clone ssh://aur@aur.archlinux.org/tql-bin.git
   cd tql-bin
   # copy the PKGBUILD in, then
   makepkg --printsrcinfo > .SRCINFO
   git add PKGBUILD .SRCINFO && git commit -m 'tql 0.3.0' && git push
   ```

3. Add the matching **private** key to this repository as the secret
   `AUR_SSH_KEY`, and releases keep it up to date after that.

```bash
yay -S tql-bin
```

## Debian and Fedora

The `.deb` and the `.rpm` are attached to each release, one per architecture,
and install with the system tools:

```bash
sudo dpkg -i tql_0.3.0_amd64.deb
sudo dnf install ./tql-0.3.0.x86_64.rpm
```

Hosting an actual apt or dnf *repository* — so `apt install tql` works without
downloading a file first — needs somewhere to serve it from and a signing key.
That is the next step, not a missing one: the packages themselves are already
built and correct.

## Adding a distribution

Anything that can install a single executable will work — there is no runtime to
declare. Add the recipe under `packaging/`, a step to the release workflow that
fills in the version and the checksums, and a row to the table above.
