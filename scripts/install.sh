#!/bin/sh
# noteboard installer
#
# Usage:
#   curl -fsSL https://raw.githubusercontent.com/conorbr/noteboard/main/scripts/install.sh | sh
#   curl -fsSL .../install.sh | NOTEBOARD_VERSION=0.4.0 sh
#   curl -fsSL .../install.sh | NOTEBOARD_INSTALL_DIR=/usr/local/bin sh
#
# What it does, in full:
#
#   detects    your OS (macOS or Linux) and CPU (x86_64 or arm64)
#   finds      the latest release on github.com/conorbr/noteboard/releases,
#              or the one in NOTEBOARD_VERSION
#   downloads  the matching archive and its .sha256, over anonymous HTTPS
#   verifies   the checksum; a mismatch aborts before anything is written
#   writes     one file, noteboard, into ~/.local/bin (or NOTEBOARD_INSTALL_DIR)
#
# No root, no edits to your shell config. To uninstall, delete that one file.

set -eu

REPO="conorbr/noteboard"
RELEASES="${NOTEBOARD_RELEASES_URL:-https://github.com/$REPO/releases}"
INSTALL_DIR="${NOTEBOARD_INSTALL_DIR:-$HOME/.local/bin}"

say() { printf 'noteboard-install: %s\n' "$*"; }
fail() {
  printf 'noteboard-install: error: %s\n' "$*" >&2
  exit 1
}
need() { command -v "$1" >/dev/null 2>&1 || fail "needs $1"; }

need curl
need tar
need uname
need mktemp

from_source="With Rust: cargo install --git https://github.com/$REPO noteboard"
case "$(uname -s)" in
  Darwin) os="apple-darwin" ;;
  Linux) os="unknown-linux-musl" ;;
  *) fail "no prebuilt noteboard for $(uname -s). $from_source" ;;
esac
case "$(uname -m)" in
  x86_64 | amd64) arch="x86_64" ;;
  arm64 | aarch64) arch="aarch64" ;;
  *) fail "no prebuilt noteboard for $(uname -m). $from_source" ;;
esac
target="$arch-$os"

if [ -n "${NOTEBOARD_VERSION:-}" ]; then
  version="${NOTEBOARD_VERSION#v}"
else
  # /releases/latest redirects to /releases/tag/vX.Y.Z. No API, no rate limit.
  latest=$(curl -fsSLI -o /dev/null -w '%{url_effective}' "$RELEASES/latest") ||
    fail "couldn't reach $RELEASES"
  version="${latest##*/tag/v}"
  case "$version" in
    "" | */*) fail "couldn't work out the latest version from $latest" ;;
  esac
fi

name="noteboard-v$version-$target"
url="$RELEASES/download/v$version/$name.tar.gz"

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
trap 'exit 1' INT TERM

say "downloading noteboard $version for $target"
curl -fsSL "$url" -o "$tmp/$name.tar.gz" || fail "download failed: $url"
curl -fsSL "$url.sha256" -o "$tmp/$name.tar.gz.sha256" || fail "download failed: $url.sha256"

if command -v sha256sum >/dev/null 2>&1; then
  check="sha256sum -c"
elif command -v shasum >/dev/null 2>&1; then
  check="shasum -a 256 -c"
else
  fail "needs sha256sum or shasum to verify the download"
fi
(cd "$tmp" && $check "$name.tar.gz.sha256" >/dev/null 2>&1) ||
  fail "checksum mismatch for $name.tar.gz. Nothing was installed"

tar -xzf "$tmp/$name.tar.gz" -C "$tmp"
mkdir -p "$INSTALL_DIR"
# Copy beside the target, then rename, so a running noteboard is never half-written.
cp "$tmp/$name/noteboard" "$INSTALL_DIR/.noteboard.new"
chmod 755 "$INSTALL_DIR/.noteboard.new"
mv -f "$INSTALL_DIR/.noteboard.new" "$INSTALL_DIR/noteboard"

say "installed $("$INSTALL_DIR/noteboard" --version) to $INSTALL_DIR/noteboard"

case ":$PATH:" in
  *":$INSTALL_DIR:"*)
    found=$(command -v noteboard || true)
    if [ -n "$found" ] && [ "$found" != "$INSTALL_DIR/noteboard" ]; then
      say "note: $found comes first on your PATH, so that's the noteboard you'll run"
    fi
    ;;
  *)
    say "$INSTALL_DIR isn't on your PATH. Add this to your shell's startup file:"
    say "  export PATH=\"$INSTALL_DIR:\$PATH\""
    ;;
esac
say "get started: cd into a folder of Markdown files and run: noteboard"
