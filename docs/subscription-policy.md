# Subscription & access policy

**Verified 2026-09-19** (the OpenAI ChatGPT-plan route was re-read from OpenAI's
own documentation on **2026-11-09**). Sources are official provider
documentation unless marked otherwise. When a provider changes its terms,
update this document, `crates/rex-providers/src/policy.rs`, and the matching
verification date (`VERIFIED_ON`, or `OPENAI_SIWC_VERIFIED_ON` for the OpenAI
ChatGPT rows) together.

## The rule

REX connects a provider account only through routes the provider's own
current terms and authentication documentation clearly permit for
third-party harness use. **Do not invent permission.** Explicit bans remain hard stops. Undocumented routes
remain disabled until the provider documents them, without claiming the provider bans
users or that technical use is illegal. A hook with a grounded account-enforcement risk
is a bug, not a feature.

REX never uses, and will never accept a contribution that adds:

- scraped cookies or session tokens,
- browser or app impersonation (fake client IDs, spoofed `originator`
  headers, replayed first-party OAuth clients),
- reverse-engineered private/internal endpoints,
- OAuth scopes the provider did not grant for third-party use,
- any flow the provider's terms reserve to its own products.

## Verdict matrix

| Provider | API key route | Consumer subscription hook |
| --- | --- | --- |
| Google Gemini | **Offered** - AI Studio key, free tier exists | **Not offered** - plans are for Google's own products |
| Anthropic | **Offered** - Claude Console key | **Forbidden by Anthropic** - never offered |
| OpenAI | **Offered** - Platform key | **Opens with the open-source release** - OpenAI documents Sign in with ChatGPT for open-source, locally hosted apps (`openai-chatgpt`); the route is closed until `OPEN_SOURCE_RELEASE` is flipped. The official Codex CLI stays available as an installed-agent route - see below |
| xAI (Grok) | **Offered** - console.x.ai key | **Partner-gated** - xAI officially enables subscription OAuth in named harnesses; no public route lets an arbitrary new harness self-register |
| Kimi (Moonshot) | **Offered** - platform key | **Offered** - Kimi for Coding, officially for third-party tools |
| Zhipu GLM | **Offered** - Z.AI open platform key | **Not offered** - GLM Coding Plan is limited to Z.AI's tool list |
| Qwen (Alibaba) | **Offered** - ModelStudio key | **Not offered** - free OAuth discontinued; Coding Plan is Qwen Code-scoped |
| GitHub Copilot | - | **Not offered** - third-party use is partner-gated (e.g. OpenCode) |
| NVIDIA NIM, Groq, OpenRouter | **Offered** - your own account's key (added 2026-11-08; **not** re-verified against each provider's official terms) | None. Their free tiers are for development and evaluation; do not use one key to serve other people, and read the provider's current terms first |

## Offered subscription hook

### Kimi for Coding (Kimi membership)

Kimi's official docs state that Kimi Code "is an intelligent programming
service for developers included in Kimi membership benefits" and that
"subscribers can also obtain an API Key to integrate Kimi Code's model
capabilities into third-party development tools and platforms." A second
official page teaches connecting Claude Code - a third-party tool - to the
endpoint.

- Connection: API key from the Kimi Code Console, endpoint
  `https://api.kimi.com/coding` (Anthropic Messages shape).
- Models depend on the membership tier (`kimi-for-coding`, `k3-256k`, `k3`,
  `kimi-for-coding-highspeed`); there is no documented listing endpoint, so
  REX takes the model ID manually.
- Sources:
  - https://www.kimi.com/code/docs/en/
  - https://www.kimi.com/code/docs/en/third-party-tools/claude-code.html

## Refused subscriptions, with reasons

### Anthropic - Claude Pro / Max (forbidden)

Anthropic's own legal page:

> "OAuth authentication is intended exclusively for purchasers of Claude
> Free, Pro, Max, Team, and Enterprise subscription plans and is designed to
> support ordinary use of Claude Code and other native Anthropic
> applications."
>
> "Anthropic does not permit third-party developers to offer Claude.ai login
> or to route requests through Free, Pro, or Max plan credentials on behalf
> of their users. Anthropic reserves the right to take measures to enforce
> these restrictions and may do so without prior notice."

This is exactly the account-suspension risk REX exists to avoid. Claude
subscriptions are not and will not be connectable. API keys from the Claude
Console are the permitted route.

- Source: https://code.claude.com/docs/en/legal-and-compliance

