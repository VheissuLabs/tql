# Packaging and releasing

tql is one Rust binary that needs nothing else on the machine. The Linux binaries
are static musl builds, so the same file runs on any distribution, and the macOS
binary links only what every Mac has. Each is built natively on its own GitHub
runner.

Everything below is driven by pushing a `v*` tag, which `bin/release` does.
`.github/workflows/release.yml` builds, checks and publishes. It also takes a
`workflow_dispatch`, which builds everything without publishing. Use that to try a
change to the pipeline.

After it publishes, `.github/workflows/install.yml` installs the new release the
way people will (the one-line installer on Linux x86_64, Linux ARM and macOS, and
the AUR package built with `makepkg` in an Arch container) and fails if any of them
does not run and report the new version. It runs on its own too, whenever
`install.sh` or `packaging/` changes and once a week, so a broken installer is
caught before someone runs into it.

## What ships

| | |
| --- | --- |
| `tql-linux-x86_64`, `tql-linux-aarch64` | standalone, needs nothing |
| `tql-macos-aarch64` | the same, for Apple Silicon Macs |
| `.deb`, `.rpm` | the matching binary, one per architecture, no dependencies |
| Homebrew | `packaging/homebrew/tql.rb`, pushed to the tap on release |
| AUR | `packaging/aur/PKGBUILD`, pushed to `tql-bin` on release |

`VERSION` and the `SHA_*` placeholders in the formula and the PKGBUILD are filled
in by the workflow from the tag and the published checksums.

To build a release binary locally:

```bash
cargo build --release                                     # for this machine
rustup target add x86_64-unknown-linux-musl              # a static Linux one, from Linux
cargo build --release --target x86_64-unknown-linux-musl  # needs musl-tools
```

The release profile in `Cargo.toml` turns on link-time optimisation and strips the
binary. The workflow checks every binary prints `Tql v<version>` for `--version`,
lists `mcp:start`, and on Linux is statically linked, before anything is published.

## Homebrew

Formulae live in a tap, which is a repository named `homebrew-<tap>`:

1. Create `VheissuLabs/homebrew-tap`, public, with a `Formula/` directory.
2. Give the release workflow a deploy key: an SSH key added to the tap with write
   access, whose private half is this repository's secret
   `HOMEBREW_TAP_DEPLOY_KEY`. It can touch the tap and nothing else, and it does
   not expire. Deploy keys must be allowed in the organisation's settings.

   ```bash
   ssh-keygen -t ed25519 -N '' -f tap_key
   gh repo deploy-key add tap_key.pub --repo VheissuLabs/homebrew-tap --allow-write
   gh secret set HOMEBREW_TAP_DEPLOY_KEY --repo VheissuLabs/tql < tap_key
   rm tap_key tap_key.pub
   ```

   A GitHub App (`TAP_APP_ID` and `TAP_APP_PRIVATE_KEY`) or a token
   (`HOMEBREW_TAP_TOKEN`) work too, when there is no deploy key.

Then every release updates `Formula/tql.rb` by itself, installs it from the tap on
a macOS runner to check it, and people install with:

```bash
brew install VheissuLabs/tap/tql
```

Until the secret exists the step skips itself and says so in the log, so the
release still succeeds.

## AUR

The package is `tql-bin`. The `-bin` suffix is the convention for a prebuilt binary
rather than one built from source on the user's machine.

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

3. Add the matching **private** key to this repository as the secret `AUR_SSH_KEY`,
   and releases keep it up to date after that.

```bash
yay -S tql-bin
```

## Debian and Fedora

The `.deb` and the `.rpm` are attached to each release, one per architecture, and
install with the system tools:

```bash
sudo dpkg -i tql_0.3.0_amd64.deb
sudo dnf install ./tql-0.3.0.x86_64.rpm
```

Hosting an actual apt or dnf *repository*, so `apt install tql` works without
downloading a file first, needs somewhere to serve it from and a signing key. That
is the next step, not a missing one: the packages themselves are already built and
correct.

## Adding a distribution

Anything that can install a single executable will work, since there is no runtime
to declare. Add the recipe under `packaging/`, a step to the release workflow that
fills in the version and the checksums, and a row to the table above.
