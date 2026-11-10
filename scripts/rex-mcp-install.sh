#!/usr/bin/env bash
# Install the REX MCP server locally. No daemons, no telemetry, no network
# (beyond what `cargo build` needs to fetch crates).
#
#   scripts/rex-mcp-install.sh                 build and install rex-mcp
#   scripts/rex-mcp-install.sh --check-store   only check the task store is safe to use
#   scripts/rex-mcp-install.sh --print-config  only print the client setup snippets
#
# Environment: REX_INSTALL_DIR (default ~/.rex/bin), REX_STATE_DIR (default
# ~/.rex/harness), CARGO_TARGET_DIR.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
DEST="${REX_INSTALL_DIR:-$HOME/.rex/bin}"
STATE_DIR="${REX_STATE_DIR:-$HOME/.rex/harness}"
# The task-store schema this build reads and writes (STORE_SCHEMA_VERSION in
# crates/rex-daemon/src/lib.rs).
SUPPORTED_SCHEMA=2

case "$(uname -s 2>/dev/null || echo unknown)" in
  MINGW* | MSYS* | CYGWIN*) EXE=".exe" ;;
  *) EXE="" ;;
esac
BIN="rex-mcp$EXE"

# Store-schema guard: never downgrade into a newer task store; notice legacy.
# task.json is written pretty-printed (`"store_schema_version": 2`, with a
# space), so the match must allow whitespace around the colon. It used to
# demand none, never matched a real file, and so never stopped anything.
check_store() {
  [ -d "$STATE_DIR/tasks" ] || return 0
  local f v legacy=0
  for f in "$STATE_DIR"/tasks/*/task.json; do
    [ -e "$f" ] || continue
    v=$(grep -o '"store_schema_version"[[:space:]]*:[[:space:]]*[0-9][0-9]*' "$f" | head -1 | grep -o '[0-9][0-9]*$' || true)
    if [ -z "$v" ]; then
      legacy=$((legacy + 1))
    elif [ "$v" -gt "$SUPPORTED_SCHEMA" ]; then
      echo "Task store at $STATE_DIR was written by a newer REX (schema v$v)." >&2
      echo "Install the newer server instead of downgrading onto it." >&2
      return 1
    fi
  done
  if [ "$legacy" -gt 0 ]; then
    echo "Note: $legacy task record(s) predate store schema v2 and migrate to"
    echo "Standard mode on first load by the new server."
  fi
}

# JSON string escaping for the one value that can hold odd characters (a path).
json_escape() {
  printf '%s' "$1" | sed -e 's/\\/\\\\/g' -e 's/"/\\"/g'
}

print_config() {
  local bin="$DEST/$BIN"
  echo "Claude Code:"
  printf '  claude mcp add rex --scope user \\\n'
  printf '    --env %q \\\n' "REX_STATE_DIR=$STATE_DIR"
  printf '    --env %q \\\n' "REX_WORKSPACE=/path/to/project"
  printf '    --env REX_APPROVE_TASK_MUTATIONS=1 \\\n'
  printf '    -- %q\n' "$bin"
  echo
  echo "Antigravity (~/.antigravity/mcp_config.json):"
  printf '  {"mcpServers":{"rex":{"command":"%s","env":{"REX_STATE_DIR":"%s","REX_WORKSPACE":"/path/to/project","REX_APPROVE_TASK_MUTATIONS":"1"}}}}\n' \
    "$(json_escape "$bin")" "$(json_escape "$STATE_DIR")"
  echo
  echo "Read docs/rex-mcp.md for the security and error contracts before"
  echo "enabling REX_APPROVE_TASK_MUTATIONS=1."
}

case "${1:-}" in
  "") ;;
  --check-store)
    check_store
    exit $?
    ;;
  --print-config)
    print_config
    exit 0
    ;;
  -h | --help)
    sed -n '2,10p' "$0" | sed 's/^# \{0,1\}//'
    exit 0
    ;;
  *)
    echo "unknown option: $1 (try --help)" >&2
    exit 2
    ;;
esac

if ! command -v cargo >/dev/null 2>&1; then
  echo "Rust is required to build rex-mcp. Install it from https://rustup.rs and try again." >&2
  exit 1
fi

check_store

echo "Building rex-mcp (release)..."
cargo build --release --locked -p rex-mcp --manifest-path "$ROOT/Cargo.toml"
mkdir -p "$DEST" "$STATE_DIR"
TARGET_DIR="${CARGO_TARGET_DIR:-$ROOT/target}"
cp "$TARGET_DIR/release/$BIN" "$DEST/$BIN"
chmod +x "$DEST/$BIN"
echo "Installed: $DEST/$BIN"
echo
print_config
