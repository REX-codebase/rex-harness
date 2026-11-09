# Usage-flow verification

Proves, against the real `rex-dev-server` binary and a scripted OpenAI-compatible
provider, the fixes that came out of live runs on free hosted models:

- a key and custom base URL set through the API reach the agent (no relay, no
  `--store-file`)
- a 429 is waited out using the provider's own hint, with a readable activity line
- a 22 second model call succeeds (the old transport cut calls at 20 s)
- a run that did the work and then stalled gets one final check and completes
  through passing gates

```bash
cargo build -p rex-mcp --bin rex-dev-server
python3 verification/usage-flow/e2e-flow.py
```

The scripted provider is `mock-openai.py`; it logs request metadata only. The
script takes about 30 seconds because it really waits out the 429 and the slow
response. Unit-level coverage lives in `crates/rex-providers` (see
`docs/execution-core.md`).
