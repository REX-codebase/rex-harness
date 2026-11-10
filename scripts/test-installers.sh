#!/usr/bin/env bash
# Tests for the install scripts. No network, no cargo build, no root: the CLI
# installer runs against a fake release folder served over file://.
#
#   bash scripts/test-installers.sh
set -uo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
WORK="$(mktemp -d "${TMPDIR:-/tmp}/rex-installer-test.XXXXXX")"
trap 'rm -rf "$WORK"' EXIT
PASS=0
FAIL=0

ok() { PASS=$((PASS + 1)); echo "PASS  $1"; }
bad() { FAIL=$((FAIL + 1)); echo "FAIL  $1${2:+: $2}"; }
check() { # check NAME CONDITION-COMMAND...
  local name="$1"
  shift
  if "$@" >/dev/null 2>&1; then ok "$name"; else bad "$name"; fi
}
refused() { # refused NAME COMMAND... : must exit non-zero
  local name="$1"
  shift
  if "$@" >/dev/null 2>&1; then bad "$name" "succeeded but should have been refused"; else ok "$name"; fi
}

# --- syntax ---------------------------------------------------------------
for f in quickstart.sh scripts/rex-mcp-install.sh scripts/rex-mcp-uninstall.sh scripts/test-installers.sh; do
  check "bash parses $f" bash -n "$ROOT/$f"
done
check "install.sh parses as POSIX sh" sh -n "$ROOT/scripts/install.sh"
if command -v dash >/dev/null 2>&1; then
  check "install.sh runs under dash (no bashisms)" dash -n "$ROOT/scripts/install.sh"
fi
for f in quickstart.sh scripts/install.sh scripts/rex-mcp-install.sh scripts/rex-mcp-uninstall.sh; do
  check "$f is executable (a fresh clone can run it)" test -x "$ROOT/$f"
done
# What git will hand a fresh clone, not just this checkout.
if git -C "$ROOT" rev-parse --git-dir >/dev/null 2>&1; then
  for f in quickstart.sh scripts/install.sh scripts/rex-mcp-install.sh; do
    mode="$(git -C "$ROOT" ls-files -s -- "$f" | cut -d ' ' -f 1)"
    [ -z "$mode" ] || [ "$mode" = 100755 ] && ok "$f is committed executable (mode ${mode:-untracked})" || bad "$f is committed as mode $mode"
  done
fi

# --- quickstart.sh option handling ----------------------------------------
check "quickstart --help exits 0" bash "$ROOT/quickstart.sh" --help
refused "quickstart rejects an unknown option" bash "$ROOT/quickstart.sh" --bogus

# --- rex-mcp-install.sh store guard ----------------------------------------
STORE="$WORK/store"
mkdir -p "$STORE/tasks/a" "$STORE/tasks/b"
printf '{\n  "task_id": "a",\n  "store_schema_version": 2\n}\n' >"$STORE/tasks/a/task.json"
printf '{\n  "task_id": "b"\n}\n' >"$STORE/tasks/b/task.json"
check "store guard accepts schema v2 and a legacy record" env REX_STATE_DIR="$STORE" bash "$ROOT/scripts/rex-mcp-install.sh" --check-store
env REX_STATE_DIR="$STORE" bash "$ROOT/scripts/rex-mcp-install.sh" --check-store 2>&1 | grep -q "predate store schema v2" \
  && ok "store guard notes legacy records" || bad "store guard notes legacy records"
mkdir -p "$STORE/tasks/c"
printf '{\n  "task_id": "c",\n  "store_schema_version": 99\n}\n' >"$STORE/tasks/c/task.json"
refused "store guard refuses a newer pretty-printed store (the real on-disk format)" env REX_STATE_DIR="$STORE" bash "$ROOT/scripts/rex-mcp-install.sh" --check-store
printf '{"task_id":"c","store_schema_version":99}' >"$STORE/tasks/c/task.json"
refused "store guard refuses a newer compact store" env REX_STATE_DIR="$STORE" bash "$ROOT/scripts/rex-mcp-install.sh" --check-store
OUT="$(REX_INSTALL_DIR='/opt/my "rex" dir' REX_STATE_DIR='/data/rex state' bash "$ROOT/scripts/rex-mcp-install.sh" --print-config)"
echo "$OUT" | grep '^  {' | sed 's/^  //' | python3 -c "import sys,json; d=json.load(sys.stdin)['mcpServers']['rex']; assert d['command']=='/opt/my \"rex\" dir/rex-mcp' and d['env']['REX_STATE_DIR']=='/data/rex state'" \
  && ok "printed client config is valid JSON for paths with spaces and quotes" || bad "printed client config is valid JSON for paths with spaces and quotes"
