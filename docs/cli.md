# The `rex` command line

`rex` runs the same agent loop as the desktop app without a window: from a
terminal, a script, CI, or an editor. It prints one JSON receipt per run when
asked, never approves anything unless told to, and exits with a code that says
what happened.

```
rex <command> [options]
rex --version
rex --help
```

Install it with `cargo install --path crates/rex-cli` from a clone, or build it
with `cargo build --release -p rex-cli` (the binary is `target/release/rex`).
`./quickstart.sh` installs it alongside the desktop app.

## Contents

- [Quick examples](#quick-examples)
- [Exit codes](#exit-codes)
- [Credentials and where data lives](#credentials-and-where-data-lives)
- [Environment variables](#environment-variables)
- [Running tasks](#running-tasks): `exec`, `resume`, `compact`, `clear`
- [Signing in with ChatGPT](#signing-in-with-chatgpt): `login`, `logout`, `auth`
- [Comparing and attacking results](#comparing-and-attacking-results): `tournament`, `redteam`
- [Receipts and history](#receipts-and-history): `runs`, `show`, `verify`, `replay`, `ps`, `keygen`
- [Contracts, skills and tools](#contracts-skills-and-tools): `policy`, `skill`, `mcp`
- [Editor and automation](#editor-and-automation): `serve`, `checkin`, `config`
- [Run options](#run-options)

## Quick examples

```bash
# Run a task headlessly and get one JSON receipt on stdout (logs go to stderr).
rex exec --task "add a --verbose flag to the CLI and prove it works" \
         --workspace . --yes --json

# See what a run would be allowed to spend before it starts; run it only if you agree.
rex exec --task "refactor the parser" --workspace . --model claude-sonnet-5-5 --bid
rex exec --task "refactor the parser" --workspace . --model claude-sonnet-5-5 --bid --accept-bid

# Keep a conversation going across runs.
rex exec --task "write the migration" --name migration --workspace .
rex resume migration --task "now add a rollback"

# Use your ChatGPT plan instead of an API key.
rex login chatgpt
rex exec --provider openai-chatgpt --task "explain this repository" --workspace .
```

`rex exec` works on a **disposable copy** of `--workspace` and never edits the
original. Nothing outside the copy is touched unless you promote the result.

## Exit codes

| Code | Meaning |
| --- | --- |
| `0` | The run completed. `rex --help` (or `-h`, or `--help` after a command) also exits `0` and prints its usage on stdout. |
| `1` | Internal error (a bug or an unreadable state folder). |
| `2` | Usage or configuration problem, or an approval is needed and `--yes` was not given. |
| `3` | The run ended without completing (budget reached, cancelled, blocked, failed gates, provider error). |

Scripts should branch on the code and parse the receipt for the reason.

## Credentials and where data lives

Credentials are looked up in this order:

1. `REX_<PROVIDER>_API_KEY` (provider id upper-cased, `-` becomes `_`, for
   example `REX_ANTHROPIC_API_KEY`, `REX_KIMI_CODING_API_KEY`);
2. `REX_API_KEY`, a generic fallback so CI can bind one secret without
   interpolating it into shell text;
3. the local credential store (what the desktop app and the model menu's
   *Connect API* write).

The ChatGPT sign-in is **not** a key: it is only ever the sign-in that
`rex login chatgpt` (or the desktop app) created, and no environment variable
stands in for it.

Everything REX remembers is on this computer: saved keys, the sign-in, the run
ledger, sessions, signing key, settings. There is no REX server. The folder is
`%LOCALAPPDATA%\rex-harness` on Windows and `~/.rex/harness` on macOS and Linux
(the desktop app uses `~/.config/rex-harness` there), or whatever
`REX_STATE_DIR` points at. Point both at one folder to share a sign-in and run
history between the app and the CLI. See [data-and-storage.md](data-and-storage.md).

## Environment variables

| Variable | Effect |
| --- | --- |
| `REX_STATE_DIR` | Where REX keeps its local data (see above). |
| `REX_<PROVIDER>_API_KEY`, `REX_API_KEY` | API key for a provider, or for any provider. |
| `REX_PROVIDER` | Default provider for `exec` and friends (default `anthropic`). |
| `REX_MODEL` | Default model id (default: the provider's default). |
| `REX_CONTEXT_WINDOW` | The model's window in tokens, for [windowed context](context-management.md). |
| `REX_WORKSPACE`, `REX_APPROVE_TASK_MUTATIONS` | Read by the `rex-mcp` server (not by `rex`): the workspace it works in, and (`1`) whether the host may change the task's state. Off by default. See [rex-mcp.md](rex-mcp.md). |
| `REX_ULTRA_JUDGE_PROVIDER`, `REX_ULTRA_JUDGE_MODEL`, `REX_ULTRA_ADVERSARY_PROVIDER`, `REX_ULTRA_ADVERSARY_MODEL` | Which models act as judge and adversary in [Ultra mode](ultra-mode.md). |

Provider ids: `gemini`, `anthropic`, `openai`, `openai-chatgpt`, `xai`,
`deepseek`, `kimi`, `kimi-coding`, `qwen`, `glm`, `nvidia`, `groq`,
`openrouter`, `local`, `custom`.

## Running tasks

### `rex exec`

```
rex exec [--task TASK | TASK...] [run options]
```

Runs one task to a terminal state. Approvals for tool calls and plans are
**never** granted silently: pass `--yes` to auto-approve (CI mode), or run the
interactive [desktop app](getting-started.md) / [`rex serve`](#rex-serve) to
decide each one yourself.

With `--json` stdout carries exactly one receipt (schema `rex.exec.receipt/1`)
and nothing else. It includes the run id, task, provider and model, terminal
status and reason, steps / tool calls / tokens used against their budgets,
elapsed time, the result summary, the prompt version and hash, per-file
provenance (which step wrote which hunks), any accepted bid and its outcome,
the evaluated policy, skills loaded, and an Ed25519 certificate over all of it.
Verify it with [`rex verify`](#rex-verify-and-rex-replay).

`--dry-run` validates the arguments and prints the effective configuration
without running anything.

### `rex resume RUN_ID|NAME`

Continues an earlier run (or a named session) as a new run linked to the
previous one. Accepts `--task`, `--name`, `--provider`, `--model`, the budget
flags, `--skill`, `--yes` and `--json`.

### `rex compact NAME` and `rex clear NAME`

A named session accumulates runs. `compact` collapses the chain into one
mechanical summary that the next `resume` injects as context; `clear` drops it.

## Signing in with ChatGPT

OpenAI offers ChatGPT plan usage to open-source, locally hosted apps; REX is
both. See [chatgpt-sign-in.md](chatgpt-sign-in.md) for how it works and what it
will not do.

```
rex login chatgpt [--no-browser] [--force-consent] [--new-account] [--json]
rex logout chatgpt [--json]
rex auth status [--json]
```

- `login` opens your browser to OpenAI, waits up to five minutes for you to
  finish, and saves the sign-in on this computer. `--no-browser` prints a
  one-time link instead of opening it. `--force-consent` asks OpenAI to show the
  permission screen again (use it only to turn plan usage on after declining it).
  `--new-account` signs in as a **different** ChatGPT account than the one saved
  here (an issued client is bound to the account that registered it, so the saved
  one cannot be reused). Sign out first; it is refused while you are signed in.
- `logout` revokes the renewable session at OpenAI, then clears it locally.
- `auth status` says whether you are signed in, as whom, and whether plan usage
  is on. It never prints a token.

## Comparing and attacking results

- `rex tournament --task TASK --providers a,b [options]` runs the same task once
  per provider, with the same budgets and workspace copy, and promotes one winner
  by a fixed rule: completed beats everything, then fewest tool calls, then fewest
  tokens. Every contestant receipt and the tournament receipt are signed.
- `rex redteam --task TASK --yes [options]` runs the task once (the builder),
  then a second agent whose only job is to attack the result. The verdict is
  mechanical (`SOUND`, `BROKEN: reason`, or `inconclusive`) and signed.

## Receipts and history

| Command | What it does |
| --- | --- |
| `rex runs [--json] [--limit N]` | Lists receipts in the local ledger (`runs.jsonl`), newest first. |
| `rex show RUN_ID [--json]` | Shows one ledger entry. |
| `rex ps [--all] [--json] [--prune] [--limit N]` | Shows runs in flight (a stale heartbeat is shown as stale, never counted as live); `--prune` removes dead markers. |
| `rex verify RECEIPT.json [--public-key BASE64]` | Checks a receipt's certificate offline. No REX install on the signing machine is needed. |
| `rex replay RECEIPT.json [--json]` | Re-derives every mechanical claim in a receipt (signature, bid terms and ceiling, policy, provenance shape) from the evidence it carries. |
| `rex keygen [--force]` | Creates or rotates this machine's signing key. Old receipts stay valid: each names its signer by public key. |

### `rex verify` and `rex replay`

`verify` answers "is this exactly what the machine that ran it produced?".
`replay` answers "does every claim in it follow from its own evidence?". Neither
needs the original workspace.

## Contracts, skills and tools

- `rex policy --workspace DIR` evaluates a repository's `.rex/policy.json` proof
  contract. When `exec` runs with `--workspace`, the contract is loaded first and
  enforced: a run either satisfies it or does not start. A malformed contract
  blocks the run.
- `rex skill install DIR [--force] | list | show NAME | verify [NAME] | remove NAME | pack DIR`
  manages skill packs: a directory with a `skill.json` manifest listing the hash
  of every file. Install verifies every hash before copying anything and writes
  a lockfile; `verify` detects tampering afterwards. `rex exec --skill NAME`
  loads an installed skill into a run, and the receipt records it.
- `rex mcp list | tools [SERVER] | call SERVER TOOL [--args JSON]` talks to
  external MCP servers over stdio, configured in `.rex/mcp.json` in the workspace
  or `mcp.json` in the data folder. Values in `env` there are stored in clear
  text; do not put secrets you would not put in a config file.

## Editor and automation

### `rex serve`

`rex serve [--port N]` binds `127.0.0.1` only (never `0.0.0.0`) and exposes a
small HTTP API for IDE extensions. The first line on stdout is `{"port": N}` so
an extension can discover the port; everything else goes to stderr. There is no
authentication: the loopback interface is the trust boundary.

| Endpoint | Purpose |
| --- | --- |
| `POST /v1/runs` | Start a run (`task`, optional `provider`, `model`, `skills`). Runs are interactive: approvals park until decided. |
| `GET /v1/runs`, `GET /v1/runs/:id` | List runs; live snapshot with any pending approval or plan. |
| `POST /v1/runs/:id/approve` | Decide a pending approval or plan. |
| `POST /v1/runs/:id/cancel` | Cancel a run. |
| `POST /v1/runs/:id/checkpoint`, `GET /v1/runs/:id/checkpoints`, `POST /v1/runs/:id/rewind` | Snapshot the workspace copy and rewind to a snapshot. |

The VS Code extension in `extensions/vscode` is built on it, and
`actions/rex-run` is a GitHub Action that runs `rex exec --json` and uploads the
receipt.

### `rex checkin` and the dead-man switch

`rex exec --deadman-mins N` arms a timer. The operator must `touch` the check-in
file (or run `rex checkin --file PATH`) at least every N minutes, or the run is
cancelled at the next poll and the receipt records it. It is a liveness signal
("a human is still watching"), separate from `--timeout-secs`, which is a budget.

### `rex config`

`rex config [get KEY | set KEY VALUE]` reads and writes small operator settings
in `config.json`. Today there is one: `max_parallel_runs` (default 2). Background
agents multiply cost and load silently, so the default is low and raising it is
deliberate.

## Run options

These apply to `exec`, and (where noted) to `resume`, `tournament` and `redteam`.

| Option | Meaning |
| --- | --- |
| `--task TEXT` | The task (or pass it as trailing words). |
| `--provider NAME` | Provider id (default `$REX_PROVIDER` or `anthropic`). |
| `--model ID` | Model id (default `$REX_MODEL` or the provider's default). |
| `--workspace DIR` | Directory copied into the run's sandbox. |
| `--name NAME` | Session label (letters, digits, `-`, `_`; at most 64). Shown by `rex runs` and usable by `rex resume`. |
| `--max-steps N`, `--max-tool-calls N`, `--max-tokens N`, `--timeout-secs N` | Budgets. The run stops, and says why, when one is reached. |
| `--budget-usd X` | Dollar cap, converted worst-case into a token budget. Needs `--model` with a known price. |
| `--bid`, `--accept-bid` | Print the binding bid and stop; run under it only with `--accept-bid`. |
| `--windowed-context` | Keep a managed transcript for large-window models ([details](context-management.md)). |
| `--context-window N` | The model's window in tokens (or `$REX_CONTEXT_WINDOW`). |
| `--skill NAME` | Load an installed skill (repeatable). |
| `--yes` | Auto-approve tool calls and plans (CI mode). |
| `--json` | Print exactly one JSON receipt on stdout. |
| `--dry-run` | Validate and print the configuration without running. |
| `--detach` | Run in the background; prints the run id. Watch it with `rex ps`. |
| `--deadman-mins N`, `--deadman-file PATH` | Dead-man switch (above). |
