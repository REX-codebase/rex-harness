# Continue with ChatGPT (use your ChatGPT plan)

REX can spend the user's ChatGPT plan instead of an API key, through OpenAI's
**Sign in with ChatGPT** for open-source, locally hosted apps. This page is how
it works, what it refuses to do, and what to do on the day REX is open-sourced.

> **Status: built, switched off.** The route is closed until
> `policy::OPEN_SOURCE_RELEASE` is `true`, which happens when REX is released
> under an open-source licence. Everything below is exercised by tests against a
> fake identity provider; it has not been run against OpenAI's live service.
> See [Not verified live](#not-verified-live).

## Why it is switched off

OpenAI documents ChatGPT plan usage for "open-source and locally hosted apps"
and sends paid or remotely hosted apps to an interest form. REX is going to be
open source but is not yet, so it does not qualify yet. The decision lives in one
constant, `OPEN_SOURCE_RELEASE` in `crates/rex-providers/src/policy.rs`. While it
is `false`:

- `rex login chatgpt`, the desktop "Continue with ChatGPT" button and the sidecar
  all refuse, with the reason;
- `rex exec --provider openai-chatgpt` refuses before staging anything;
- the provider shows as "After open-source release" and is never "connected";
- a stored sign-in cannot be used for a request. (Signing out still works.)

Policy and sources: [`subscription-policy.md`](subscription-policy.md).

## The day REX is open-sourced (checklist)

1. ~~Add the LICENSE file and change every crate's `license = "Proprietary"`.~~
   Done (2026-11-09): `MIT OR Apache-2.0`, `LICENSE-MIT` and `LICENSE-APACHE` at
   the repository root, every manifest updated.
2. **Make the repository public**, then set `OPEN_SOURCE_RELEASE` to `true` in
   `crates/rex-providers/src/policy.rs`. The licence is not the same thing as
   being open source to OpenAI's reviewers: do not flip the switch while the
   repository is private. `the_release_switch_cannot_be_thrown_before_the_release`
   checks the licence files and manifests; `subscription_oauth_is_offered_only_on_openais_documented_route`
   fails until the policy rows agree with the flag. (`rex-mcp`, the sibling
   repository, still declares its crates `Proprietary`; it was left out on purpose.)
3. Re-read the sources in `subscription-policy.md` (OpenAI changes this surface;
   the docs call it a preview) and update `OPENAI_SIWC_VERIFIED_ON`.
4. Replace the text-only button in `src/components/ChatGptPlan.tsx` with OpenAI's
   approved "Continue with ChatGPT" button and logo
   (<https://developers.openai.com/siwc/website>).
5. Run the live checks in [Not verified live](#not-verified-live) with a real
   ChatGPT account, and fix what they find before announcing the feature.

The frontend asks the backend for the verdict, so the UI opens on its own once
the backend does; only the bundled fallback text in `src/data/providers.ts` says
"opens with the release".

## How it works

`crates/rex-providers/src/chatgpt_oauth/` is the whole client:

| Piece | What it does |
| --- | --- |
| `begin` | picks and persists the host id (`urn:uuid:…`) before the first sign-in, starts the loopback listener, builds the authorization URL (PKCE S256, `state`, `nonce`, `resource`, the identity + plan scopes) |
| `loopback.rs` | `127.0.0.1` only, `GET /auth/callback` only, `Host` checked, `state` checked before anything else is trusted, never reflects the request; one extra single-use path (`/auth/start/<random>`) that redirects to the real URL so it never sits on a command line |
| `complete` | exchanges the code (public client: no secret), verifies the ID token against OpenAI's JWKS (RS256/ES256/EdDSA only; `iss`, `aud`, `exp`, `nonce`, `sub`), checks the granted scopes, saves everything in one write |
| `access_token` | returns a valid token, refreshing under a cross-process lock when it is near expiry; credentials are erased only when OpenAI says the grant is gone, never on a network error |
| `sign_out` | revokes the refresh token at OpenAI (retried), then clears local tokens; the issued client and host id are kept |
| `store.rs` | one JSON record in the existing 0600 credential store, tokens redacted from `Debug` |

On the first sign-in OpenAI registers REX as a user-defined agent (the callback
carries the issued `client_id`, which is saved and reused). No client ID is
embedded in REX and no secret exists.

Requests go to `https://api.openai.com/v1/responses` with `store: false` and
`stream: true`. `crates/rex-providers/src/responses.rs` builds them, reads the
reply from the stream's `response.completed` event, namespaces the tools, replays
the encrypted reasoning items statelessly, and turns OpenAI's
`subscription_sharing_*` errors into the recovery text OpenAI documents. The
windowed context engine (`docs/context-management.md`) works on this wire format
like the others.

### Credentials and where they go

- The access token lives about an hour. A run asks the service for it **at every
  turn boundary**, not once, and the service renews it when it is within five
  minutes of expiry.
- Nothing credential-shaped reaches the UI, a log, the ledger, run state or a
  command line: summaries carry `email`, `plan_usage` and an expiry time only.
- A pasted key is refused for this provider (`set_key` returns an error). The
  only credential is the one a sign-in issued.
- Environment variables never stand in for it (`REX_API_KEY` is not read for the
  internal `openai-chatgpt#…` records).

## Using it

```
rex login chatgpt [--no-browser] [--force-consent] [--json]
rex logout chatgpt [--json]
rex auth status [--json]
rex exec --provider openai-chatgpt --task "…"
```

`--force-consent` asks OpenAI to show the permission screen again: use it only
after you declined plan usage and now want it on. In the desktop app: model menu
-> Connect subscription -> Continue with ChatGPT.

The UI follows OpenAI's UI/UX guidelines: "Continue with ChatGPT" starts the
sign-in; a "You're using your ChatGPT plan / Got it" dialog shows once, after the
first sign-in with plan usage on; "Using ChatGPT plan" with a "Manage usage" link
(<https://chatgpt.com/settings/usage>) sits next to the model selector whenever
the plan is in use; and a run that hits the plan's usage limit shows "Usage limit
reached" with Manage usage as the primary action.

## What it will not do

Per the rule in `subscription-policy.md`: no Codex client ID, no ChatGPT Codex
backend, no scraped cookies or tokens, no spoofed `originator`, no scopes beyond
what OpenAI's open-source contract grants. The sidecar never opens the browser
itself (it answers any page that can reach `127.0.0.1`): it returns the one-shot
launch link and the page opens it on a click.

## Not verified live

Everything here is tested against a scripted identity provider and a scripted
Responses endpoint. Before announcing the feature, run it once with a real
account and check:

- **Discovery and JWKS.** The verifier accepts RS256, ES256 and EdDSA and fails
  closed on anything else; OpenAI's actual signing algorithm was not observed.
- **Dynamic registration.** That the callback carries the issued `client_id`
  exactly as the docs describe, and that the first sign-in shows the approval
  screen.
- **Tool namespaces.** Tools are declared in one `namespace` tool group, as the
  docs describe; the live route's acceptance of REX's full tool set (including
  `recall`) is unconfirmed.
- **Reasoning replay.** Encrypted reasoning items are replayed with their calls;
  whether the route wants the `update_plan` calls' items too is unconfirmed.
- **Usage-limit errors.** The recovery text follows OpenAI's documented codes;
  the live shape of a limit error was not seen.
- **The desktop shell.** The Tauri commands (`chatgpt_sign_in_*`,
  `chatgpt_sign_out`) were written but not compiled in the sandbox this was built
  in (no system GUI libraries). Build `src-tauri` and click through the flow.
- **Windows and macOS** browser launching and the loopback listener.
