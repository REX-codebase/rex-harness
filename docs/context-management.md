# Context management: the windowed mode

The default REX loop is **stateless per turn**: each request carries the system
prompt, one fresh state message and only the *previous* turn's tool exchange.
It can never overflow a window, which is the right trade for the small
free-tier models the lean profile targets (see
[execution-core.md](execution-core.md)). The cost is continuity: a file read
three turns ago survives only as a one-line digest entry, so the model reads it
again.

**Windowed mode** keeps a real transcript and manages it against the model's
actual context window. It is **opt-in**. A run that does not ask for it takes the
same code path it always has, and the existing request-shape tests pin that.

```bash
rex exec --windowed-context --task "..."            # window from the model table
rex exec --windowed-context --context-window 200000 # state the window yourself
REX_CONTEXT_WINDOW=1000000 rex exec --windowed-context --task "..."
```

`rex resume` takes the same flags. `rex serve` and the dev sidecar accept
`"windowed_context": true` and `"context_window": N` in the run request, and the
desktop `custody_begin` command takes `windowedContext` and `contextWindow`.
There is no UI toggle yet.

## The design in one picture

The transcript is **event-sourced**: a lossless log, and a lossy *view* of it.

```
 model reply + tool results ──►  state/context/log.jsonl     every turn, whole,
                                                             appended once, never rewritten
                                        │ projected by
                                 state/context/view.jsonl    stub watermark, summaries,
                                                             learned bytes-per-token
                                        │
 request = system + tools
         + [ turns folded into a summary : not sent, summary rides in the state ]
         + [ turns before the watermark  : bulky results and arguments stubbed  ]
         + [ newest turns                : whole                                ]
         + state message (plan, budget, notes, compacted_history, recalled)

 BM25 index over the log  ──►  `recall` tool, and unasked "recalled" passages
```

Because compaction changes the *projection*, never the data:

* nothing is ever lost. A summary that is wrong cannot destroy what it summarised,
  and `recall` can read any original result back;
* a crash cannot corrupt history. Both files are append-only, a torn last line is
  skipped, and resume replays them to the exact state;
* the same log answers retrieval, so there is one store and one source of truth.

## Where the window comes from

The catalog carries no context size, so `context/limits.rs` resolves it in order,
and always reports which step answered (`window_source` in the stats):

1. an override: `--context-window`, the run option, or `REX_CONTEXT_WINDOW`;
2. a table of model families;
3. a conservative default of 32,768 tokens.

**The table holds floors, not marketing maxima**: compacting a little early costs
some summary tokens; overflowing a real window fails a turn. Pass the override to
use a larger window than the table credits. `local` and `custom` endpoints skip
the table on purpose: the same model name behind Ollama commonly runs with a far
smaller `num_ctx` than the model's architectural maximum, and REX cannot see that
setting. Gemma models, served from the Gemini API, are listed below the Gemini
floor for the same reason.

Very large windows are not filled just because they exist. Unless the window was
stated, the working set is capped at 400,000 tokens: long contexts are priced per
input token and retrieve less well. A stated window lifts the cap to itself.

Everything below is a fraction of the **input budget**: the window, minus the
8,192-token output reserve, minus a 3% margin for estimator error.

If the input budget is under **20,000 tokens** (a window under about 29k) the run
declines windowed mode, says so in its events, and keeps the stateless loop. The
system prompt and tool declarations are about 27 KB (7.7k tokens) on every
request, and below that line they would take so large a share that every turn
would compact.

## The tiers

| Pressure | Share of input budget | Action |
|---|---|---|
| Low | under 55% | nothing |
| Elevated | 55% | **Stub**: replace old tool results and bulky arguments with one-line stubs |
| High | 75% | stubbing if it frees enough, otherwise **summarise** the old prefix |
| Critical | 90% | stubbing if it is enough, otherwise an **emergency** summary and a one-turn tail |

The cheapest sufficient tier always wins, at every level of pressure.

**Stubbing** needs no model call and loses nothing. A stub names the tool, target
and outcome, previews the first 100 characters, and says how to get the rest back:
`[cleared from context: read_file src/a.rs -> ok, 12431 chars. Began: "...".
recall(turn=7, call=0) returns it in full.]` Results under 500 characters are
never stubbed (the stub would cost as much). Arguments over 400 characters (file
bodies in `create_file`, patch text) are cut from stubbed turns, because the file
is on disk. The stub watermark moves in **batches**, only when at least 10% of the
budget would be freed (below High pressure), because moving it rewrites every
message after it and so rebuilds a provider's prompt cache from that point.

