//! Context management for long runs: the "windowed" mode.
//!
//! # Why this exists
//!
//! The default REX loop is stateless per turn. Each request carries the
//! system prompt, one fresh state message and only the previous turn's tool
//! exchange, so a run can never overflow a window, at the price of
//! continuity: a file read three turns ago is a one-line digest entry and the
//! model re-reads it. That is the right trade for the small free-tier models
//! the lean profile targets. It is the wrong one for a model with a 200k or
//! 1M window that can hold the whole working set.
//!
//! Windowed mode keeps a real transcript and manages it against the model's
//! actual window. It is opt-in (`RunOptions::windowed_context`) and leaves
//! the default path byte-for-byte unchanged.
//!
//! # Architecture
//!
//! ```text
//!                       +--------------------------+
//!   model replies  -->  |  log.jsonl   (lossless)  |  append-only, never rewritten
//!   + tool results      +------------+-------------+
//!                                    |  projected by
//!                       +------------v-------------+
//!                       |  view.jsonl  (events)    |  stub watermark, summaries
//!                       +------------+-------------+
//!                                    |
//!        rendered request  =  system + tools
//!                           + [ turns <= compacted : summary (in the state message) ]
//!                           + [ turns <= watermark : stubbed ]
//!                           + [ newest turns       : whole   ]
//!                           + state message (plan, budget, notes, summary, recalled)
//!
//!   index.rs  BM25 over the log  -->  `recall` tool, and proactive recall
//! ```
//!
//! | file | job |
//! |---|---|
//! | `limits.rs` | what the window is, and how sure we are |
//! | `estimate.rs` | bytes to tokens, calibrated by provider usage |
//! | `policy.rs` | thresholds, protected tail, budgets |
//! | `wire.rs` | the three provider message shapes, pairing repair |
//! | `transcript.rs` | the lossless log and the lossy view |
//! | `compaction.rs` | summarizer prompt, sanitising, mechanical fallback, breaker |
//! | `index.rs` | BM25 retrieval |
//! | `overflow.rs` | recognising a provider's context-overflow error |
//!
//! # The tiers
//!
//! 0. **Stub** (Elevated, about 55% of the input budget). Old tool results
//!    and bulky arguments become one-line stubs. No model call, nothing is
//!    lost (the log keeps it, `recall` brings it back). The watermark moves
//!    in a batch, never per turn, so a prompt cache is disturbed once, not
//!    every turn.
//! 1. **Summarize** (High, about 75%). The old prefix is folded into a
//!    rolling, structured summary by one tool-free model call per chunk.
//! 2. **Mechanical** fallback. If the model call fails, is refused, is empty,
//!    or three in a row have failed, a summary is built from the log by
//!    code. Compaction can always happen.
//! 3. **Emergency** (Critical, about 90%, or a provider overflow error). The
//!    protected tail shrinks to one turn and the retry is immediate.
//!
//! The newest turns (at least three, up to 15% of the budget) are never
//! touched, and turns are only ever removed whole, so a tool call is never
//! separated from its result.

pub(crate) mod compaction;
pub(crate) mod estimate;
pub(crate) mod index;
pub(crate) mod limits;
pub(crate) mod overflow;
pub(crate) mod policy;
pub(crate) mod transcript;
pub(crate) mod wire;

use crate::providers::ProviderProtocol;
use compaction::Breaker;
use estimate::TokenEstimator;
use policy::{ContextPolicy, Pressure};
use serde::Serialize;
use serde_json::{json, Value};
use std::path::PathBuf;
use transcript::{RenderOpts, SummaryMode, SummaryRecord, Transcript};

/// Largest slice of one result `recall` returns at once, in characters.
const RECALL_PAGE_CHARS: usize = 12_000;
/// Passages `recall` returns for a query at most.
const RECALL_MAX_HITS: usize = 8;
/// Passages offered unasked in the state message at most.
const PROACTIVE_HITS: usize = 3;
/// Characters of each unasked passage.
const PROACTIVE_EXCERPT_CHARS: usize = 700;
/// A passage must score at least this to be offered unasked.
const PROACTIVE_MIN_SCORE: f64 = 1.5;
/// Share of the budget a stub pass must free to be worth disturbing the
/// prompt cache for, while pressure is only Elevated. Moving the watermark
/// changes every message after it, so a provider's prompt cache is rebuilt
/// from there; doing that every turn (one turn leaves the protected tail
/// each turn) would spend the saving on cache misses. Waiting for this much
/// to accumulate makes the watermark move in batches. At High and above any
/// gain is taken.
const MIN_STUB_GAIN: f64 = 0.10;

pub const RECALL_NOTE: &str = "Passages of earlier tool results that are no longer shown in full. They are the original text, but the files may have changed since: an item marked stale_after_turn was modified later, so read the file again before relying on it.";

