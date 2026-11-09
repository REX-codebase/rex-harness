# Changelog

All notable changes to REX Harness. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/); versions follow
[Semantic Versioning](https://semver.org/) (before 1.0, minor versions may
change behaviour).

## [Unreleased]

## [0.1.0] - 2026-11-09

The first public release. REX Harness is open source under `MIT OR Apache-2.0`.

### Added
- **Open source.** `LICENSE-MIT` and `LICENSE-APACHE`; every crate and package
  declares `MIT OR Apache-2.0`.
- **Continue with ChatGPT.** Use a ChatGPT plan instead of an API key through
  OpenAI's Sign in with ChatGPT for open-source apps: dynamic registration, PKCE,
  a loopback callback, ID-token verification, rotating refresh with a lock, and
  revocation on sign-out. Includes the OpenAI Responses wire format, the
  `openai-chatgpt` provider, `rex login chatgpt`, `rex logout chatgpt`,
  `rex auth status`, and the UI OpenAI's guidelines ask for. Not yet exercised
  against OpenAI's live service; see [docs/chatgpt-sign-in.md](docs/chatgpt-sign-in.md).
- **Windowed context** (opt-in, `--windowed-context`): a managed transcript with
  stubbing of old tool results, rolling summaries, overflow recovery, and a
  `recall` tool. The default stateless loop is unchanged.
- **Windows setup wizard**: a branded NSIS installer (licence page, install scope,
  Start menu folder), built and published by the release workflow. Unsigned.

### Added since the first 0.1.0 installer
The installer first published for `v0.1.0` was built from commit `c8798f1`;
everything below is newer than it.
- **Local, encrypted credential storage on Windows.** Saved API keys and the
  ChatGPT sign-in are encrypted with the Windows Data Protection API (DPAPI) for
  your account before they are written. Values saved in plaintext by an older
  build are encrypted the first time the store opens.
- `rex login chatgpt --new-account` and a **Use a different account** button, to
  sign in as a different ChatGPT account than the one saved on this computer.
- `rex login` and `rex auth status` say where the sign-in is stored (this computer
  only).
- A Windows CI job that runs the DPAPI, credential store and data-folder tests on
  a real Windows runner.
- Documentation: a docs index, getting started, a complete CLI reference,
  architecture, security model, data and storage, troubleshooting,
  CONTRIBUTING and SECURITY.

- **The AI is told the environment it works in.** The per-turn state now carries
  an `environment` block: the operating system and architecture, the limits a
  command is held to, which kinds of program are blocked, which build tools are
  on `PATH` (with `PATHEXT` on Windows), and the project files at the workspace
  root. The tool contract tells the model to work to it and, when it can run
  commands, how commands work (an argv list, no shell, no network, a clean
  environment) and what to use instead of a blocked program. Small
  context windows get a compact form. This is prompt version `1.1.0`: a run
  started under an earlier prompt version cannot be resumed (REX refuses to mix
  prompt versions mid-run), and benchmark records from before count as a
  different prompt identity. See [docs/prompt-architecture.md](docs/prompt-architecture.md).

### Fixed since the first 0.1.0 installer
- **Commands crashed while linking.** A command run through the tool runtime was
  capped at 1 GiB of address space; linking crashed in 5 of 5 clean builds and in
  3 of 4 `cargo test` runs. The cap is now 4 GiB, with which every run passed.
- **Failing tests gave no evidence.** `rex_test` turned a recipe that exited
  non-zero (or timed out, or could not start) into an error, so `passed: false`
  could never come back and nothing was recorded. It now returns
  `passed: false`, records the evidence, and puts the cause and the end of the
  output in `summary`.
- **Every tool error was `scope_denied`.** A command that ran and failed, a
  missing file or an I/O error now report the new `tool_failed` code; policy
  refusals stay `scope_denied`. Hosts that treated every error as a policy denial
  should read `error.data.code`.
- **cargo could not find rustup's toolchain.** Commands run with `HOME` set to the
  project folder, so rustup found no default toolchain and `.cargo`/`.rustup`
  folders were created in the project. `CARGO_HOME` and `RUSTUP_HOME` now point at
  the real ones (an explicit value wins, else `~/.cargo` and `~/.rustup`).
  `HOME` is still the project folder.
- **`rex_proof_verify` said `false` for a completed task** unless `rex_proof` had
  been called first, because verification compares against a saved bundle and
  nothing saved one. The bundle is now saved when the task finishes. Tasks that
  finished before this fix need one `rex_proof` call before they verify.
- **`rex --help` exited with an error.** It printed to stderr and exited `2`; it
  now prints the usage on stdout and exits `0` (also `-h`, and `--help` after a
  command). `rex` with no command is still a usage error.
- **Where REX stores its data on Windows.** The desktop app used the temp folder
  whenever `HOME` was unset (always, on Windows), and the CLI used the current
  directory, so a saved key or sign-in could land in `%TEMP%` or in a different
  folder per working directory, and the app and CLI never shared one. Both now use
  `%LOCALAPPDATA%\rex-harness` (or `REX_STATE_DIR`). macOS and Linux locations are
  unchanged. See [docs/data-and-storage.md](docs/data-and-storage.md) for moving
  data from the old location.
- Signing in as a different ChatGPT account failed with advice ("sign out first")
  that did nothing, because a sign-out keeps the saved registration.
- The release workflow's checksum step looked in the wrong build folder.
- The desktop UI no longer calls the credential store a "keychain".
