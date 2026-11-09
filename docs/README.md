# REX Harness documentation

Start with **[Getting started](getting-started.md)**. Everything else is here by
what you are trying to do.

## Use REX

| Guide | What it covers |
| --- | --- |
| [Getting started](getting-started.md) | Install, connect a model, run and check your first task. |
| [Install](../INSTALL.md) | Windows, macOS and Linux installers, SmartScreen, WebView2, cutting a release. |
| [The `rex` command line](cli.md) | Every command, option, exit code and environment variable. |
| [Continue with ChatGPT](chatgpt-sign-in.md) | Use your ChatGPT plan instead of an API key: how it works and what it will not do. |
| [Which accounts REX can use](subscription-policy.md) | The provider-by-provider verdicts, with sources, on subscriptions and API keys. |
| [Troubleshooting](troubleshooting.md) | Symptom, cause, fix. |

## Your data and your safety

| Guide | What it covers |
| --- | --- |
| [Where REX keeps your data](data-and-storage.md) | The folder, every file in it, how credentials are protected, backup and reset. |
| [Security model](security.md) | What REX protects, how, and what it does not. |
| [Security policy](../SECURITY.md) | How to report a vulnerability. |

## How it works

| Guide | What it covers |
| --- | --- |
| [Architecture](architecture.md) | The pieces, how a run flows, the trust boundaries, the crates. |
| [Execution core](execution-core.md) | Measured behaviour on free and small models: rate limits, lean prompt, final check. |
| [Windowed context](context-management.md) | The managed transcript for large-window models: stubbing, summaries, recall. |
| [Local tools](local-tools.md) | The tool protocol, approvals and limits. |
| [Prompt architecture](prompt-architecture.md) | How every model call's system prompt is assembled. |
| [Ultra mode](ultra-mode.md) | Contract-driven runs with an adversary and a clean-room judge. |
| [Agent-operator mode](agent-operator-mode.md) | Task-scoped custody for agents that arrive with a task. |
| [Live app preview](live-preview.md) | The preview trust boundary and its evidence gates. |

## Integrate

| Guide | What it covers |
| --- | --- |
| [REX MCP server](rex-mcp.md) | Host setup, contracts and operations for the stdio MCP server. |
| [REX MCP Ultra](rex-mcp-ultra.md) | The host-driven Ultra loop and its truth boundary. |
| [REX-search](rex-search.md), [search providers](search-providers.md), [footprint](rex-search-footprint.md) | The evidence-retrieval layer and its resource cost. |

## The interface design system

[Aura](aura.md) (the REX mark), [buttons](buttons.md) and [motion](motion.md).

## Contribute

[CONTRIBUTING](../CONTRIBUTING.md) has setup, the checks CI runs, and the ground
rules. The [roadmap](../ROADMAP.md) lists what is done and what is next, and the
[changelog](../CHANGELOG.md) says what changed in each release.