### OpenAI - two distinct routes

**ChatGPT plan through Sign in with ChatGPT: documented for open-source apps;
closed in REX until REX is released as open source.** Re-verified 2026-11-09.
OpenAI now publishes a contract for exactly this: "ChatGPT plan usage is an
optional capability within Sign in with ChatGPT. In addition to identity scopes,
your open-source app can request permission to use the user's ChatGPT plan for
eligible Responses API requests." Its docs cover "open-source and locally hosted
apps"; a paid or remotely hosted app has to go through OpenAI's interest form
instead. The flow is self-service: the app registers itself at the user's first
sign-in (dynamic registration, `agent_name_hint`, no client secret, no
pre-issued client ID), the user approves spending their plan, and requests go to
the public Responses API with the user's access token. The quickstart says the
capability "is available to all open-source partners and selected private
clients".

That supersedes the earlier verdict ("undocumented for arbitrary harnesses"),
which was right when it was written and is no longer true. What did not change
is the rule: REX uses **this** contract and nothing else. It never uses Codex's
own client ID, never calls the ChatGPT Codex backend, and never copies another
harness's OAuth client (see "The rule" above). A route that works for Pi through
a copied first-party client is still not a route REX takes.

REX qualifies for the documented route only as an open-source, locally hosted
app, so **the route is implemented and switched off**:
`policy::OPEN_SOURCE_RELEASE` is `false`. While it is, REX cannot start a
sign-in, hand out a token, or run a model on a ChatGPT plan (the CLI, the
desktop app and the sidecar all refuse with the reason); the provider shows as
"After open-source release". The flag is flipped in the commit that publishes
REX under an open-source licence, and a test refuses a `true` while any crate
still declares `license = "Proprietary"` or the repository has no LICENSE file.
The checklist is in `docs/chatgpt-sign-in.md`.

**Official Codex CLI as an installed agent: retained.** OpenAI's own documentation
covers driving the Codex CLI non-interactively from scripts, pipelines and other
tools (`codex exec --json`), with authentication staying inside the CLI (Sign in
with ChatGPT or an API key), and the OpenAI Terms of Use contain no clause barring
third-party software from launching the official client. On 2026-09-19 the user
decided to keep this route under strict non-abuse controls: the official binary
only, read-only sandbox, no permission-bypass flags, staged review of every change,
and an offline fail-closed contract probe. The full dated contract with sources is
`crates/rex-installed-agents/COMPATIBILITY.md`. If OpenAI's terms or the documented
interface change, the route fails closed until re-review.

- Additional sources for the retained route:
  - https://developers.openai.com/codex/non-interactive-mode
  - https://openai.com/policies/terms-of-use/

- Official sources for the ChatGPT-plan route (read 2026-11-09):
  - https://developers.openai.com/siwc/quickstart
  - https://developers.openai.com/siwc/token-sharing-open-source
  - https://developers.openai.com/siwc/token-sharing-open-source/sign-in
  - https://developers.openai.com/siwc/token-sharing-open-source/preview-limitations
  - https://developers.openai.com/siwc/ui-ux-guidelines
- Official sources for the Codex CLI route:
  - https://developers.openai.com/codex/auth
  - https://developers.openai.com/community/codex-for-oss
  - https://github.com/openai/codex/issues/36886
- Pi implementation evidence (snapshot inspected 2026-09-19), kept as history of why
  copying a first-party client is not the route:
  - https://github.com/earendil-works/pi/blob/36b60d2e8985899743c4cf5bd5f8929832a3f05d/packages/ai/src/auth/oauth/openai-codex.ts
  - https://github.com/earendil-works/pi/blob/36b60d2e8985899743c4cf5bd5f8929832a3f05d/packages/ai/src/providers/openai-codex.ts

### xAI - X Premium / SuperGrok (partner-gated)

Pi currently implements xAI's standard device authorization and refresh-token flow
against `auth.x.ai`, requests `grok-cli:access api:access`, and sends requests to the
public xAI API. xAI has also published official launch pages for Grok subscription
connections in named third-party harnesses including Hermes Agent, OpenClaw, OpenCode
and Warp. The old statement that consumer plans have no third-party programmatic
entitlement was therefore too broad.

What xAI has not published is a general developer registration path or permission to
copy another app's OAuth client ID. That makes subscription OAuth **partner-gated**,
not prohibited. REX keeps it disabled until xAI issues REX its own supported client
registration or publishes a general integration contract. This avoids impersonating
Pi or another approved client while stating the real limitation.

