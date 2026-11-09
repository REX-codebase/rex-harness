# Changelog

All notable changes to REX Harness. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/); versions follow
[Semantic Versioning](https://semver.org/) (before 1.0, minor versions may
change behaviour).

## [Unreleased]

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

### Fixed
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
  `.rex\harness` under the current directory (the CLI); see *Unreleased → Fixed*.
  Saved keys are stored in plaintext in that file.
- The Windows installer has no `.sha256` on its release page.
- The Windows installer is not code-signed, so SmartScreen warns.
