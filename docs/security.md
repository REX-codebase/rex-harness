# Security model

REX runs an AI model against your code and your machine. This page says what
REX does to keep that safe, and, as important, what it does not. For reporting a
vulnerability see [SECURITY.md](../SECURITY.md).

## Principles

1. **The model proposes; you and the harness decide.** A model never receives a
   raw file or process handle. It asks for a tool call; REX validates it, shows
   it to you, and runs it only on approval.
2. **Credentials stay in the backend.** The interface never holds a key or token.
3. **Claims are mechanical.** A run completes only if its evidence gates pass; a
   receipt records what happened and is signed.
4. **Local first.** There is no REX server. See [data-and-storage.md](data-and-storage.md).

## Tools and approvals

Defined in [local-tools.md](local-tools.md) and enforced in `crates/rex-tools`:

- One explicit **workspace root**, canonicalised. Relative paths only: `..`,
  absolute paths, canonical escapes and symlink components are refused.
- Reads are bounded (2 MiB). **Every write and every command needs a visible
  approval** bound to the stored request, not to a flag the model can set.
  Edits are exact-match and fail on stale or ambiguous content.
- Commands are **argv, never a shell string**. Shells, interpreters, network
  clients, privilege tools, destructive file and process commands, and command
  chaining are denied even after approval.
- Commands run in the workspace with a cleared environment (only `PATH`, `HOME`
  and `REX_WORKSPACE`), a 120-second ceiling, a 512 KiB output ceiling, and
  termination of the process group on timeout. Common API keys, bearer tokens
  and secret assignments are redacted from output before the model sees it.
- Agents work on a **disposable copy** of your workspace. Changes are promoted
  only after review.

`--yes` turns approvals off for a run. Use it only in a sandbox or CI you
control; it is the single flag that lets a model act without you.

## Custody, budgets and proof

- Every run has hard budgets (steps, tool calls, tokens, wall time) that survive
  a crash. A binding cost bid (`--bid`) can be printed before anything runs.
- A dead-man switch (`--deadman-mins`) cancels a run nobody is watching.
- A repository can declare a proof contract (`.rex/policy.json`); a run either
  satisfies it or does not start, and a malformed contract blocks the run.
- Every receipt carries an Ed25519 certificate from this machine's key;
  `rex verify` and `rex replay` check it offline. See [cli.md](cli.md).

## Credentials

See [data-and-storage.md](data-and-storage.md#how-the-credentials-are-protected)
for where and how. In short: Windows DPAPI encryption for your account, or a
`0600` file on macOS and Linux; never logged, never returned to the UI. (DPAPI
encryption ships in the release after v0.1.0; v0.1.0 on Windows stored credentials
unencrypted, in the temp folder.)

### The ChatGPT sign-in

The OAuth flow follows OpenAI's contract for open-source apps
([chatgpt-sign-in.md](chatgpt-sign-in.md)):

- PKCE with a fresh `state`, `nonce` and verifier per attempt; no client secret
  exists to leak.
- The redirect is a loopback listener on `127.0.0.1` that answers only the
  callback path, checks the `Host` header and the `state` before trusting
  anything, and reflects nothing from the request.
- The authorization URL (which can carry an ID-token hint on a repeat sign-in) is
  never put on a command line, in a log or in the UI. Browsers are sent through a
  one-time loopback link that redirects to it once.
- The ID token's signature, issuer, audience, expiry and nonce are verified
  against OpenAI's published keys before anything is saved. Algorithms other than
  RS256, ES256 and EdDSA are refused.
- Refresh tokens rotate; renewal happens under a lock, and credentials are erased
  only when OpenAI says the grant is gone, never because of a network error.
- Sign-out revokes the refresh token at OpenAI (retried) and clears the local copy.

## What REX does not protect against

Be clear-eyed about these:

- **Malware or another program running as you.** It can read your files and ask
  the OS to decrypt what REX encrypted. File permissions and DPAPI separate
  *users*, not programs in one session.
- **A model you ask to run code, with approvals off.** With `--yes` the model can
  do whatever the tool policy allows, unattended.
- **Prompt injection.** Text in files, web pages and tool output is bracketed as
  untrusted data and the model is instructed to treat it so, but no model is
  immune. Read approvals; they are the backstop.
- **The model provider.** What you send a model leaves your machine and is
  handled under that provider's terms. REX can redact common secret patterns
  from tool output, not everything.
- **The loopback services.** `rex serve` and the dev sidecar listen on
  `127.0.0.1` without authentication; any program on the machine can talk to
  them. For the same reason the sidecar never opens your browser for the
  ChatGPT sign-in itself; it hands the page a one-time link to open on a click.
- **Platform differences.** The command resource limits (CPU, address space,
  file descriptors) in the tool runtime are Unix-only. Windows has the timeout,
  output cap and approval policy. Windows is the newest platform, and the tool
  runtime's Windows behaviour has not been audited to the same depth.
- **Unsigned installers.** The Windows installer is not code-signed yet. Verify
  the download against the `.sha256` on the release page when one is published.
- **Receipts prove integrity, not correctness.** A valid signature says the
  receipt is what this machine produced; it does not say the code is right.

## Supply chain

Dependencies are locked (`Cargo.lock`, `package-lock.json`) and every pull
request runs format, lint, the full test suite and a frontend build. The licence
of every dependency was checked when the project was opened (none is GPL-only;
a few are MPL-2.0, which is file-level copyleft).