- Official xAI sources:
  - https://x.ai/news/grok-hermes
  - https://x.ai/news/grok-openclaw
  - https://x.ai/news/grok-opencode
  - https://x.ai/news/grok-warp
- Pi implementation evidence (snapshot inspected 2026-09-19):
  - https://github.com/earendil-works/pi/blob/36b60d2e8985899743c4cf5bd5f8929832a3f05d/packages/ai/src/auth/oauth/xai.ts
  - https://github.com/earendil-works/pi/blob/36b60d2e8985899743c4cf5bd5f8929832a3f05d/packages/ai/src/providers/xai.ts

### Google - AI Pro / Ultra

Google documents these consumer plans for its own products (the Gemini app
and Google's own tooling). The documented programmatic path is an AI
Studio API key with a free tier. No official route lets a third-party
harness spend the consumer subscription.

- Sources:
  - https://ai.google.dev/gemini-api/docs/api-key
  - https://ai.google.dev/gemini-api/docs/billing

### Zhipu - GLM Coding Plan (tool-scoped)

Z.AI's own overview:

> "The GLM Coding Plan is strictly limited to use within officially
> supported tools and products. The subscriber shall not use the
> subscription benefits in any unsupported tools or scenarios. If the
> system detects usage through unauthorized or unsupported tools (such as
> SDK-based access or other third-party integrations), some subscription
> benefits may be restricted..."

The supported list covers Claude Code, Cline, OpenCode, Kilo Code, and
others named by Z.AI. REX is not on it, so offering the plan inside REX
would put the subscriber's benefits at risk. The pay-as-you-go Z.AI open
platform key remains fully permitted.

- Sources:
  - https://docs.z.ai/devpack/overview
  - https://docs.z.ai/guides/overview/pricing.md

### Qwen / Alibaba

Qwen Code's authentication docs state the free Qwen OAuth tier "was
discontinued on 2026-04-15" and that "new requests will be rejected." The
Alibaba Cloud Coding Plan is documented only for Qwen Code itself, with a
dedicated endpoint and subscription key; no documentation covers arbitrary
third-party harnesses. The standard ModelStudio API key (DashScope
compatible-mode endpoint) is the permitted route.

- Source: https://qwenlm.github.io/qwen-code-docs/en/users/configuration/auth/

### GitHub Copilot (partner-gated)

GitHub officially supports Copilot subscriptions inside third-party tools
only through named partnerships: "GitHub is officially supporting using
your Copilot Pro, Pro+, Business, or Enterprise subscription with
OpenCode" (January 16, 2026 changelog). REX has no such partnership, so a
Copilot hook is not offered. If GitHub ever opens a general third-party
programme, this entry can be revisited.

- Source: https://github.blog/changelog/2026-01-16-github-copilot-now-supports-opencode/

## Removed installed-agent routes (2026-09-19)

The Installed Agent settings surface previously offered vendor-CLI backends for
Google Antigravity, Claude Code, Cursor and Pi. All four were removed on
2026-09-19 after route-by-route terms audits: Google's Antigravity Additional
Terms section 6 expressly prohibits third-party software accessing the service
with Antigravity OAuth (no ACP exception); Anthropic expressly forbids
third-party products offering claude.ai login or Pro/Max rate limits; Cursor
publishes no third-party harness entitlement for its consumer subscription; and
Pi's pattern of re-implementing providers' OAuth with copied first-party client
IDs has no provider-published permission. OpenAI Codex CLI is the only retained
installed-agent backend. Details and sources:
`crates/rex-installed-agents/COMPATIBILITY.md`.

## How this is enforced in code

- `crates/rex-providers/src/policy.rs` carries every verdict above as data,
  with the same sources, and exposes it to the UI via `ProviderSummary`.
- Tests fail if a non-offered route loses its sources, if any
  consumer-subscription OAuth route other than OpenAI's documented one is ever
  marked offered, if the OpenAI route stops citing its Sign in with ChatGPT
  documentation or disagrees with `OPEN_SOURCE_RELEASE`, if that switch is
  thrown while the repository is still proprietary, or if Anthropic's
  prohibition regresses.
- The settings UI renders these verdicts next to each provider, including
  the reason a subscription is not connectable.

## Re-verification

Before adding or changing a route: read the provider's current official
terms and auth documentation (not blog posts, not what other harnesses
get away with), update the verdict, sources, and `VERIFIED_ON`, and run
`cargo test -p rex-providers`.