/// What the engine asks the loop to do about pressure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Action {
    /// Show turns up to `upto` stubbed.
    Stub { upto: usize },
    /// Fold turns up to `upto` into a summary.
    Compact { upto: usize, emergency: bool },
}

/// What the UI and the ledger can show about the engine.
#[derive(Debug, Clone, Serialize, Default, PartialEq)]
pub struct ContextStats {
    pub window: u64,
    pub input_budget: u64,
    /// `override`, `table` or `default`: how the window was found.
    pub window_source: String,
    pub estimated_tokens: u64,
    pub pressure: String,
    pub turns_recorded: usize,
    pub turns_stubbed: usize,
    pub turns_folded: usize,
    pub compactions: u32,
    pub model_compactions: u32,
    pub mechanical_compactions: u32,
    pub overflow_recoveries: u32,
    pub summary_chars: usize,
    pub bytes_per_token: f64,
    /// Provider usage reports the token estimate has been corrected by. Zero
    /// means the ratio is still the built-in default.
    pub calibration_samples: u32,
    /// Recorded turns whose call/result pairing had to be repaired before it
    /// was safe to replay. Anything above zero means a provider or a loop
    /// path produced an unmatched call or result.
    pub repaired_pairs: usize,
}

/// The turns one compaction folds, ready for the loop to summarize.
pub(crate) struct SummaryJob {
    pub first_turn: usize,
    /// Rendered turns, grouped so no group is oversized.
    pub chunks: Vec<String>,
    pub previous: Option<String>,
}

pub(crate) struct ContextEngine {
    policy: ContextPolicy,
    estimator: TokenEstimator,
    transcript: Transcript,
    breaker: Breaker,
    recall_tool: bool,
    overflow_recoveries: u32,
    last_estimate: u64,
}

impl ContextEngine {
    /// Open the engine under `dir`, replaying what an earlier drive left.
    pub fn open(
        protocol: ProviderProtocol,
        dir: PathBuf,
        policy: ContextPolicy,
        recall_tool: bool,
        max_turn: usize,
    ) -> Self {
        let transcript = Transcript::open(protocol, dir, max_turn);
        let estimator = transcript
            .saved_ratio()
            .map(TokenEstimator::with_ratio)
            .unwrap_or_default();
        Self {
            policy,
            estimator,
            transcript,
            breaker: Breaker::default(),
            recall_tool,
            overflow_recoveries: 0,
            last_estimate: 0,
        }
    }

    pub fn policy(&self) -> &ContextPolicy {
        &self.policy
    }

    #[cfg(test)]
    pub fn transcript(&self) -> &Transcript {
        &self.transcript
    }

    pub fn summary(&self) -> Option<&SummaryRecord> {
        self.transcript.view().summary.as_ref()
    }

    fn opts(&self) -> RenderOpts {
        RenderOpts {
            recall: self.recall_tool,
            // 3.5 chars per token is the estimator's own starting ratio
            result_cap_chars: (self.policy.result_cap_tokens() as f64 * 3.5) as usize,
        }
    }

    /// The history messages a request carries, oldest first.
    pub fn history(&self) -> Vec<Value> {
        self.transcript.render(&self.opts())
    }

    pub fn estimate_tokens(&self, request_bytes: usize) -> u64 {
        self.estimator.tokens(request_bytes)
    }

    pub fn record_turn(
        &mut self,
        turn: usize,
        model_parts: Vec<Value>,
        response_parts: Vec<Value>,
    ) {
        self.transcript.record(turn, model_parts, response_parts);
    }

    /// Learn from a response: `request_bytes` was counted as
    /// `reported_prompt_tokens` by the provider.
    pub fn calibrate(&mut self, request_bytes: usize, reported_prompt_tokens: u64) {
        if self
            .estimator
            .calibrate(request_bytes, reported_prompt_tokens)
        {
            self.transcript.save_ratio(self.estimator.ratio());
        }
    }

    /// A provider rejected a request estimated at `estimated_tokens` as too
    /// long: count the recovery and plan around a smaller window from now on.
    pub fn note_overflow(&mut self, estimated_tokens: u64) {
        self.overflow_recoveries += 1;
        self.policy = self.policy.tightened_to(estimated_tokens);
    }

    /// The newest turn the protected tail does not cover: every turn up to
    /// it may be shed. `0` when the tail covers everything shown.
    fn tail_boundary(&self, min_turns: usize, budget_tokens: u64) -> usize {
        let opts = self.opts();
        let view = self.transcript.view();
        let mut kept = 0usize;
        let mut acc = 0u64;
        let mut boundary = view.compacted_upto;
        let recs: Vec<_> = self
            .transcript
            .turns()
            .iter()
            .filter(|r| r.turn > view.compacted_upto)
            .collect();
        for rec in recs.iter().rev() {
            let cost = self
                .estimator
                .tokens(self.transcript.turn_bytes(rec, false, &opts));
            if kept < min_turns || acc + cost <= budget_tokens {
                kept += 1;
                acc += cost;
            } else {
                boundary = rec.turn;
                break;
            }
        }
        boundary
    }

