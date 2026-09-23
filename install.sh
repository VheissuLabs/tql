#!/usr/bin/env sh
# Install tql, a database client for the terminal.
#
#   curl -fsSL https://raw.githubusercontent.com/VheissuLabs/tql/main/install.sh | sh
#
# Honours TQL_VERSION (a tag such as v0.3.0, default: the latest release) and
# TQL_BIN_DIR (where to put the binary).

set -eu

REPO="VheissuLabs/tql"
VERSION="${TQL_VERSION:-latest}"

say() { printf '%s\n' "$1"; }
die() { printf '\n%s\n' "$1" >&2; exit 1; }

command -v curl >/dev/null 2>&1 || die "tql needs curl to install."

# The binary is a phar: PHP runs it, so PHP has to be there and be new enough.
if command -v php >/dev/null 2>&1; then
    php -r 'exit(PHP_VERSION_ID >= 80400 ? 0 : 1);' 2>/dev/null \
        || die "tql needs PHP 8.4 or newer. You have $(php -r 'echo PHP_VERSION;')."
else
    die "tql needs PHP 8.4 or newer, and php is not on your PATH."
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
    URL="https://github.com/$REPO/releases/latest/download/tql"
else
    URL="https://github.com/$REPO/releases/download/$VERSION/tql"
fi

TMP="$(mktemp)"
trap 'rm -f "$TMP"' EXIT

say "Downloading tql ($VERSION)…"
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
