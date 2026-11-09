# Getting started

This takes you from nothing to a finished, verified task. About ten minutes.

## 1. Install

**Windows.** Download the `…x64-setup.exe` from the
[latest release](https://github.com/REX-codebase/rex-harness/releases/latest) and
run it. It is a normal setup wizard. Windows SmartScreen may warn because the
installer is not code-signed yet: choose **More info → Run anyway**. Details in
[INSTALL.md](../INSTALL.md).

**macOS and Linux.** Build from source (below) or use the installers from a
release when one is published for your platform.

**From source** (any platform, needs [Rust](https://rustup.rs), Node.js 20+ and
the [Tauri prerequisites](https://v2.tauri.app/start/prerequisites/)):

```bash
git clone https://github.com/REX-codebase/rex-harness
cd rex-harness
./quickstart.sh          # installs dependencies and the `rex` CLI, starts the app
```

Only want the command line? `cargo install --path crates/rex-cli` gives you
`rex`; see [cli.md](cli.md).

## 2. Connect a model

REX needs one model to work with. Pick the route that fits you:

| You have | Do this |
| --- | --- |
| A **ChatGPT plan** (Plus, Pro, and so on) | In the app: model menu → **Connect subscription** → **Continue with ChatGPT**. Or `rex login chatgpt`. Eligible requests use your plan instead of API billing, and you can review the usage in ChatGPT settings. |
| An **API key** (Gemini, Anthropic, OpenAI, xAI, DeepSeek, Kimi, Qwen, GLM, NVIDIA NIM, Groq, OpenRouter) | Model menu → **Connect API**, choose the provider, paste the key. The key goes to the local backend and is never shown again. |
| A **local model server** | Choose *Local server* and enter its address. A key is only needed if the server asks for one. |
| Only a **free tier** | NVIDIA NIM, Groq and OpenRouter work as presets. The backend waits out rate limits; see [execution-core.md](execution-core.md) for what to expect from small free quotas. |

Which subscriptions can and cannot be used, and why, is in
[subscription-policy.md](subscription-policy.md). In short: REX connects an
account only where the provider's own documentation allows third-party tools.

Once connected, the model menu lists the provider's real models, fetched live.

## 3. Run your first task

1. Choose a **workspace folder** for the task (or let REX create a fresh one).
2. Type what you want, for example *"add a function that parses ISO dates, with
   tests, and show the tests pass"*, and press **Ctrl+Enter**.
3. REX proposes a **plan** and waits. Read it, then approve or reject.
4. As it works, every file write and command is shown as an **approval** with the
   exact diff or argument list. Allow or deny each one.
5. It finishes with a plain-language result, then the **evidence**: the steps it
   took, the checks it ran and how long they took, and a **receipt**.

REX works on a disposable **copy** of your folder. Your original is not touched
until you review the changes and promote them.

The same run from a terminal:

```bash
rex exec --task "add a function that parses ISO dates, with tests" \
         --workspace . --provider anthropic --json
```

(`--yes` auto-approves every tool call and plan. Use it in CI, not on code you
have not reviewed.)

## 4. Check the receipt

Every run leaves a signed receipt: what was asked, what the model did, what it
spent, and an Ed25519 signature from your machine's key.

```bash
rex runs                     # your history
rex show <run-id>            # one entry
rex verify receipt.json      # was this really produced by that machine, unmodified?
rex replay receipt.json      # does every claim follow from its own evidence?
```

## 5. Next steps

- **Keep going:** follow-ups in the app, or `rex exec --name x` then
  `rex resume x`.
- **Control cost:** `--bid` prints the budgets and a worst-case dollar ceiling
  before anything runs; `--max-tokens`, `--max-steps` and `--timeout-secs` are
  hard stops.
- **Long tasks on big models:** `--windowed-context`
  ([context-management.md](context-management.md)).
- **Understand what is allowed to happen:** [security.md](security.md).
- **Something not working:** [troubleshooting.md](troubleshooting.md).
