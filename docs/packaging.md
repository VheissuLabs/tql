# Packaging tql

One artifact does all of it: the phar built by `app:build`. Every package is
that file plus a dependency on PHP 8.4, so there is nothing per-distribution to
compile and nothing to keep in sync but a version and a checksum.

Everything below is driven by pushing a `v*` tag.
`.github/workflows/release.yml` builds the phar, smoke-tests it, and publishes.

## What ships where

| | |
| --- | --- |
| the binary | attached to the GitHub release, and what `install.sh` downloads |
| `.deb` | attached to the release, built by nfpm from `packaging/nfpm.yaml` |
| `.rpm` | the same config, `--packager rpm` |
| Homebrew | `packaging/homebrew/tql.rb`, pushed to the tap on release |
| AUR | `packaging/aur/PKGBUILD`, pushed to `tql-bin` on release |

`VERSION` and `SHA` in the formula and the PKGBUILD are placeholders; the
workflow fills them in from the tag and the checksum of the published binary.

To build the packages locally:

```bash
php -d phar.readonly=0 tql app:build tql --build-version=0.0.0
VERSION=0.0.0 nfpm package --config packaging/nfpm.yaml --packager deb --target dist/
VERSION=0.0.0 nfpm package --config packaging/nfpm.yaml --packager rpm --target dist/
```

## Homebrew

Formulae live in a tap, which is a repository named `homebrew-<tap>`:

1. Create `VheissuLabs/homebrew-tap`, public, with a `Formula/` directory.
2. Make a fine-grained token with **contents: write** on that repository and add
   it to this repository as the secret `HOMEBREW_TAP_TOKEN`.

Then every release updates `Formula/tql.rb` by itself, and people install with:

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

The `.deb` and the `.rpm` are attached to each release and install with the
system tools:

```bash
sudo dpkg -i tql_0.3.0_all.deb
sudo dnf install ./tql-0.3.0.noarch.rpm
```

Hosting an actual apt or dnf *repository* — so `apt install tql` works without
downloading a file first — needs somewhere to serve it from and a signing key.
That is the next step, not a missing one: the packages themselves are already
built and correct.

## Adding a distribution

Anything that can install a single executable and depend on PHP will work. Add
the recipe under `packaging/`, a step to the release workflow that fills in the
version and checksum, and a row to the table above.
