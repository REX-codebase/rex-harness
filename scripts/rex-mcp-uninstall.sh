#!/usr/bin/env bash
# Remove the rex-mcp binary. Client config and state are yours to remove:
# `claude mcp remove rex`, delete the rex entry from Antigravity's
# mcp_config.json, then optionally `rm -rf ~/.rex/harness` (irreversible:
# that directory is the only durable record of tasks, custody and audits).
set -euo pipefail
DEST="${REX_INSTALL_DIR:-$HOME/.rex/bin}"
removed=0
for name in rex-mcp rex-mcp.exe; do
  if [ -e "$DEST/$name" ]; then
    rm -f "$DEST/$name"
    echo "Removed $DEST/$name"
    removed=1
  fi
done
[ "$removed" = 1 ] || echo "Nothing to remove: no rex-mcp in $DEST."
echo "State kept at ${REX_STATE_DIR:-$HOME/.rex/harness} (remove manually if unwanted)."