echo "$OUT" | grep -q 'REX_STATE_DIR=/data/rex\\ state' \
  && ok "printed claude command quotes the state dir and uses REX_STATE_DIR" || bad "printed claude command quotes the state dir and uses REX_STATE_DIR"

# --- install.sh against a fake release ---------------------------------------
REL="$WORK/release"
mkdir -p "$REL" "$WORK/pkg"
printf '#!/bin/sh\necho "rex 9.9.9"\n' >"$WORK/pkg/rex"
chmod +x "$WORK/pkg/rex"
make_release() { # make_release TARGET
  (cd "$WORK/pkg" && tar -czf "$REL/rex-$1.tar.gz" rex)
  (cd "$REL" && sha256sum "rex-$1.tar.gz" >"rex-$1.tar.gz.sha256")
}
T=x86_64-unknown-linux-gnu
make_release "$T"
DEST="$WORK/bin"
run_install() { REX_TARGET="$T" REX_RELEASE_BASE_URL="file://$REL" REX_INSTALL_DIR="$DEST" PATH="$DEST:$PATH" sh "$ROOT/scripts/install.sh" "$@"; }

run_install >"$WORK/out.txt" 2>&1 && ok "install.sh installs from a release folder" || bad "install.sh installs from a release folder" "$(cat "$WORK/out.txt")"
check "the installed rex is executable and runs" test "$("$DEST/rex" --version 2>/dev/null)" = "rex 9.9.9"
grep -q "Checksum OK" "$WORK/out.txt" && ok "install.sh reports the verified checksum" || bad "install.sh reports the verified checksum"
check "no temporary file is left in the install folder" test -z "$(ls -A "$DEST" | grep -v '^rex$')"
run_install >/dev/null 2>&1 && ok "install.sh can be re-run over an existing install" || bad "install.sh can be re-run over an existing install"

# A tampered archive must be refused, and the working install must survive.
rm -rf "$DEST"
mkdir -p "$DEST"
printf '#!/bin/sh\necho "rex 1.0.0"\n' >"$DEST/rex"
chmod +x "$DEST/rex"
printf '#!/bin/sh\necho "evil"\n' >"$WORK/pkg/rex"
(cd "$WORK/pkg" && tar -czf "$REL/rex-$T.tar.gz" rex)   # checksum file now stale
refused "install.sh refuses an archive that does not match its checksum" run_install
check "a refused install leaves the existing rex untouched" test "$("$DEST/rex" --version)" = "rex 1.0.0"
REFUSAL="$(run_install 2>&1 || true)"   # captured first: the installer exits non-zero on purpose
case "$REFUSAL" in *"checksum mismatch"*) ok "the refusal says why" ;; *) bad "the refusal says why" "$REFUSAL" ;; esac

# Missing checksum file.
make_release "$T"
rm -f "$REL/rex-$T.tar.gz.sha256"
refused "install.sh refuses a release with no checksum file" run_install

# Platform detection, with a uname shim.
mkdir -p "$WORK/shim"
for combo in "Darwin x86_64" "Linux aarch64" "FreeBSD amd64"; do
  set -- $combo
  printf '#!/bin/sh\nif [ "$1" = "-s" ]; then echo %s; else echo %s; fi\n' "$1" "$2" >"$WORK/shim/uname"
  chmod +x "$WORK/shim/uname"
  out="$(env -u REX_TARGET PATH="$WORK/shim:$PATH" REX_RELEASE_BASE_URL="file://$REL" REX_INSTALL_DIR="$WORK/none" sh "$ROOT/scripts/install.sh" 2>&1)" && bad "unsupported platform $1/$2 is refused" \
    || { echo "$out" | grep -q "cargo install --path crates/rex-cli" && ok "unsupported platform $1/$2 is refused with the build instruction" || bad "unsupported platform $1/$2 is refused with the build instruction" "$out"; }
done
refused "install.sh rejects an unknown option" sh "$ROOT/scripts/install.sh" --nope

echo
echo "$PASS passed, $FAIL failed"
[ "$FAIL" -eq 0 ]