**The protected tail**: the newest three turns, or up to 15% of the budget if that
holds more, are never touched. Turns are only ever removed whole, so a tool call
is never separated from its result. In an emergency, or after two compactions in a
row that freed under 10% of the budget, the tail shrinks to one turn.

**One oversized result** can never overflow a small window: a result larger than
25% of the budget is shown head and tail with a marker, and stays whole in the log.

## Summaries

A summary is written by **the run's own model**, with no tools, and is **rolling**:
each fold receives the previous summary and the plan and merges the new turns in,
so facts survive any number of folds without the summary growing. The prompt asks
for seven sections (task, progress, files, commands, decisions, dead ends, next
step) and to keep identifiers, paths and error text exactly. A history too long for
one request is folded chunk by chunk, each anchored on the summary so far; the
chunk size follows the window.

Compaction can always happen. If the model call fails, returns too little, would
pass the token budget, or three have failed in a row, the summary is built **from
the log by code**: files changed, what each turn did, the failures. It carries no
analysis and says it is mechanical. Compactions cannot follow each other within
two turns unless the window is about to overflow.

Tool output is attacker-influenced, and a summary is written in the agent's own
voice, so an instruction hidden in a file could come back looking like the agent's
own conclusion. Three defences: the summarizer is told to record and never to obey;
its input is bracketed as untrusted data; and its output is scanned line by line
with the same patterns that guard workspace notes, replacing any line that reads
like an instruction. That is defence in depth, not proof. The summary is also
framed to the model as imprecise, to be checked, never a source of instructions.

## Retrieval

`recall` is offered only in windowed runs, and is read-only (so a read-only role
such as the adversary may use it). It has three forms:

* `recall(turn=7, call=0)` pages one result back in, 12,000 characters at a time,
  with `next_offset`;
* `recall(query="E0432 serde_yaml")` searches everything the run has seen that is
  no longer shown in full;
* `recall()` lists the archived turns.

Search is BM25 over 1,400-character passages, with a code-aware tokenizer:
`parseConfig`, `read_file` and `src/autonomous.rs` are each indexed whole and as
their sub-words, and the tool name and target count extra. There are no
embeddings: most of the providers REX talks to have no embeddings endpoint, and
agent transcripts are mostly identifiers, paths and error codes, which lexical
ranking handles well.

A result whose file was **changed later by `create_file`, `edit_file` or
`apply_patch`** is returned with `stale_after_turn`, the same guarantee working
memory gives (reads of changed files are dropped). A file rewritten by a shell
command is not tracked.

Relevant passages are also offered unasked, under `recalled` in the state message
(up to three, 700 characters each, only above a relevance floor), chosen by what
the in-progress plan items say. Nothing is offered until something has been
archived.

## When the estimate is wrong

Tokens are estimated from bytes with a ratio that starts at 3.5 bytes per token and
is corrected by the provider's own prompt-token count after every clean request
(one with no image). The correction is asymmetric: it moves fast when the provider
reports more tokens than estimated and slowly when it reports fewer, because
under-counting is how a window overflows.

If a provider still refuses a request as too long (HTTP 400, 413 or 422 with a
recognised context-length message; a tokens-per-minute quota is deliberately not
one), the loop learns the real size (it plans around 85% of the refused request
from then on), makes room and retries, up to twice. If that does not help, the run
ends with the provider's own error, marked `context_window_exceeded`.

## Spend, caching and the token budget

The history is re-sent every turn, so the stateless default of 250,000 tokens would
end a windowed run after a handful of turns. A windowed run that chose no budget
gets twenty input budgets, never above the 4,000,000 ceiling every budget is clamped
to. A **custody grant states its own ceiling** and a run never exceeds its grant, so
the managed request raises the grant explicitly (`with_windowed_context`) rather
than anything being raised behind it; the larger figure is in the audit record.

Tokens a provider reports as cache writes or reads count toward the budget **at
full weight**. That overstates spend where caching is on, which is the safe
direction.

For Anthropic's own endpoint the system prompt, the tool list and the last message
of history are marked for prompt caching, so a long transcript is paid for once;
the state message that follows changes every turn and is not marked. On Anthropic
and OpenAI-compatible endpoints the state message is last, so everything before it
is a stable prefix. The conversation opens with a constant one-line user message,
because Anthropic has required the first message to come from the user and strict
chat templates refuse an assistant turn straight after the system prompt. Gemini
keeps the state message first, as the stateless loop does (its contents must
alternate), so it gets no cache-friendly prefix.

