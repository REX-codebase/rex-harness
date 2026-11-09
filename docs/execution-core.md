# Execution core: running on free and small models

Ultra's verification institution (contract, adversary, judge, candidates) sits
on top of one worker loop, `crates/rex-providers/src/autonomous.rs`. Live runs
against free hosted models showed that the weak points were in that worker
loop and the provider layer under it, not in Ultra's verification. This
document records what was measured, what changed, and what has not been done.

## What the live runs showed

All numbers below were measured in this repository's own runs (Groq and NVIDIA
NIM free keys, 2026-11-08), not taken from provider marketing.

| Finding | Evidence |
| --- | --- |
| A custom endpoint saved in the Connect dialog was invisible to the agent. | `no base URL configured` on every run; each provider service kept the URL only in its own memory. |
| The dev sidecar, run plainly, could not run agents at all. | It built three separate in-memory key stores; a key saved in the UI never reached the agent. |
| A free Groq key allows 8,000 tokens a minute; one harness request was ~6,000 tokens. | `Limit 8000, Used 6062, Requested 6439` on the second request. |
| The retry loop burned its three attempts in about two seconds on a 429 and ignored the provider's "try again in 33s". | Relay log: three 429s within three seconds, then the run failed. |
| Healthy hosted calls took up to 19.3 s; the transport cut every call at 20 s. | MiniMax on NVIDIA NIM: 4 s to 19.3 s per call. |
| A small model finished the work, then never said so. | Nemotron 3.5 Lightning created and read back the file, re-read it four more times, and the run ended "no progress across turns". |

## What changed

### Provider layer

- **One endpoint, everywhere.** The base-URL override is stored beside the key
  (`<provider>#base_url` in the shared secret store), so every service that
  shares the store resolves the same endpoint. Disconnecting forgets it. The
  dev sidecar now hands all its services one shared in-memory store.
- **Rate limits are waited out, visibly.** `provider_failure::plan_rate_limit_wait`
  reads the provider's own hint (Google `RetryInfo`, or "try again in 1m26.4s")
  and the loop waits that long, in cancellable slices, with an activity line
  such as "rate limited by the provider; waiting 35s before retrying". A hint
  over 90 seconds (a daily quota) fails the turn at once and surfaces the
  number instead of hammering the provider. Cancel lands within a fraction of a
  second, and ends the run as `Cancelled`, not as a provider error.
- **Redaction is unchanged.** Provider bodies still never reach durable state.
  Only a number (seconds) and identifier-shaped `type`/`code` strings are kept;
  a test pins that sentences cannot ride through either field.
- **Model calls get 120 s**, catalog and search GETs keep 20 s.

### Worker loop

- **One final check before a stall ends a run.** When the no-progress limit is
  hit, the run is not failed yet. For one turn only `update_plan`,
  `complete_task` and `ask_user` are offered (and the harness refuses anything
  else the model calls anyway). A completion claim still goes through the
  gates, which check the workspace and refuse while plan items are unfinished,
  so the final check cannot wave a run through. If the model still does not
  comply, the run ends exactly as before. Each stall gets one chance; real
  progress clears it.
- **Lean prompt (opt-in).** A run option that offers an 11-tool core, drops
  the skills catalog and sends compact state. The system prompt's tool
  contract, the wire declarations and the enforced allowlist all use the same
  narrowed set, so the model is never told about a tool it does not have. Wire
  declarations are now also filtered by `allowed_tools` for role-scoped runs.
  Measured on a real request in the development sandbox: **26.8 KB to 11.5 KB
  (about 6.7k to 2.9k tokens), a 57% cut.** About 8.5 KB of the saving was a
  skills catalog from that machine's home directory; on a machine with no
  skills the cut is smaller (an estimate of roughly 40%, from the tool and
  formatting savings alone). Lean gives up web search and fetch, skills,
  memory and sub-agent exploration. Model quality under lean has not been
  measured.
- Free-tier presets (NVIDIA NIM, Groq, OpenRouter) turn it on by default; the
  switch is in Run options and an explicit choice always wins.

### Verified end to end

`verification/usage-flow/` drives the real backend binary against a scripted
OpenAI-compatible server: a key and custom base URL set through the API, a
Groq-shaped 429, a 22-second response, a stall, then the final check ending
`completed` through passing gates. No relay and no `--store-file` is needed.

CI runs it (job `usage-flow`), the motion-layer checks and the
motion-preference contract script (job `ui-verification`), and the existing
Rust and frontend jobs. The Rust job also builds and lints the desktop crate,
which the sandbox this was written in could not (it lacked the GTK development
packages), and runs the whole workspace suite.

## Using a free key

Connect through the model menu: pick NVIDIA NIM, Groq or OpenRouter, paste your
own key. Facts that matter, all measured unless marked:

- Groq's free tier: 8,000 tokens per minute per model. The lean prompt helps,
  but a long task will still be slow because each request waits for the window.
- NVIDIA NIM: many listed models are slow or not callable for a free account
  (five timed out at 70 s and two returned 404 in one probe). Rate limit
  reported by the community at about 40 requests a minute (not measured).
- OpenRouter: `:free` models are limited to about 50 requests a day without
  credits, per OpenRouter, and failed requests count (not measured).
- A local server such as Ollama ignores the key, but REX needs one saved: type
  any placeholder.
- A free key is the account holder's own allowance for development and
  evaluation. It is not a way to serve other people; read the provider's
  current terms before sharing a key or building a paid service on one.

## Not done

- Ultra's own roles were not changed. The judge and adversary already accept a
  different provider and model; the UI does not expose that yet.
- Per-turn prompt caching, a cheaper summariser model, and routing planning
  turns to a smaller model than execution turns are designs only.
- In the development sandbox (running as root, not CI) the existing suite
  stalled intermittently in browser-dependent tests (3 of 14 runs on unmodified
  code), leaked headless Chromium processes (37 left behind per run) and had
  two tests that fail there but pass in CI. All of that predates this work; the
  suite passed on its first CI run.
