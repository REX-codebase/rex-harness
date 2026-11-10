#!/usr/bin/env sh
# Install the `rex` command line from a GitHub release: a download, not a build.
#
#   curl -fsSL https://raw.githubusercontent.com/REX-codebase/rex-harness/main/scripts/install.sh | sh
#
# The archive is checked against the SHA-256 published next to it before
# anything is installed. Nothing is run as root and nothing outside the install
# folder is touched.
#
# Environment:
#   REX_VERSION           a tag such as v0.1.2 (default: the latest release)
#   REX_INSTALL_DIR       where to put `rex` (default: ~/.local/bin)
#   REX_TARGET            override the detected platform, for example
#                         x86_64-unknown-linux-gnu
#   REX_RELEASE_BASE_URL  download from here instead of GitHub (a mirror, or a
#                         file:// folder for testing); expects rex-<target>.tar.gz
#                         and rex-<target>.tar.gz.sha256 inside
#
# Platforms with a prebuilt CLI: Linux x86_64 and macOS Apple Silicon. On
# Windows download rex-x86_64-pc-windows-msvc.zip from the Releases page. Any
# other platform: cargo install --path crates/rex-cli
set -eu

REPO="REX-codebase/rex-harness"
INSTALL_DIR="${REX_INSTALL_DIR:-$HOME/.local/bin}"

say() { printf '%s\n' "$*"; }
die() { printf 'install.sh: %s\n' "$*" >&2; exit 1; }

case "${1:-}" in
  -h | --help)
    # Piped from curl, $0 is the shell, not this file.
    if [ -f "$0" ]; then
      sed -n '2,23p' "$0" | sed 's/^# \{0,1\}//'
    else
      say "Installs the rex command line from a GitHub release. See scripts/install.sh in the repository."
    fi
    exit 0
    ;;
  "") ;;
  *) die "unknown option: $1 (try --help)" ;;
esac

detect_target() {
  os="$(uname -s)"
  arch="$(uname -m)"
  case "$os/$arch" in
    Linux/x86_64 | Linux/amd64) echo x86_64-unknown-linux-gnu ;;
    Darwin/arm64 | Darwin/aarch64) echo aarch64-apple-darwin ;;
    Darwin/x86_64)
      die "there is no prebuilt rex for Intel Macs yet. Build it: cargo install --path crates/rex-cli (needs Rust, https://rustup.rs)"
      ;;
    MINGW* | MSYS* | CYGWIN*)
      die "on Windows download rex-x86_64-pc-windows-msvc.zip from https://github.com/$REPO/releases/latest"
      ;;
    *)
      die "there is no prebuilt rex for $os/$arch. Build it: cargo install --path crates/rex-cli (needs Rust, https://rustup.rs)"
      ;;
  esac
}

TARGET="${REX_TARGET:-$(detect_target)}"
ARCHIVE="rex-$TARGET.tar.gz"

if [ -n "${REX_RELEASE_BASE_URL:-}" ]; then
  BASE="${REX_RELEASE_BASE_URL%/}"
elif [ -n "${REX_VERSION:-}" ]; then
  BASE="https://github.com/$REPO/releases/download/$REX_VERSION"
else
  BASE="https://github.com/$REPO/releases/latest/download"
fi

fetch() { # fetch URL DEST
  if command -v curl >/dev/null 2>&1; then
    curl -fsSL --retry 3 -o "$2" "$1"
  elif command -v wget >/dev/null 2>&1; then
    wget -q -O "$2" "$1"
  else
    die "curl or wget is required"
  fi
}

sha256_of() {
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$1" | cut -d ' ' -f 1
  elif command -v shasum >/dev/null 2>&1; then
    shasum -a 256 "$1" | cut -d ' ' -f 1
  else
    die "sha256sum or shasum is required to verify the download"
  fi
}

command -v tar >/dev/null 2>&1 || die "tar is required"

TMP="$(mktemp -d "${TMPDIR:-/tmp}/rex-install.XXXXXX")"
trap 'rm -rf "$TMP"' EXIT INT TERM

say "Downloading $ARCHIVE from $BASE"
fetch "$BASE/$ARCHIVE" "$TMP/$ARCHIVE" || die "could not download $BASE/$ARCHIVE (does that release have a CLI for $TARGET?)"
fetch "$BASE/$ARCHIVE.sha256" "$TMP/$ARCHIVE.sha256" || die "could not download the checksum $BASE/$ARCHIVE.sha256"

want="$(cut -d ' ' -f 1 <"$TMP/$ARCHIVE.sha256" | tr 'A-F' 'a-f')"
got="$(sha256_of "$TMP/$ARCHIVE")"
[ -n "$want" ] || die "the checksum file is empty"
[ "$want" = "$got" ] || die "checksum mismatch for $ARCHIVE (expected $want, got $got). Nothing was installed."
say "Checksum OK ($got)"

tar -xzf "$TMP/$ARCHIVE" -C "$TMP" rex || die "the archive does not contain a rex binary"
[ -f "$TMP/rex" ] || die "the archive does not contain a rex binary"

mkdir -p "$INSTALL_DIR"
# Copy to a temporary name and rename: the install is all-or-nothing, and
# replacing a running rex does not fail with "text file busy".
cp "$TMP/rex" "$INSTALL_DIR/.rex.new"
chmod 755 "$INSTALL_DIR/.rex.new"
mv -f "$INSTALL_DIR/.rex.new" "$INSTALL_DIR/rex"

say "Installed $("$INSTALL_DIR/rex" --version) to $INSTALL_DIR/rex"
case ":$PATH:" in
  *":$INSTALL_DIR:"*) ;;
  *)
    say ""
    say "$INSTALL_DIR is not on your PATH. Add it (and put this line in your shell profile):"
    say "  export PATH=\"$INSTALL_DIR:\$PATH\""
    ;;
esac
say ""
say "Next: rex login chatgpt   (or set REX_ANTHROPIC_API_KEY, REX_OPENAI_API_KEY, ...)"
say "      rex exec --dry-run --task \"say hello\" --json"