    /// Tokens stubbing every unstubbed turn up to `upto` would free.
    fn stub_gain(&self, upto: usize) -> u64 {
        let opts = self.opts();
        let view = self.transcript.view();
        self.transcript
            .turns()
            .iter()
            .filter(|r| {
                r.turn > view.stubbed_upto && r.turn > view.compacted_upto && r.turn <= upto
            })
            .map(|r| {
                let full = self.transcript.turn_bytes(r, false, &opts);
                let stub = self.transcript.turn_bytes(r, true, &opts);
                self.estimator.tokens(full.saturating_sub(stub))
            })
            .sum()
    }

    /// What to do about a request estimated at `estimated` tokens at the
    /// start of turn `turn`. `None` means send it as it is.
    pub fn plan(&self, estimated: u64, turn: usize) -> Option<Action> {
        let pressure = self.policy.pressure(estimated);
        if pressure == Pressure::Low {
            return None;
        }
        let view = self.transcript.view();
        let budget = self.policy.input_budget();

        // The cheapest tier first, at every level of pressure: stubbing
        // costs no model call and loses nothing. It uses the normal tail.
        let stub_to = self.tail_boundary(self.policy.tail_turns, self.policy.tail_budget());
        let gain = self.stub_gain(stub_to);
        let worth_stubbing = gain > 0
            && (pressure >= Pressure::High || gain as f64 >= budget as f64 * MIN_STUB_GAIN);
        let stub = (stub_to > view.stubbed_upto && worth_stubbing)
            .then_some(Action::Stub { upto: stub_to });
        if pressure == Pressure::Elevated {
            return stub;
        }
        // High or Critical: stubbing is enough only if it brings the
        // request back under the clear threshold.
        let after = estimated.saturating_sub(gain);
        if stub.is_some() && after <= self.policy.threshold_tokens(self.policy.clear_at) {
            return stub;
        }
        let critical = pressure == Pressure::Critical;
        if !critical && self.breaker.too_soon(turn) {
            return stub; // just compacted: shed what is cheap, do not summarise again
        }
        // Summarise. An emergency, or compactions that keep freeing too
        // little, mean the protected tail itself has to shrink.
        let shrink = critical || self.breaker.tail_must_shrink();
        let min_turns = if shrink { 1 } else { self.policy.tail_turns };
        let tail_budget = if critical {
            self.policy.tail_budget() / 2
        } else {
            self.policy.tail_budget()
        };
        let boundary = self.tail_boundary(min_turns, tail_budget);
        if boundary <= view.compacted_upto {
            return stub; // nothing outside the tail to fold
        }
        Some(Action::Compact {
            upto: boundary,
            emergency: critical,
        })
    }

    pub fn apply_stub(&mut self, upto: usize) {
        self.transcript.set_stub_watermark(upto);
    }

    /// The turns a compaction up to `upto` would fold.
    pub fn summary_job(&self, upto: usize) -> SummaryJob {
        let view = self.transcript.view();
        let turns: Vec<_> = self
            .transcript
            .turns()
            .iter()
            .filter(|r| r.turn > view.compacted_upto && r.turn <= upto)
            .collect();
        SummaryJob {
            first_turn: turns.first().map_or(upto, |r| r.turn),
            chunks: compaction::chunk_turns(
                self.transcript.protocol(),
                &turns,
                self.policy.summary_chunk_chars(),
            ),
            previous: view.summary.as_ref().map(|s| s.text.clone()),
        }
    }

    /// A summary of the turns up to `upto`, built without a model.
    pub fn mechanical(&self, upto: usize, plan_json: &str, files_written: &[String]) -> String {
        let view = self.transcript.view();
        let turns: Vec<_> = self
            .transcript
            .turns()
            .iter()
            .filter(|r| r.turn > view.compacted_upto && r.turn <= upto)
            .collect();
        compaction::mechanical_summary(
            self.transcript.protocol(),
            view.summary.as_ref().map(|s| s.text.as_str()),
            &turns,
            plan_json,
            files_written,
            self.policy.summary_max_chars(),
        )
    }

    pub fn breaker(&self) -> &Breaker {
        &self.breaker
    }

    pub fn breaker_mut(&mut self) -> &mut Breaker {
        &mut self.breaker
    }

