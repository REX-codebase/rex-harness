# Where REX keeps your data

> **Version note.** This page describes v0.1.1 and later.
> v0.1.0 on Windows kept its data in the temp folder and stored credentials
> unencrypted; see [Upgrading from an older build](#upgrading-from-an-older-build).

**Everything REX remembers stays on your computer.** There is no REX account and
no REX server: saved keys, your ChatGPT sign-in, run history, receipts and
settings are files in one folder on this machine. This page says which folder,
what is in it, how it is protected, and how to move, back up or erase it.

## The folder

| Platform | Desktop app | `rex` command line |
| --- | --- | --- |
| **Windows** | `%LOCALAPPDATA%\rex-harness` | the same folder |
| **macOS, Linux** | `$XDG_CONFIG_HOME/rex-harness`, else `~/.config/rex-harness` | `~/.rex/harness` |

Set **`REX_STATE_DIR`** to use a different folder. It wins over everything, for
the app and the CLI alike, and it is how you make the two share one sign-in and
one history on macOS and Linux (point both at the same folder).

If no home folder can be found at all, REX falls back to a folder under the
system temp directory. It never uses the current directory. The rules live in
`crates/rex-providers/src/paths.rs`.

Typical `%LOCALAPPDATA%` path: `C:\Users\<you>\AppData\Local\rex-harness`. In
Explorer, type `%LOCALAPPDATA%\rex-harness` in the address bar.

## What is in it

| Path | Holds | Sensitive |
| --- | --- | --- |
| `secrets.json` | Saved API keys, custom endpoint addresses, and the ChatGPT sign-in (`openai-chatgpt#oauth`) and this machine's host id | **Yes** |
| `chatgpt-refresh.lock` | A lock file that keeps the app and the CLI from renewing the ChatGPT sign-in at the same moment | No |
| `signing-key.json` | This machine's Ed25519 key that signs receipts | **Yes** (private key) |
| `runs.jsonl` | The run ledger: one signed receipt per finished run | Describes your tasks |
| `active/` | One small marker per run in flight (for `rex ps`) | No |
| `sessions/` | Named sessions and their compacted history | Describes your tasks |
| `agent-runs/` | Each run's working copy, state, evidence and ledger | Contains your code |
| `checkpoints/` | Workspace snapshots (desktop app) | Contains your code |
| `config.json` | Operator settings such as `max_parallel_runs` | No |
| `mcp.json`, `mcp_servers.json` | External MCP server definitions | `env` values are clear text |
| `cost.json`, `search-index.json`, `fable-sessions/` | Cost records, the local search index, Fable gate sessions (desktop app) | Describes your work |

## How the credentials are protected

`secrets.json` is the only place a key or sign-in is written.

- **Windows:** every value is encrypted with the **Windows Data Protection API
  (DPAPI)** for your Windows account before it is written. The encryption key
  belongs to Windows and is never stored by REX. Another Windows user, a backup
  of the folder, a synced copy, or a stolen `secrets.json` cannot read it.
- **macOS and Linux:** the file is created with mode `0600` and its folder `0700`,
  so only your account can read it. Writes go to a private temporary file that
  is then renamed into place, so a crash keeps the old keys instead of leaving a
  half-written file.
- Keys are never logged, never put in run state or receipts, and never sent back
  to the interface: the UI only learns *whether* a credential exists. The
  ChatGPT access and refresh tokens are held to the same rule.

What this does not do: it does not stop a program that is already running as
you. Such a program can ask Windows to decrypt for itself, or read a `0600` file,
exactly as REX does. See [security.md](security.md).

`signing-key.json` is not DPAPI-encrypted. It is `0600` on macOS and Linux; on
Windows it relies on the folder's default permissions, which in `%LOCALAPPDATA%`
are limited to your account and administrators.

### Upgrading from an older build

- **Windows v0.1.0** kept its data in `%TEMP%\rex-harness` (the app fell back to
  the temp folder because `HOME` is not set on Windows) and the CLI in
  `.rex\harness` under whatever directory it was run from. Newer builds use
  `%LOCALAPPDATA%\rex-harness`. REX does not move the old files on its own. To
  keep a saved key, copy the old `secrets.json` into the new folder; REX encrypts
  it the next time it opens the store. Or simply connect again.
- **Plaintext values** written by an older build are read normally and encrypted
  the first time a Windows build opens the store.

## Sharing a sign-in between the app and the CLI

Both programs use one store, so once they point at the same folder they share
the sign-in. That is the default on Windows. On macOS and Linux:

```bash
export REX_STATE_DIR="$HOME/.config/rex-harness"   # before starting the CLI
```

(or set it for the app too). The ChatGPT access token is renewed under a lock
file, so the app and the CLI cannot renew it at the same moment and invalidate
each other's refresh token.

## Back up, move, reset

- **Back up or move:** copy the folder. On Windows, `secrets.json` can only be
  decrypted by the same Windows account on the same machine, so a copy of it on
  another PC or account will say it cannot be read; connect again there. Receipts
  and sessions move freely.
- **Sign out of ChatGPT:** `rex logout chatgpt`, or Disconnect in the model menu.
  This revokes the session at OpenAI and clears it here.
- **Forget one key:** Disconnect that provider in the model menu.
- **Erase everything REX stored:** close REX and delete the folder. Uninstalling
  the app does **not** delete it, so your runs and keys survive a reinstall.
- **Also revoke from OpenAI's side:** disconnect REX in your ChatGPT settings.
  That works even if you no longer have the computer.

## What leaves the machine

Only what a run needs: your prompts and the files and command output the agent
shows the model go to the model provider you connected, under that provider's
terms. REX sends nothing to a REX server (there is none) and contains no
telemetry or analytics. Optional web search goes to the search provider you
choose. The Windows installer downloads Microsoft's WebView2 runtime if your PC
lacks it.
