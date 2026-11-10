# Changelog

All notable changes to REX Harness. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/); versions follow
[Semantic Versioning](https://semver.org/) (before 1.0, minor versions may
change behaviour).

## [Unreleased]

## [0.1.2] - 2026-11-10

Run-lifecycle fixes in the task screen, one folder for delegated tasks on
Windows, and an installer that can be run from a fresh clone. New tests cover the
task screen (they drive the real `App`), the install scripts, the state-folder
resolution and the version check.

### Added
- **Prebuilt `rex` command line on every release**: `rex-<platform>.tar.gz`
  (Linux x86_64, macOS Apple Silicon) and a `.zip` for Windows, each with a
  `.sha256` beside it. `scripts/install.sh` downloads the right one, refuses it if
  the checksum does not match and installs `rex` to `~/.local/bin`, so the CLI no
  longer needs a Rust toolchain. See [INSTALL.md](INSTALL.md#command-line-only-rex).
- **A way back after a run.** A finished run now offers **New task**, which
  releases the run's sandbox, clears the task box and puts the caret in it. Before,
  only the simulated tour had one.
- **A notice when the backend stops answering** while a run is going (after about
  seven seconds of failed polls), with **Stop watching**. It used to show "Working"
  forever.
- `quickstart.sh --build`, `--no-app` (dependencies and the CLI only) and `--help`;
  `scripts/rex-mcp-install.sh --check-store` and `--print-config`.
- `scripts/test-installers.sh` and a CI job for it, and a version check that now
  covers every manifest and the release workflow's default tag, not three of them.
- A favicon, description and theme colour for the page; a DMG window that lays out
  the drag-to-Applications icons; Debian package section and priority.

### Changed
- The composer says which search engine is in use ("Search · REX-search") instead
  of always "REX-search not connected", which was untrue in the desktop app.
- The "Recent" list of sample runs appears only when no backend is connected. With
  one connected it listed invented runs under the person's real one.
- `rex --help` names the Windows data folder.

### Fixed
- **`./quickstart.sh` failed with "Permission denied"** on a fresh clone: it was
  committed without its executable bit, and the README's first command is
  `./quickstart.sh`.
- **A second run could be started over a running one.** Run and Ctrl+Enter stayed
  enabled during a custody, Ultra or installed-agent run (only the simulated tour
  locked them), so a second run orphaned the first: still spending its budget, with
  an approval nobody could see. The composer is now locked until the run ends, a
  staged installed-agent result counts until it is promoted or discarded, and a
  double click is ignored.
- **Pressing Run with no backend removed the composer.** It settled the task box
  away, leaving a setup checklist that says "describe a task above" and nothing to
  type into. Live runs of every kind now keep the composer.
- **Starting a run gave no feedback**, and a failed start always named Gemini. The
  default run never showed that it was starting, and four copies of the failure
  record hard-coded `provider: "gemini"`. There is one now, and it names the
  provider that was selected.
- **A Fable gate that could not open failed silently.** It pulsed the setup
  checklist, which is not on screen when a backend is connected. The failure now
  appears where the run would have been.
- **A run from one engine stayed on screen under the next run's view** (an Ultra
  result above a later custody run), and polling for Ultra runs was never stopped
  when the app closed. One poll loop now serves every engine, and a response that
  arrives after the person moved on is ignored instead of bringing the old run back.
- "Receipts and checks will appear here after a run" was shown under an Ultra,
  installed-agent or delegated run that was on screen.
- Any task containing "ui", "app" or "react" as part of a longer word ("build",
  "mapping") was treated as a web-page task by the tour.
- **Delegated tasks and the human Stop on Windows.** The desktop app read `HOME`
  for the shared task store, which Windows does not set, so the store went to a
  `.rex\harness` folder relative to wherever the app was started (read-only for an
  install for everyone), and it ignored `REX_STATE_DIR`. `rex-mcp`, the dev server and
  the app now all use the one resolver in `rex_providers::paths`. See
  [INSTALL.md](INSTALL.md) for moving existing tasks. macOS and Linux locations are
  unchanged.
- **User skills and instructions never loaded on Windows**, and a git root that is
  the home directory (a dotfiles repo) was trusted as a project root, because only
  `HOME` was read. `USERPROFILE` is used when `HOME` is unset.
- **`rex-mcp-install.sh` never refused a newer task store.** Its check looked for
  `"store_schema_version":2` with no space; the store writes `": 2"`, so it matched
  nothing and an older server could be installed over a newer store. It also created
  `~/.rex/harness` even when `REX_STATE_DIR` was set, printed config with unquoted
  paths, and could not install on Windows (`rex-mcp.exe`).
- **`quickstart.sh` reported a successful install as a failure** when
  `~/.cargo/bin` was not on `PATH`, ran `cargo install` twice when it did fail
  (hiding the first error), listed a Linux package name CI does not use, and treated
  any unknown argument as "start the app".
- Two `rex-providers` tests failed on any machine that runs them as root or has
  skills installed in its home folder: they now skip, or look at the answer field,
  instead of depending on the machine.

## [0.1.1] - 2026-11-09

The first installer published as `v0.1.0` (built from commit `c8798f1`) had the
known issue listed under 0.1.0 below. This release fixes it, along with the
items below.

### Added
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

### Fixed
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
- **A rejected, failed or cancelled Ultra run reported its end before its workspace
  was rolled back.** The terminal state was published first and the rollback ran
  as the run's thread wound down, so a reader that acted on "rejected" straight
  away could still see the run's changes (this also made a test fail about once
  in 40 runs). The workspace is now restored before the state is published.
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

### Known issues
- On Windows, this version keeps its data in `%TEMP%\rex-harness` (the app) and in
  `.rex\harness` under the current directory (the CLI). Fixed in 0.1.1; see
  *0.1.1 → Fixed*.