    /// Fold turns up to `upto` into `text`. `before` and `after` are the
    /// estimated request tokens around the compaction.
    pub fn finish_compaction(
        &mut self,
        text: String,
        upto: usize,
        mode: SummaryMode,
        turn: usize,
        before: u64,
        after: impl FnOnce(&Self) -> u64,
    ) -> SummaryRecord {
        let generation = self.summary().map_or(0, |s| s.generation) + 1;
        let rec = compaction::record(text, upto, generation, mode);
        self.transcript.apply_compaction(rec.clone());
        match mode {
            SummaryMode::Model => self.breaker.note_model_ok(),
            SummaryMode::Mechanical => self.breaker.note_mechanical(),
        }
        let after_tokens = after(self);
        self.breaker
            .note_effect(turn, before, after_tokens, self.policy.input_budget());
        rec
    }

    /// The `recall` tool: page a turn back in, search the archive, or list
    /// what is archived. Returns `(ok, JSON text)`.
    pub fn recall(&self, args: &Value) -> (bool, String) {
        let t = &self.transcript;
        let turn = args.get("turn").and_then(Value::as_u64).map(|n| n as usize);
        let query = args
            .get("query")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|q| !q.is_empty());
        if let Some(turn) = turn {
            let Some(rec) = t.get(turn) else {
                return (
                    false,
                    format!(
                        "no turn {turn} in this run; turns 1 to {} are recorded",
                        t.last_turn()
                    ),
                );
            };
            let call = args
                .get("call")
                .and_then(Value::as_u64)
                .map(|n| n as usize)
                .or((rec.calls.len() == 1).then_some(0));
            let Some(call) = call else {
                return (
                    true,
                    json!({
                        "turn": turn,
                        "calls": rec.calls.iter().enumerate().map(|(i, c)| json!({
                            "call": i, "tool": c.name, "target": c.target,
                            "ok": c.ok, "chars": c.result_chars,
                        })).collect::<Vec<_>>(),
                        "note": "this turn made several calls; pass call to read one result",
                    })
                    .to_string(),
                );
            };
            let Some((note, text)) = t.result_text(turn, call) else {
                return (
                    false,
                    format!(
                        "turn {turn} has no call {call}; it made {} call(s)",
                        rec.calls.len()
                    ),
                );
            };
            let offset = args.get("offset").and_then(Value::as_u64).unwrap_or(0) as usize;
            let total = text.chars().count();
            let page: String = text.chars().skip(offset).take(RECALL_PAGE_CHARS).collect();
            let end = offset + page.chars().count();
            let stale = note
                .target
                .as_deref()
                .filter(|_| note.name == "read_file")
                .and_then(|p| t.changed_after(turn, p));
            return (
                true,
                json!({
                    "turn": turn, "call": call, "tool": note.name,
                    "target": note.target, "ok": note.ok,
                    "stale_after_turn": stale,
                    "offset": offset, "total_chars": total,
                    "next_offset": (end < total).then_some(end),
                    "text": page,
                })
                .to_string(),
            );
        }
        if let Some(q) = query {
            let limit = args
                .get("limit")
                .and_then(Value::as_u64)
                .map_or(5, |n| (n as usize).clamp(1, RECALL_MAX_HITS));
            let shown_from = t.view().stubbed_upto;
            let hits = t.index().search(q, limit * 2, |m| m.turn <= shown_from);
            // two passages of one result are enough
            let mut per_call: std::collections::HashMap<(usize, usize), usize> = Default::default();
            let items: Vec<Value> = hits
                .into_iter()
                .filter(|h| {
                    let n = per_call.entry((h.meta.turn, h.meta.call)).or_insert(0);
                    *n += 1;
                    *n <= 2
                })
                .take(limit)
                .map(|h| self.hit_json(&h, None))
                .collect();
            return (
                true,
                json!({
                    "query": q,
                    "count": items.len(),
                    "items": items,
                    "note": if items.is_empty() {
                        "nothing archived matches. Turns still shown in full are already in the conversation."
                    } else {
                        RECALL_NOTE
                    },
                })
                .to_string(),
            );
        }
        (
            true,
            json!({
                "archived_turns": t.outline(60),
                "summary_covers_turns_up_to": t.view().compacted_upto,
                "note": "pass query to search these, or turn (and call) to read one result in full",
            })
            .to_string(),
        )
    }

    fn hit_json(&self, h: &index::Hit<'_>, max_chars: Option<usize>) -> Value {
        let t = &self.transcript;
        let stale = h
            .meta
            .target
            .as_deref()
            .filter(|_| h.meta.tool == "read_file")
            .and_then(|p| t.changed_after(h.meta.turn, p));
        let excerpt: String = match max_chars {
            Some(n) => h.text.chars().take(n).collect(),
            None => h.text.to_string(),
        };
        json!({
            "turn": h.meta.turn, "call": h.meta.call, "tool": h.meta.tool,
            "target": h.meta.target, "ok": h.meta.ok, "offset": h.meta.offset,
            "stale_after_turn": stale,
            "excerpt": excerpt,
        })
    }

    /// Passages worth showing without being asked, for the state message:
    /// the best matches for `query` among archived turns. `None` when
    /// nothing is archived or nothing scores well enough.
    pub fn proactive(&self, query: &str) -> Option<Value> {
        let t = &self.transcript;
        let shown_from = t.view().stubbed_upto;
        if shown_from == 0 || query.trim().is_empty() {
            return None;
        }
        let hits: Vec<_> = t
            .index()
            .search(query, PROACTIVE_HITS, |m| m.turn <= shown_from)
            .into_iter()
            .filter(|h| h.score >= PROACTIVE_MIN_SCORE)
            .collect();
        if hits.is_empty() {
            return None;
        }
        Some(json!({
            "note": RECALL_NOTE,
            "items": hits.iter().map(|h| self.hit_json(h, Some(PROACTIVE_EXCERPT_CHARS))).collect::<Vec<_>>(),
        }))
    }

    /// A compact description of what a request carries from history, for
    /// the evidence folder (the full text is in `log.jsonl`).
    pub fn manifest(&self, estimated_tokens: u64) -> Value {
        let view = self.transcript.view();
        json!({
            "windowed": true,
            "estimated_input_tokens": estimated_tokens,
            "pressure": self.policy.pressure(estimated_tokens).label(),
            "shown": self.transcript.visible().iter().map(|(turn, stubbed)| json!({"turn": turn, "stubbed": stubbed})).collect::<Vec<_>>(),
            "summary": view.summary.as_ref().map(|s| json!({
                "generation": s.generation, "covers_upto": s.covers_upto,
                "mode": s.mode, "fingerprint": s.fingerprint, "chars": s.text.chars().count(),
            })),
            "log": "context/log.jsonl",
        })
    }

    pub fn stats(&mut self, estimated_tokens: u64) -> ContextStats {
        self.last_estimate = estimated_tokens;
        let view = self.transcript.view();
        ContextStats {
            window: self.policy.window,
            input_budget: self.policy.input_budget(),
            window_source: match self.policy.source {
                limits::LimitSource::Override => "override",
                limits::LimitSource::Table => "table",
                limits::LimitSource::Default => "default",
            }
            .to_string(),
            estimated_tokens,
            pressure: self.policy.pressure(estimated_tokens).label().to_string(),
            turns_recorded: self.transcript.turns().len(),
            turns_stubbed: self
                .transcript
                .turns()
                .iter()
                .filter(|r| r.turn > view.compacted_upto && r.turn <= view.stubbed_upto)
                .count(),
            turns_folded: self
                .transcript
                .turns()
                .iter()
                .filter(|r| r.turn <= view.compacted_upto)
                .count(),
            compactions: self.breaker.total_model + self.breaker.total_mechanical,
            model_compactions: self.breaker.total_model,
            mechanical_compactions: self.breaker.total_mechanical,
            overflow_recoveries: self.overflow_recoveries,
            summary_chars: view.summary.as_ref().map_or(0, |s| s.text.chars().count()),
            bytes_per_token: self.estimator.ratio(),
            calibration_samples: self.estimator.samples(),
            repaired_pairs: self.transcript.repairs(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use limits::{LimitSource, ModelLimits, DEFAULT_MAX_OUTPUT};
    use wire::make_result;
    use ProviderProtocol::{Anthropic, Gemini, OpenAiCompatible};

    fn policy(window: u64) -> ContextPolicy {
        ContextPolicy::for_limits(&ModelLimits {
            context_window: window,
            max_output: DEFAULT_MAX_OUTPUT,
            source: LimitSource::Override,
        })
        .unwrap()
    }

    fn engine(p: ProviderProtocol, window: u64) -> ContextEngine {
        let tmp = tempfile::tempdir().unwrap();
        // the tempdir is dropped, so the engine runs against a missing
        // folder: persistence is best effort and the engine must not care
        ContextEngine::open(
            p,
            tmp.path().join("gone/context"),
            policy(window),
            true,
            999,
        )
    }

    fn add(e: &mut ContextEngine, p: ProviderProtocol, n: usize, chars: usize) {
        let id = format!("id{n}");
        let body = format!("fn item{n}() {{}} unique_marker_{n}\n{}", "x".repeat(chars));
        let (m, r) = match p {
            Anthropic => (
                vec![
                    json!({"type":"tool_use","id":id,"name":"read_file","input":{"path":format!("src/m{n}.rs")}}),
                ],
                vec![make_result(p, "read_file", &id, true, &body)],
            ),
            Gemini => (
                vec![
                    json!({"functionCall":{"name":"read_file","args":{"path":format!("src/m{n}.rs")}}}),
                ],
                vec![make_result(p, "read_file", "", true, &body)],
            ),
            OpenAiCompatible => (
                vec![
                    json!({"id":id,"type":"function","function":{"name":"read_file","arguments":json!({"path":format!("src/m{n}.rs")}).to_string()}}),
                ],
                vec![make_result(p, "read_file", &id, true, &body)],
            ),
        };
        e.record_turn(n, m, r);
    }

    fn est(e: &ContextEngine) -> u64 {
        let bytes: usize = e.history().iter().map(|m| m.to_string().len()).sum();
        e.estimate_tokens(bytes)
    }

    #[test]
    fn below_the_clear_threshold_nothing_happens() {
        let mut e = engine(Anthropic, 200_000);
        for n in 1..=4 {
            add(&mut e, Anthropic, n, 1_000);
        }
        assert_eq!(e.plan(est(&e), 5), None);
    }

    #[test]
    fn elevated_pressure_stubs_the_old_turns_and_keeps_the_tail() {
        let mut e = engine(Anthropic, 32_768);
        for n in 1..=12 {
            add(&mut e, Anthropic, n, 3_000);
        }
        let b = e.policy().input_budget();
        let at = e.policy().threshold_tokens(0.6);
        assert!(at < b);
        let a = e.plan(at, 13).expect("stubbing is warranted");
        match a {
            Action::Stub { upto } => {
                assert!(
                    upto >= 1 && upto <= 12 - e.policy().tail_turns,
                    "tail of {} kept, upto {upto}",
                    e.policy().tail_turns
                );
                e.apply_stub(upto);
            }
            other => panic!("expected a stub, got {other:?}"),
        }
        let visible = e.transcript().visible();
        let newest: Vec<_> = visible.iter().rev().take(3).collect();
        assert!(
            newest.iter().all(|(_, stubbed)| !stubbed),
            "the tail stays whole"
        );
        assert_eq!(
            e.plan(at, 13),
            None,
            "a second call has nothing new to stub"
        );
    }

    /// 80 turns whose results are each under the stub threshold: nothing
    /// is worth stubbing, so only a summary can make room.
    fn small_results(p: ProviderProtocol) -> ContextEngine {
        let mut e = engine(p, 32_768);
        for n in 1..=80 {
            add(&mut e, p, n, 400);
        }
        e
    }

    #[test]
    fn high_pressure_prefers_stubbing_when_it_frees_enough() {
        let mut e = engine(Gemini, 32_768);
        for n in 1..=20 {
            add(&mut e, Gemini, n, 3_000);
        }
        let high = e.policy().threshold_tokens(0.8);
        match e.plan(high, 21) {
            Some(Action::Stub { upto }) => assert!((1..20).contains(&upto)),
            other => panic!("stubbing 17 bulky results frees plenty, got {other:?}"),
        }
    }

    #[test]
    fn critical_pressure_still_takes_the_lossless_tier_when_it_suffices() {
        let mut e = engine(Anthropic, 32_768);
        for n in 1..=20 {
            add(&mut e, Anthropic, n, 3_000);
        }
        let crit = e.policy().threshold_tokens(0.95);
        assert!(
            matches!(e.plan(crit, 21), Some(Action::Stub { .. })),
            "no model call is needed when stubbing alone clears the pressure"
        );
    }

    #[test]
    fn high_pressure_compacts_when_stubbing_cannot_free_enough() {
        let e = small_results(Gemini);
        let high = e.policy().threshold_tokens(0.8);
        match e.plan(high, 81) {
            Some(Action::Compact { upto, emergency }) => {
                assert!(!emergency);
                assert!((1..80).contains(&upto));
            }
            other => panic!("expected compaction, got {other:?}"),
        }
    }

    #[test]
    fn critical_pressure_is_an_emergency_with_a_smaller_tail() {
        let e = small_results(OpenAiCompatible);
        let normal = match e.plan(e.policy().threshold_tokens(0.8), 81) {
            Some(Action::Compact { upto, .. }) => upto,
            other => panic!("{other:?}"),
        };
        let crit = e.policy().threshold_tokens(0.95);
        match e.plan(crit, 81) {
            Some(Action::Compact { upto, emergency }) => {
                assert!(emergency);
                assert!(upto >= normal, "an emergency folds at least as much");
                assert!(upto <= 79, "one turn is always kept");
            }
            other => panic!("expected an emergency, got {other:?}"),
        }
    }

    #[test]
    fn nothing_is_shed_when_the_tail_covers_everything() {
        let mut e = engine(Anthropic, 32_768);
        add(&mut e, Anthropic, 1, 10);
        add(&mut e, Anthropic, 2, 10);
        let crit = e.policy().threshold_tokens(0.95);
        // pressure comes from the state message, not the history: there is
        // nothing outside the tail to fold
        assert_eq!(e.plan(crit, 3), None);
    }

    #[test]
    fn compaction_replaces_the_prefix_and_shrinks_the_request() {
        let mut e = engine(Anthropic, 32_768);
        for n in 1..=12 {
            add(&mut e, Anthropic, n, 3_000);
        }
        let before = est(&e);
        let job = e.summary_job(9);
        assert_eq!(job.first_turn, 1);
        assert!(job.previous.is_none() && !job.chunks.is_empty());
        let rec = e.finish_compaction(
            "TASK\nread many files\nNEXT\nkeep going".into(),
            9,
            SummaryMode::Model,
            13,
            before,
            est,
        );
        assert_eq!(rec.generation, 1);
        assert!(est(&e) < before / 2, "{} vs {before}", est(&e));
        assert_eq!(e.summary().unwrap().covers_upto, 9);
        assert_eq!(e.stats(est(&e)).turns_folded, 9);
        // a second compaction anchors on the first
        let job2 = e.summary_job(11);
        assert_eq!(
            job2.previous.as_deref(),
            Some("TASK\nread many files\nNEXT\nkeep going")
        );
        assert_eq!(job2.first_turn, 10);
        let rec2 = e.finish_compaction(
            "second".into(),
            11,
            SummaryMode::Mechanical,
            20,
            before,
            est,
        );
        assert_eq!(rec2.generation, 2);
    }

    #[test]
    fn right_after_a_compaction_high_pressure_stubs_instead_of_compacting_again() {
        let mut e = engine(Anthropic, 32_768);
        for n in 1..=12 {
            add(&mut e, Anthropic, n, 3_000);
        }
        e.finish_compaction("s".repeat(200), 6, SummaryMode::Model, 13, 20_000, |_| {
            5_000
        });
        let high = e.policy().threshold_tokens(0.8);
        let a = e.plan(high, 14);
        assert!(
            !matches!(a, Some(Action::Compact { .. })),
            "one turn after a compaction must not summarise again: {a:?}"
        );
    }

    #[test]
    fn recall_pages_tile_the_whole_result() {
        let mut e = engine(Anthropic, 32_768);
        let body = format!("{}END", "line of text\n".repeat(2_000));
        e.record_turn(
            1,
            vec![json!({"type":"tool_use","id":"a","name":"read_file","input":{"path":"big.txt"}})],
            vec![make_result(Anthropic, "read_file", "a", true, &body)],
        );
        let mut collected = String::new();
        let mut offset = 0u64;
        let mut pages = 0;
        loop {
            let (ok, text) = e.recall(&json!({"turn": 1, "offset": offset}));
            assert!(ok);
            let v: Value = serde_json::from_str(&text).unwrap();
            assert_eq!(v["offset"].as_u64().unwrap(), offset);
            assert_eq!(v["tool"], "read_file");
            assert_eq!(
                v["total_chars"].as_u64().unwrap() as usize,
                body.chars().count()
            );
            collected.push_str(v["text"].as_str().unwrap());
            pages += 1;
            match v["next_offset"].as_u64() {
                Some(n) => offset = n,
                None => break,
            }
        }
        assert!(pages >= 3, "a 26k result pages in 12k slices, got {pages}");
        assert_eq!(
            collected, body,
            "the pages must reproduce the result exactly"
        );
    }

    #[test]
    fn recall_errors_say_what_is_wrong() {
        let mut e = engine(Gemini, 32_768);
        add(&mut e, Gemini, 1, 100);
        let (ok, msg) = e.recall(&json!({"turn": 9}));
        assert!(!ok && msg.contains("turns 1 to 1"));
        let (ok, msg) = e.recall(&json!({"turn": 1, "call": 4}));
        assert!(!ok && msg.contains("no call 4"));
        let (ok, text) = e.recall(&json!({}));
        assert!(ok);
        let v: Value = serde_json::from_str(&text).unwrap();
        assert!(v["archived_turns"].is_array());
    }

    #[test]
    fn a_turn_with_several_calls_asks_which_one() {
        let mut e = engine(Anthropic, 32_768);
        e.record_turn(
            1,
            vec![
                json!({"type":"tool_use","id":"a","name":"read_file","input":{"path":"x"}}),
                json!({"type":"tool_use","id":"b","name":"glob_files","input":{"pattern":"*"}}),
            ],
            vec![
                make_result(Anthropic, "read_file", "a", true, "X"),
                make_result(Anthropic, "glob_files", "b", true, "Y"),
            ],
        );
        let (ok, text) = e.recall(&json!({"turn": 1}));
        assert!(ok);
        let v: Value = serde_json::from_str(&text).unwrap();
        assert_eq!(v["calls"].as_array().unwrap().len(), 2);
        let (_, one) = e.recall(&json!({"turn": 1, "call": 1}));
        assert!(one.contains("\"text\":\"Y\""));
    }

    #[test]
    fn searching_finds_archived_content_only() {
        let mut e = engine(Anthropic, 32_768);
        for n in 1..=6 {
            add(&mut e, Anthropic, n, 200);
        }
        let none = e.recall(&json!({"query": "unique_marker_2"}));
        assert!(
            none.1.contains("\"count\":0"),
            "turns still shown in full are not archived: {}",
            none.1
        );
        e.apply_stub(4);
        let (ok, text) = e.recall(&json!({"query": "unique_marker_2"}));
        assert!(ok);
        let v: Value = serde_json::from_str(&text).unwrap();
        assert!(v["count"].as_u64().unwrap() >= 1);
        assert_eq!(v["items"][0]["turn"], 2);
        // Turns 5 and 6 are still shown in full, so their text is never
        // offered. (The query shares sub-words with the archived turns, so
        // those may match; what must not appear is a shown turn.)
        let (_, text) = e.recall(&json!({"query": "unique_marker_6", "limit": 8}));
        let v: Value = serde_json::from_str(&text).unwrap();
        for item in v["items"].as_array().unwrap() {
            assert!(item["turn"].as_u64().unwrap() <= 4, "{item}");
        }
    }

    #[test]
    fn a_read_that_a_later_edit_changed_is_marked_stale() {
        let mut e = engine(Gemini, 32_768);
        add(&mut e, Gemini, 1, 100);
        e.record_turn(
            2,
            vec![json!({"functionCall":{"name":"edit_file","args":{"path":"src/m1.rs","expected":"a","replacement":"b","replace_all":false}}})],
            vec![make_result(Gemini, "edit_file", "", true, "{\"ok\":true}")],
        );
        add(&mut e, Gemini, 3, 100);
        e.apply_stub(2);
        let (_, text) = e.recall(&json!({"query": "unique_marker_1"}));
        let v: Value = serde_json::from_str(&text).unwrap();
        assert_eq!(v["items"][0]["stale_after_turn"], 2);
        let (_, text) = e.recall(&json!({"turn": 1}));
        let v: Value = serde_json::from_str(&text).unwrap();
        assert_eq!(v["stale_after_turn"], 2);
    }

    #[test]
    fn proactive_recall_needs_archived_content_and_a_good_match() {
        let mut e = engine(Anthropic, 32_768);
        for n in 1..=6 {
            add(&mut e, Anthropic, n, 200);
        }
        assert!(
            e.proactive("unique_marker_2").is_none(),
            "nothing archived yet"
        );
        e.apply_stub(4);
        let hit = e
            .proactive("unique_marker_2 item2")
            .expect("a strong match");
        assert_eq!(hit["items"][0]["turn"], 2);
        assert!(
            hit["items"][0]["excerpt"].as_str().unwrap().chars().count() <= PROACTIVE_EXCERPT_CHARS
        );
        assert!(
            e.proactive("zebra giraffe").is_none(),
            "no match, nothing offered"
        );
        assert!(e.proactive("   ").is_none());
    }

    #[test]
    fn calibration_is_saved_and_restored_across_a_restart() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("context");
        {
            let mut e = ContextEngine::open(Anthropic, dir.clone(), policy(200_000), true, 99);
            add(&mut e, Anthropic, 1, 100);
            e.calibrate(100_000, 50_000);
            assert!(e.stats(0).bytes_per_token < 2.5);
        }
        let e = ContextEngine::open(Anthropic, dir, policy(200_000), true, 99);
        let mut e = e;
        assert!(
            e.stats(0).bytes_per_token < 2.5,
            "the learned ratio survives resume"
        );
    }

    #[test]
    fn stats_report_the_shape_of_the_run() {
        let mut e = engine(Anthropic, 32_768);
        for n in 1..=10 {
            add(&mut e, Anthropic, n, 500);
        }
        e.apply_stub(5);
        e.finish_compaction(
            "s".repeat(100),
            3,
            SummaryMode::Mechanical,
            11,
            9_000,
            |_| 4_000,
        );
        e.note_overflow(10_000);
        let s = e.stats(7_777);
        assert_eq!(s.turns_recorded, 10);
        assert_eq!(s.turns_folded, 3);
        assert_eq!(s.turns_stubbed, 2);
        assert_eq!(s.compactions, 1);
        assert_eq!(s.mechanical_compactions, 1);
        assert_eq!(s.overflow_recoveries, 1);
        assert_eq!(s.estimated_tokens, 7_777);
        assert_eq!(s.window_source, "override");
        assert_eq!(s.summary_chars, 100);
    }

    #[test]
    fn the_manifest_names_what_is_shown_without_embedding_it() {
        let mut e = engine(Gemini, 32_768);
        for n in 1..=5 {
            add(&mut e, Gemini, n, 4_000);
        }
        e.apply_stub(2);
        e.finish_compaction("s".repeat(100), 1, SummaryMode::Model, 6, 9_000, |_| 4_000);
        let m = e.manifest(1234);
        assert_eq!(m["windowed"], true);
        assert_eq!(m["summary"]["covers_upto"], 1);
        assert_eq!(m["shown"].as_array().unwrap().len(), 4);
        assert!(
            m.to_string().len() < 1_500,
            "a manifest is small however long the run is"
        );
    }
}
