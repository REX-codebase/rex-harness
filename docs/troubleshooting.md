# Troubleshooting

Find your symptom; each entry says what is happening and what to do. If it is
not here, open an issue with the exact message and what you ran (never paste a
key or token).

## Installing

**Windows says "Windows protected your PC".** The installer is not code-signed
yet, so SmartScreen warns about every unsigned download. Click **More info**,
then **Run anyway**. To check the file is intact, compare its SHA-256
(`Get-FileHash -Algorithm SHA256 .\<installer>.exe`) with the `.sha256` on the
release page when one is published.

**Setup says it needs to download WebView2.** The app's window uses Microsoft's
WebView2 runtime. Windows 11 and current Windows 10 already have it; otherwise
Setup downloads it once, so it needs an internet connection for that step.

**The app opens a blank window.** WebView2 is missing or broken. Install the
Evergreen runtime from Microsoft and start the app again.

## Connecting a model

**"Provider rejected the API key."** The provider answered 401 or 403. Check the
key is for the provider you chose, has not expired, and has access to the models
you want. Disconnect and paste it again; keys are never shown after saving.

**"Rate limited."** The provider throttled you. The agent waits out the delay the
provider reports (up to 90 seconds) and tells you it is waiting. A longer hint,
such as a daily quota, ends the turn at once and shows the number. Free tiers
are small; see [execution-core.md](execution-core.md).

**The model list is empty or fails to load.** The provider returned no usable
models, or the network is blocked. Test the key with the provider's own tools.
For a custom or local endpoint that has no model-listing API, type the model id
yourself.

**"No API key stored for this provider" / "credential store error".** Nothing is
saved for that provider, or the store cannot be read. On Windows, a
`secrets.json` copied from another PC or account cannot be decrypted; connect
again. See [data-and-storage.md](data-and-storage.md).

## Continue with ChatGPT

**The browser did not open.** Use `rex login chatgpt --no-browser`, or in the app
use the "Open the sign-in page" link. The link works once, on this computer: it
points at a port REX is listening on locally.

**"Sign-in timed out."** REX waits five minutes for the browser to finish. Start
again.

**"ChatGPT plan usage was declined."** You signed in but did not allow REX to use
your plan. Run `rex login chatgpt --force-consent` (or *Allow ChatGPT plan usage*
in the app) to see the permission screen again.

**"Not signed in" or "the ChatGPT sign-in has expired or was revoked."** Sign in
again. This also happens if you disconnected REX in ChatGPT settings.

**"Usage limit reached."** Your ChatGPT plan, or the limit you set for REX, is
used up. Choose **Manage usage** (or open <https://chatgpt.com/settings/usage>) to
see which and change it. The run stops; nothing is retried in a loop.

**"That is a different ChatGPT account from the one saved on this computer."**
REX registered itself under the first account you signed in with, and that
registration is bound to it. To switch, sign out (`rex logout chatgpt`), then
sign in with **Use a different account** in the app or
`rex login chatgpt --new-account`. REX registers afresh for the new account; you
may also want to disconnect the old registration in ChatGPT settings.

**Port 1455 is busy.** Nothing to do: REX uses that port when it is free and
otherwise lets the system choose one.

**Use it in the app and the CLI.** They share a sign-in when they use the same
data folder (the default on Windows; set `REX_STATE_DIR` on macOS and Linux). See
[data-and-storage.md](data-and-storage.md#sharing-a-sign-in-between-the-app-and-the-cli).

## The Terminal, Editor and Git tabs

**"Open a folder to …".** These tabs need a project folder. Use the folder chip in the
status row, or **Open folder…** on the tab. A path cannot be typed: it has to be chosen
in the system dialog. See [workspace.md](workspace.md).

**The git chip says "Git not installed".** Install git from https://git-scm.com and
reopen REX. It looks in the usual places as well as `PATH`, so a desktop app started
from the Dock or Start menu still finds it.

**A commit fails with "Git does not know who you are".** Run
`git config --global user.name "Your Name"` and
`git config --global user.email "you@example.com"` in a terminal, then commit again.

**The git chip says "Git unavailable" and shows a message about ownership.** Git
refuses a repository owned by a different user. Follow the command in the message
(`git config --global --add safe.directory <folder>`).

**The Editor will not open a file.** It opens only UTF-8 text files up to 2 MB, and
says which rule the file broke. Use the Terminal for anything else.

**The Editor shows "Loading editor…" for a long time.** The editor is bundled and
needs no network. If it never appears, restart the app and report it.

**The shell exited.** Press **Restart** on the Terminal tab; it starts a new shell in
the same folder.

## Running tasks

**A run is "waiting on approval" and nothing happens.** It is waiting for you.
Approve or deny the plan or tool call in the app. A headless `rex exec` without
`--yes` exits with code 2 when it needs one.

**A run ended with "no progress".** The model kept reading without changing
anything. Small models do this; REX offers one final check (declare completion or
ask a question) before failing. Give the task more concrete success criteria or
use a stronger model.

**A run stopped with a budget message.** A step, tool-call, token or time limit
was reached. Raise it (`--max-steps`, `--max-tokens`, `--timeout-secs`) if the
task legitimately needs more. For very long tasks on large-window models, try
`--windowed-context`.

**"Context window exceeded."** With `--windowed-context` REX recovers from this
itself by folding older turns into a summary. Without it, each turn is sent on its
own and cannot overflow.

**My changes are not in my folder.** The agent edits a disposable copy. Review
and promote the changes; your original is untouched until you do.

## Where are my files?

See [data-and-storage.md](data-and-storage.md). On Windows:
`%LOCALAPPDATA%\rex-harness`. Run history is `runs.jsonl`; `rex runs` lists it.

## Still stuck

Open an issue at <https://github.com/REX-codebase/rex-harness/issues> with your
platform and version (`rex --version`), the exact message, and the steps to
reproduce. For anything security-related, follow [SECURITY.md](../SECURITY.md)
instead.
