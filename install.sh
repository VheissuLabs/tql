#!/usr/bin/env sh
# Install tql, a database client for the terminal.
#
#   curl -fsSL https://raw.githubusercontent.com/VheissuLabs/tql/main/install.sh | sh
#
# The binary needs nothing else on the machine. Honours TQL_VERSION (a tag
# such as v0.3.0, default: the latest release) and TQL_BIN_DIR (where to put
# it).

set -eu

REPO="VheissuLabs/tql"
VERSION="${TQL_VERSION:-latest}"

say() { printf '%s\n' "$1"; }
die() { printf '\n%s\n' "$1" >&2; exit 1; }

command -v curl >/dev/null 2>&1 || die "tql needs curl to install."

case "$(uname -s)" in
    Darwin) OS="macos" ;;
    Linux) OS="linux" ;;
    *) die "No tql binary for $(uname -s). It builds from source: https://github.com/$REPO" ;;
esac

case "$(uname -m)" in
    x86_64 | amd64) ARCH="x86_64" ;;
    arm64 | aarch64) ARCH="aarch64" ;;
    *) die "No tql binary for $(uname -m)." ;;
esac

ASSET="tql-$OS-$ARCH"

[ "$ASSET" != "tql-macos-x86_64" ] || die "tql needs an Apple Silicon Mac; there is no build for Intel Macs."

SUDO=""
[ "$(id -u)" = 0 ] || SUDO="sudo"

# On Arch, install the release's pacman package rather than a loose binary, so
# pacman upgrades and removes it like everything else. Returns 1 when the
# release has no package (0.17.0 and earlier), and the binary goes in instead.
install_with_pacman() {
    if [ "$VERSION" = "latest" ]; then
        TAG="$(curl -fsSLI -o /dev/null -w '%{url_effective}' "https://github.com/$REPO/releases/latest")" \
            || die "Could not find the latest tql release."
        TAG="${TAG##*/}"
    else
        TAG="$VERSION"
    fi

    PKG="tql-${TAG#v}-1-$ARCH.pkg.tar.zst"
    BASE="https://github.com/$REPO/releases/download/$TAG"
    WORK="$(mktemp -d)"
    trap 'rm -rf "$WORK"' EXIT

    say "Downloading the tql $TAG package for pacman…"
    curl -fsSL "$BASE/$PKG" -o "$WORK/$PKG" 2>/dev/null || {
        say "$TAG has no pacman package, installing the binary instead."
        return 1
    }

    curl -fsSL "$BASE/SHA256SUMS" -o "$WORK/SHA256SUMS" || die "Could not download $BASE/SHA256SUMS"
    (cd "$WORK" && grep "  $PKG\$" SHA256SUMS | sha256sum --check --status) \
        || die "$PKG does not match the release's SHA256SUMS, so it was not installed."

    # An earlier run of this script put a loose binary first on PATH, where it
    # would hide the package's /usr/bin/tql.
    for OLD in /usr/local/bin/tql "$HOME/.local/bin/tql"; do
        if [ -f "$OLD" ] && "$OLD" --version 2>/dev/null | grep -q '^Tql v'; then
            say "Removing $OLD, which the package replaces."
            rm -f "$OLD" 2>/dev/null || $SUDO rm -f "$OLD"
        fi
    done

    # Piped into sh, pacman's questions would read the rest of this script, so
    # it asks on the terminal, or goes ahead without asking if there is none.
    if (: </dev/tty) 2>/dev/null; then
        $SUDO pacman -U "$WORK/$PKG" </dev/tty
    else
        $SUDO pacman -U --noconfirm "$WORK/$PKG" </dev/null
    fi

    say ""
    say "Installed $(tql --version 2>/dev/null || echo tql) with pacman. Run it with: tql"
    say "pacman -Syu will not see new releases; run this script again, or sudo pacman -R tql to remove it."
}

if [ "$OS" = "linux" ] && [ -z "${TQL_BIN_DIR:-}" ] && command -v pacman >/dev/null 2>&1; then
    install_with_pacman && exit 0
fi

# Somewhere on PATH that does not need a password, if there is one.
if [ -n "${TQL_BIN_DIR:-}" ]; then
    DIR="$TQL_BIN_DIR"
elif [ -w /usr/local/bin ]; then
    DIR="/usr/local/bin"
elif [ -d "$HOME/.local/bin" ]; then
    DIR="$HOME/.local/bin"
else
    DIR="/usr/local/bin"
fi

mkdir -p "$DIR" 2>/dev/null || true

if [ "$VERSION" = "latest" ]; then
    URL="https://github.com/$REPO/releases/latest/download/$ASSET"
else
    URL="https://github.com/$REPO/releases/download/$VERSION/$ASSET"
fi

TMP="$(mktemp)"
trap 'rm -f "$TMP"' EXIT

say "Downloading tql ($VERSION) for $OS ${ARCH}…"
curl -fsSL "$URL" -o "$TMP" || die "Could not download $URL"

chmod +x "$TMP"

if [ -w "$DIR" ]; then
    mv "$TMP" "$DIR/tql"
else
    say "Writing to $DIR needs sudo."
    sudo mv "$TMP" "$DIR/tql"
fi

trap - EXIT

say ""
say "Installed $("$DIR/tql" --version 2>/dev/null || echo tql) to $DIR/tql"

case ":$PATH:" in
    *":$DIR:"*) say "Run it with: tql" ;;
    *) say "$DIR is not on your PATH. Add it, or run $DIR/tql" ;;
esac
