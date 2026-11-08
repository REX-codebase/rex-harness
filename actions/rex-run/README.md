# `rex-run` GitHub Action

Runs a REX harness agent task headlessly inside your CI job and fails the
job unless the run completes. The full JSON receipt is uploaded as an
artifact on every run.

## Usage

```yaml
jobs:
  rex-task:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4

      - uses: REX-codebase/rex-harness/actions/rex-run@main
        with:
          task: 'Run the test suite and report failures with file:line references. Do not modify anything.'
          provider: 'anthropic'
          api-key: ${{ secrets.ANTHROPIC_API_KEY }}
```

## Inputs

| input | default | notes |
|---|---|---|
| `task` | — | **required.** What the agent should do. |
| `provider` | `anthropic` | `gemini`, `openai`, `xai`, `deepseek`, `kimi`, `kimi-coding`, `qwen`, `glm`, `local`. |
| `model` | provider default | Model id. |
| `api-key` | — | **required.** Pass a secret. Exported as `REX_API_KEY` for the run step only. An empty value fails the step at once with a message naming the missing secret. |
| `workspace` | `.` | Directory the agent gets a **copy** of. Your checkout is never touched. |
| `max-steps` | `40` | Step budget. |
| `max-tool-calls` | `200` | Tool-call budget. |
| `timeout-secs` | `1200` | Wall-clock budget. |
| `rex-ref` | `main` | REX harness ref to build the CLI from. Pin to a SHA for reproducible CI. |
| `github-token` | `github.token` | Used to post the proof summary. The calling workflow needs `permissions: checks: write` (and `pull-requests: write` for the PR comment); without it the check run is skipped with a warning. |
| `post-summary` | `true` | Post the proof-replay summary as a check run, a PR comment, and the job summary. |
| `allow-unconfined-commands` | `false` | GitHub-hosted ubuntu runners cannot create user namespaces, so REX's command sandbox is unavailable there and agent commands are **refused** (file reads still work). `true` runs them unconfined. Use it only on a disposable runner with a trusted task: an unconfined command can reach anything the runner user can, including the provider key in the `rex` process. |

## Outputs

- `receipt`: path to the JSON run receipt (`rex.exec.receipt/1` schema).
- `run-id`: the REX run id.
- `status`: terminal agent status. The step exits non-zero unless this is `completed`.

## Trust model

- The agent runs with `--yes`: every tool call and plan is auto-approved
  **inside the run's budgets**. Scope what it may do through the task text
  and the workspace you hand it.
- The agent works on a disposable copy under `$RUNNER_TEMP`. Nothing it
  does touches your checkout.
- The API key lives only in the step's environment.