## On disk and in the UI

```
state/context/log.jsonl     every turn, whole (also what retrieval searches)
state/context/view.jsonl    stub watermark, summaries, learned bytes-per-token
state/evidence/turn-N-request.json   a small manifest of what was shown
state/evidence/compact-*-request|response.json   each summarizer call
state/ledger.jsonl          context_stub, context_compaction, context_overflow, recall
```

A windowed request holds the whole transcript, which `log.jsonl` already has, so
the evidence folder records a manifest per turn instead of a copy (copies would
grow quadratically). `AgentSnapshot.context` reports the window and its source,
the input budget, the estimated tokens and pressure, turns recorded, stubbed and
folded, compactions by kind, overflow recoveries, summary size, the learned
bytes-per-token with the number of samples behind it, and `repaired_pairs`
(recorded turns whose call/result pairing had to be repaired; above zero means a
provider or a loop path produced an unmatched call or result).

## How it relates to the other memory pieces

| Piece | Scope | Windowed mode |
|---|---|---|
| Working memory (`memory.rs`) | one run, last 24 results | still recorded, not sent: the transcript carries them |
| `summarize_history` (`history.rs`) | one run, folds evicted excerpts | replaced by compaction |
| `read_output` | one run, last 8 clipped command outputs | unchanged |
| `remember` / workspace notes | across runs, curated facts | unchanged |
| `past_runs` | across runs | unchanged |
| `rex compact` / `rex clear` | across a session's chain of runs | unchanged |

## What is verified, and what is not

Verified by tests in this repository: the engine's arithmetic and tiers; the
transcript's replay after a simulated crash; pairing repair on all three wire
shapes; BM25 ranking and staleness; and, through the real loop against a scripted
transport, a stub-and-recall run, model summaries, the mechanical fallback,
overflow recovery (and giving up), resume, and **forty-turn runs on the Gemini,
Anthropic, OpenAI-compatible and OpenAI Responses (ChatGPT plan) shapes whose
every request is checked for the invariants those APIs enforce** (first message
from the user, every call answered by its results, unique ids; on the Responses
route also `store: false`, `stream: true`, no system item, none of the fields the
route rejects, tools in one namespace, reasoning replayed in front of its call).
The Responses run also checks that a fresh access token is presented every turn. A mutation check confirmed those tests fail when the
opening user message is removed.

**Not verified:**

* **The Responses (ChatGPT plan) shape has never met the live route**, which is
  closed until REX is open-sourced (`docs/chatgpt-sign-in.md`). Its request and
  stream handling follow OpenAI's preview documentation and are tested against a
  scripted endpoint only.

* **No live provider was called.** This was built in a sandbox with no provider
  keys. The Anthropic `cache_control` placement, Gemini replaying earlier turns
  with their `thoughtSignature`, and the overflow phrases (paraphrased from each
  provider's documented errors, not captured) all follow the documented shapes but
  have not met the real services. A phrase that is not recognised just means that
  provider's overflow ends the run with its own error, as before.
* **Quality is unmeasured.** Nothing here shows a windowed run finishes tasks
  better, or a model summary keeps what matters. The mechanism is sound and
  tested; the benefit is a hypothesis to measure with `rex-bench`.
* **Sizes are one sandbox's measurements.** About 27 KB of fixed request and about
  4.6 KB per `read_file` result reached the model here. REX clips tool results and
  caps a run at 64 steps, so on a window of 200k or more a run usually fits whole
  and stubbing is the tier that matters; compaction matters on windows from 29k to
  roughly 128k, or if the step cap is raised.
* The mechanical summary and the stale flag do not see files rewritten by
  `run_command`.
* Windowed mode is not wired into `rex-mcp`, where the host agent owns its own
  context, nor into the explorer sub-agents, whose raw output already stays out of
  the parent's context.

## Two fixes that came with it

* A lean run could not resume: `resume` rebuilt the full tool contract while the
  run had begun under the lean one, and the prompt-identity gate refused the
  mismatch. `begin` and `resume` now share one scope function (a test fails without
  it).
* An Anthropic response's cache tokens are now counted in `usage_tokens`; they are
  zero unless a request asked for caching, so nothing changes for existing runs.
