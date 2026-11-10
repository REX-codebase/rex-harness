#!/usr/bin/env bash
# REX Harness quickstart: one command to go from clone to running app.
#
# Usage: ./quickstart.sh [--build | --no-app | --help]
#
#   (no option)  install everything, then start the app in dev mode
#   --build      install everything, then build the production app
#   --no-app     install the frontend dependencies and the `rex` CLI only
#
# What it does:
#   1. Checks for Node.js, npm, Rust and the system libraries Tauri needs
#   2. Installs frontend dependencies (npm ci, from the lockfile)
#   3. Installs the headless `rex` CLI (rex exec --json for CI)
#   4. Starts the Tauri dev app, or builds it
#
# On Windows, the setup wizard from the Releases page is the easier route.

set -euo pipefail

cd "$(dirname "$0")"

info() { echo "→ $1"; }
ok() { echo "✓ $1"; }
warn() { echo "! $1" >&2; }
fail() { echo "✗ $1" >&2; exit 1; }

MODE=dev
case "${1:-}" in
  "") ;;
  --build) MODE=build ;;
  --no-app) MODE=cli ;;
  -h | --help)
    sed -n '2,16p' "$0" | sed 's/^# \{0,1\}//'
    exit 0
    ;;
  *) fail "unknown option: $1 (try --help)" ;;
esac

OS="$(uname -s 2>/dev/null || echo unknown)"
case "$OS" in
  MINGW* | MSYS* | CYGWIN*)
    warn "Windows detected. The setup wizard on the Releases page is the easier way to install:"
    warn "  https://github.com/REX-codebase/rex-harness/releases/latest"
    ;;
esac

# 1. Check prerequisites.
info "Checking prerequisites…"
command -v node >/dev/null || fail "Node.js not found. Install it from https://nodejs.org (v20 or newer)."
command -v npm >/dev/null || fail "npm not found. Install Node.js first."
command -v cargo >/dev/null || fail "Rust not found. Install it from https://rustup.rs"

NODE_MAJOR=$(node -p "process.versions.node.split('.')[0]")
[ "$NODE_MAJOR" -ge 20 ] || fail "Node.js v20 or newer is required (found v$(node -p process.versions.node))."

# The desktop app needs system libraries; the CLI alone does not.
if [ "$MODE" != "cli" ]; then
  case "$OS" in
    Linux)
      # Ask pkg-config rather than a package manager, so this is right on any
      # distribution. The names below are the Debian/Ubuntu ones that CI uses.
      missing=0
      if command -v pkg-config >/dev/null 2>&1; then
        for lib in webkit2gtk-4.1 ayatana-appindicator3-0.1 librsvg-2.0 openssl; do
          pkg-config --exists "$lib" 2>/dev/null || missing=1
        done
      else
        missing=1
      fi
      if [ "$missing" = 1 ]; then
        warn "Some libraries Tauri needs were not found. On Debian/Ubuntu:"
        warn "  sudo apt install libwebkit2gtk-4.1-dev build-essential curl wget file libxdo-dev \\"
        warn "    libssl-dev libayatana-appindicator3-dev librsvg2-dev pkg-config"
        warn "Other distributions: https://v2.tauri.app/start/prerequisites/#linux"
      fi
      ;;
    Darwin)
      xcode-select -p >/dev/null 2>&1 \
        || warn "Xcode command line tools not found; the app build needs them: xcode-select --install"
      ;;
  esac
fi
ok "Prerequisites look good."

# 2. Install frontend deps. `npm ci` installs exactly what package-lock.json
# says; fall back to `npm install` only when the lockfile is out of step.
if [ ! -d node_modules ]; then
  info "Installing frontend dependencies…"
  npm ci --no-audit --no-fund || npm install --no-audit --no-fund
else
  info "Frontend dependencies already installed."
fi
ok "Dependencies ready."

# 3. Install the headless CLI. Quiet on success; the full build output is
# shown only if it fails (it used to be hidden and then run a second time).
info "Installing the rex CLI (this compiles it, so the first run takes a few minutes)…"
LOG="$(mktemp "${TMPDIR:-/tmp}/rex-install.XXXXXX")"
if ! cargo install --path crates/rex-cli --locked >"$LOG" 2>&1; then
  tail -n 40 "$LOG" >&2
  fail "cargo install of the rex CLI failed (full log: $LOG)."
fi
rm -f "$LOG"
CARGO_BIN="${CARGO_HOME:-$HOME/.cargo}/bin"
if command -v rex >/dev/null 2>&1; then
  REX_BIN="$(command -v rex)"
else
  REX_BIN="$CARGO_BIN/rex"
fi
[ -x "$REX_BIN" ] || [ -x "$REX_BIN.exe" ] || fail "rex was installed but was not found in $CARGO_BIN."
ok "rex CLI installed ($("$REX_BIN" --version))."
if ! command -v rex >/dev/null 2>&1; then
  warn "$CARGO_BIN is not on your PATH, so \`rex\` will not be found by name yet. Add it:"
  warn "  export PATH=\"$CARGO_BIN:\$PATH\"   # put this in your shell profile"
fi
info "Try: rex exec --dry-run --task \"say hello\" --json"

# 4. Build or dev.
case "$MODE" in
  cli)
    ok "Done. Run ./quickstart.sh again without --no-app to start the desktop app."
    ;;
  build)
    info "Building production app…"
    npm run tauri build
    ok "Build complete. See target/release/bundle/ (or src-tauri/target/release/bundle/)."
    ;;
  dev)
    info "Starting REX Harness in dev mode…"
    info "Press Ctrl+C to stop."
    npm run tauri dev
    ;;
esac
