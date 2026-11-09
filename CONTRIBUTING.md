# Contributing to REX Harness

Thanks for helping. This page gets you from a clone to a pull request.

REX is licensed `MIT OR Apache-2.0`. By submitting a contribution you agree it
is dual licensed the same way, without extra terms (see the README).

## Set up

You need [Rust](https://rustup.rs) (stable), Node.js 20+, and, for the desktop
shell, the [Tauri prerequisites](https://v2.tauri.app/start/prerequisites/). On
Debian/Ubuntu:

```bash
sudo apt-get install -y libwebkit2gtk-4.1-dev build-essential curl wget file \
  libxdo-dev libssl-dev libayatana-appindicator3-dev librsvg2-dev
npm ci
mkdir -p dist      # the Tauri macro needs this folder to exist for clippy/test
```

## Run it

```bash
npm run dev                                  # the web UI on http://localhost:1420
cargo run -p rex-mcp --bin rex-dev-server    # the backend for it, on 127.0.0.1:8787
npm run tauri dev                            # the desktop app
cargo run -p rex-cli -- --help               # the CLI
```

## Checks CI runs

Run the same before you push:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
npm test            # frontend tests and the npm launcher tests
npm run build       # type-check and production build
```

CI also builds the VS Code extension and the JetBrains plugin, runs the real
backend against a scripted provider, drives the production UI in Chromium, and
runs the credential-storage tests on a Windows runner. One `rex-preview` test
needs Chrome or Chromium on `PATH`.

If you run the suite as root, `secrets::tests::a_failed_save_keeps_the_old_keys`
fails (root ignores file permissions). That is the environment, not the code.

## Ground rules

- **Do not invent permission.** REX connects a provider account only through a
  route that provider's own current documentation allows for third-party tools.
  Never add scraped cookies or tokens, copied first-party OAuth clients, spoofed
  headers, reverse-engineered private endpoints, or scopes the provider did not
  grant. A new provider or subscription route needs an entry in
  `crates/rex-providers/src/policy.rs` with sources, and an update to
  [docs/subscription-policy.md](docs/subscription-policy.md). Tests enforce this.
- **Credentials never leave the backend.** Do not log them, put them in run
  state or receipts, or return them to the frontend. Error text must not contain
  secret material.
- **Say what is verified.** Docs and UI copy must not claim something the code
  does not do. If you could not test it, say so.
- **Tests with the change.** New behaviour comes with tests that fail without it.
  Prefer testing real behaviour over mocking it.
- **Keep changes focused.** One concern per pull request.

## Where things are

See [docs/architecture.md](docs/architecture.md) for the map, and
[docs/README.md](docs/README.md) for every guide. Useful starting points:

| To change | Look at |
| --- | --- |
| The agent loop | `crates/rex-providers/src/autonomous.rs`, `context/` |
| Provider access rules | `crates/rex-providers/src/policy.rs` |
| Credential storage | `crates/rex-providers/src/secrets.rs`, `protect.rs`, `paths.rs` |
| ChatGPT sign-in | `crates/rex-providers/src/chatgpt_oauth/`, `responses.rs` |
| Local tools | `crates/rex-tools` |
| The CLI | `crates/rex-cli/src` |
| The UI | `src/components`, `src/data` |
| The Windows installer | `src-tauri/tauri.conf.json`, `src-tauri/installer/`, `scripts/installer-art/` |

## Pull requests

1. Branch from `main`.
2. Make the change, with tests and docs.
3. Run the checks above.
4. Open a pull request that says what changed and why, and what you could not
   verify. CI must be green before it is merged.

## Cutting a release

Maintainers: see the "cutting a release" section of [INSTALL.md](INSTALL.md).
