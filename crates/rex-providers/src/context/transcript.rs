//! The run's transcript: a lossless log, and a lossy view of it.
//!
//! This is the central design decision of the context engine, borrowed from
//! event sourcing. Two things are stored separately:
//!
//! * **`log.jsonl`**: every turn, exactly as the provider saw it, appended
//!   once and never rewritten. This is the source of truth and the archive
//!   retrieval searches.
//! * **`view.jsonl`**: a short list of events (`stub`, `compact`, `ratio`)
//!   that say how much of the log the model is *shown*.
//!
//! What goes into a request is a pure function of the two: turns folded
//! into a summary are not rendered, turns before the stub watermark are
//! rendered with their bulky tool results and arguments replaced by short
//! stubs, the rest are rendered whole. Compaction is therefore a change of
//! projection, never a loss. A crash cannot corrupt history, and a summary
//! that turns out to be wrong cannot destroy what it summarised. Replaying
//! both files rebuilds the exact state on resume.

use super::index::{self, ChunkMeta, Index};
use super::wire;
use crate::providers::ProviderProtocol;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::PathBuf;

/// A result shorter than this is never stubbed: the stub would cost as
/// much as the result.
pub const STUB_MIN_CHARS: usize = 500;
/// String arguments longer than this are cut from stubbed turns.
pub const ARG_MAX_CHARS: usize = 400;
/// Characters of a result kept as a preview inside its stub.
const STUB_PREVIEW_CHARS: usize = 100;

/// One call and what became of it, read back out of the recorded parts.
#[derive(Debug, Clone, PartialEq)]
pub struct CallNote {
    pub id: String,
    pub name: String,
    pub target: Option<String>,
    pub ok: Option<bool>,
    pub result_chars: usize,
    /// Workspace paths a successful file-writing call changed.
    pub writes: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct TurnRecord {
    pub turn: usize,
    pub model_parts: Vec<Value>,
    pub response_parts: Vec<Value>,
    pub calls: Vec<CallNote>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SummaryMode {
    /// Written by a model call.
    Model,
    /// Built from the log by code, no model call.
    Mechanical,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SummaryRecord {
    pub text: String,
    /// Turns up to and including this one are represented by `text`.
    pub covers_upto: usize,
    /// 1 for the first compaction of the run, then 2, 3, ...
    pub generation: u32,
    pub mode: SummaryMode,
    pub fingerprint: String,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct ViewState {
    /// Turns up to this are shown only through `summary`.
    pub compacted_upto: usize,
    pub summary: Option<SummaryRecord>,
    /// Turns up to this (and above `compacted_upto`) are shown stubbed.
    pub stubbed_upto: usize,
}

/// How a request is rendered from the transcript.
#[derive(Debug, Clone, Copy)]
pub struct RenderOpts {
    /// Say in stubs that `recall` can bring a result back.
    pub recall: bool,
    /// Longest a result may be at full fidelity, in characters.
    pub result_cap_chars: usize,
}

#[derive(Serialize, Deserialize)]
struct LogLine {
    turn: usize,
    model_parts: Vec<Value>,
    response_parts: Vec<Value>,
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "ev", rename_all = "snake_case")]
enum ViewEvent {
    Stub { upto: usize },
    Compact { summary: SummaryRecord },
    Ratio { bytes_per_token: f64 },
}

pub struct Transcript {
    protocol: ProviderProtocol,
    turns: Vec<TurnRecord>,
    view: ViewState,
    index: Index,
    /// (turn, path) of every recorded file write, oldest first.
    writes: Vec<(usize, String)>,
    ratio: Option<f64>,
    repairs: usize,
    log: Option<File>,
    view_log: Option<File>,
}

/// First 16 hex digits of the SHA-256 of `text`: a short identifier for a
/// summary in the ledger.
pub fn fingerprint(text: &str) -> String {
    rex_prompt::sha256_hex(text.as_bytes())
        .chars()
        .take(16)
        .collect()
}

fn clip_chars(text: &str, max: usize, marker: impl Fn(usize) -> String) -> String {
    let n = text.chars().count();
    if n <= max {
        return text.to_string();
    }
    let head = max * 3 / 5;
    let tail = max - head;
    let h: String = text.chars().take(head).collect();
    let t: String = text.chars().skip(n - tail).collect();
    format!("{h}\n{}\n{t}", marker(n - head - tail))
}

fn ok_from_content(content: &str) -> Option<bool> {
    let v: Value = serde_json::from_str(content).ok()?;
    v.get("ok").and_then(Value::as_bool)
}

/// Paths named by an `apply_patch` body.
fn patch_paths(patch: &str) -> Vec<String> {
    patch
        .lines()
        .filter_map(|l| {
            [
                "*** Add File: ",
                "*** Update File: ",
                "*** Delete File: ",
                "*** Move to: ",
            ]
            .iter()
            .find_map(|p| l.strip_prefix(p))
        })
        .map(|p| p.trim().to_string())
        .filter(|p| !p.is_empty())
        .collect()
}

fn short(text: &str, max: usize) -> String {
    let flat: String = text
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect();
    let n = flat.chars().count();
    if n <= max {
        flat
    } else {
        format!("{}…", flat.chars().take(max).collect::<String>())
    }
}

/// What a call was about: the path, command, query or URL.
fn target_of(name: &str, args: &Value) -> Option<String> {
    let s = |k: &str| args.get(k).and_then(Value::as_str).map(str::to_string);
    match name {
        "read_file" | "create_file" | "edit_file" => s("path"),
        "apply_patch" => {
            let paths = patch_paths(args.get("patch").and_then(Value::as_str).unwrap_or(""));
            (!paths.is_empty()).then(|| paths.join(", "))
        }
        "search_files" => s("query").map(|q| short(&q, 80)),
        "glob_files" => s("pattern"),
        "run_command" => args.get("argv").and_then(Value::as_array).map(|a| {
            short(
                &a.iter()
                    .filter_map(Value::as_str)
                    .collect::<Vec<_>>()
                    .join(" "),
                120,
            )
        }),
        "web_fetch" => s("url"),
        "web_search" => s("query").map(|q| short(&q, 80)),
        "explore" => s("task")
            .or_else(|| {
                args.get("tasks")
                    .and_then(Value::as_array)
                    .and_then(|t| t.first())
                    .and_then(Value::as_str)
                    .map(str::to_string)
            })
            .map(|t| short(&t, 100)),
        "load_skill" => s("name"),
        "read_output" => s("call_id"),
        "recall" => s("query").or_else(|| args.get("turn").map(|t| format!("turn {t}"))),
        "mcp_call" => match (s("server"), s("name")) {
            (Some(a), Some(b)) => Some(format!("{a}/{b}")),
            _ => None,
        },
        _ => None,
    }
}

fn written_paths(name: &str, args: &Value, ok: Option<bool>) -> Vec<String> {
    if ok == Some(false) {
        return Vec::new();
    }
    match name {
        "create_file" | "edit_file" => args
            .get("path")
            .and_then(Value::as_str)
            .map(|p| vec![p.to_string()])
            .unwrap_or_default(),
        "apply_patch" => patch_paths(args.get("patch").and_then(Value::as_str).unwrap_or("")),
        _ => Vec::new(),
    }
}

fn derive_notes(
    protocol: ProviderProtocol,
    model_parts: &[Value],
    response_parts: &[Value],
) -> Vec<CallNote> {
    let calls = wire::calls(protocol, model_parts);
    let results = wire::results(protocol, response_parts);
    calls
        .iter()
        .enumerate()
        .map(|(i, c)| {
            let r = match protocol {
                ProviderProtocol::Gemini => results.get(i),
                _ => results.iter().find(|r| r.id == c.id),
            };
            let content = r.map(|r| r.content.as_str()).unwrap_or("");
            let ok = r.and_then(|r| r.ok).or_else(|| ok_from_content(content));
            CallNote {
                id: c.id.clone(),
                name: c.name.clone(),
                target: target_of(&c.name, &c.args),
                ok,
                result_chars: content.chars().count(),
                writes: written_paths(&c.name, &c.args, ok),
            }
        })
        .collect()
}

impl Transcript {
    /// An in-memory transcript (no files).
    pub fn new(protocol: ProviderProtocol) -> Self {
        Self {
            protocol,
            turns: Vec::new(),
            view: ViewState::default(),
            index: Index::default(),
            writes: Vec::new(),
            ratio: None,
            repairs: 0,
            log: None,
            view_log: None,
        }
    }

    /// Open (or create) the transcript under `dir` and replay what an
    /// earlier drive of the run left there. Turns numbered above
    /// `max_turn` belong to a turn whose checkpoint never landed and are
    /// ignored; unreadable lines (a torn last write) are skipped.
    pub fn open(protocol: ProviderProtocol, dir: PathBuf, max_turn: usize) -> Self {
        let mut t = Self::new(protocol);
        let _ = fs::create_dir_all(&dir);
        if let Ok(text) = fs::read_to_string(dir.join("log.jsonl")) {
            let mut by_turn: std::collections::BTreeMap<usize, LogLine> = Default::default();
            for line in text.lines() {
                if let Ok(l) = serde_json::from_str::<LogLine>(line) {
                    if l.turn <= max_turn {
                        by_turn.insert(l.turn, l); // a later line for a turn wins
                    }
                }
            }
            for (_, l) in by_turn {
                t.ingest(l.turn, l.model_parts, l.response_parts);
            }
        }
        if let Ok(text) = fs::read_to_string(dir.join("view.jsonl")) {
            for line in text.lines() {
                match serde_json::from_str::<ViewEvent>(line) {
                    Ok(ViewEvent::Stub { upto }) => t.view.stubbed_upto = upto,
                    Ok(ViewEvent::Compact { summary }) => {
                        t.view.compacted_upto = summary.covers_upto;
                        t.view.summary = Some(summary);
                    }
                    Ok(ViewEvent::Ratio { bytes_per_token }) => t.ratio = Some(bytes_per_token),
                    Err(_) => {}
                }
            }
        }
        // a view event may name turns that were dropped above
        let last = t.turns.last().map_or(0, |r| r.turn);
        t.view.compacted_upto = t.view.compacted_upto.min(last);
        t.view.stubbed_upto = t.view.stubbed_upto.min(last).max(t.view.compacted_upto);
        let open_append = |name: &str| {
            OpenOptions::new()
                .create(true)
                .append(true)
                .open(dir.join(name))
                .ok()
        };
        t.log = open_append("log.jsonl");
        t.view_log = open_append("view.jsonl");
        t
    }

    fn ingest(&mut self, turn: usize, mut model: Vec<Value>, mut response: Vec<Value>) {
        self.repairs += wire::repair_pairing(self.protocol, &mut model, &mut response);
        let calls = derive_notes(self.protocol, &model, &response);
        let results = wire::results(self.protocol, &response);
        for (i, note) in calls.iter().enumerate() {
            for p in &note.writes {
                self.writes.push((turn, p.clone()));
            }
            let content = match self.protocol {
                ProviderProtocol::Gemini => results.get(i),
                _ => results.iter().find(|r| r.id == note.id),
            }
            .map(|r| r.content.as_str())
            .unwrap_or("");
            for (offset, piece) in index::chunk(content) {
                self.index.add(
                    ChunkMeta {
                        turn,
                        call: i,
                        tool: note.name.clone(),
                        target: note.target.clone(),
                        ok: note.ok,
                        offset,
                    },
                    piece,
                );
            }
        }
        self.turns.retain(|r| r.turn != turn);
        self.turns.push(TurnRecord {
            turn,
            model_parts: model,
            response_parts: response,
            calls,
        });
        self.turns.sort_by_key(|r| r.turn);
    }

    fn append(file: &mut Option<File>, line: &str) {
        if let Some(f) = file.as_mut() {
            let _ = f.write_all(line.as_bytes());
            let _ = f.write_all(b"\n");
        }
    }

    /// Record one finished turn.
    pub fn record(&mut self, turn: usize, model_parts: Vec<Value>, response_parts: Vec<Value>) {
        self.ingest(turn, model_parts, response_parts);
        if let Some(r) = self.turns.iter().find(|r| r.turn == turn) {
            let line = LogLine {
                turn,
                model_parts: r.model_parts.clone(),
                response_parts: r.response_parts.clone(),
            };
            if let Ok(s) = serde_json::to_string(&line) {
                Self::append(&mut self.log, &s);
            }
        }
    }

    fn log_view(&mut self, ev: &ViewEvent) {
        if let Ok(s) = serde_json::to_string(ev) {
            Self::append(&mut self.view_log, &s);
        }
    }

    pub fn protocol(&self) -> ProviderProtocol {
        self.protocol
    }
    pub fn turns(&self) -> &[TurnRecord] {
        &self.turns
    }
    pub fn view(&self) -> &ViewState {
        &self.view
    }
    pub fn index(&self) -> &Index {
        &self.index
    }
    pub fn repairs(&self) -> usize {
        self.repairs
    }
    pub fn saved_ratio(&self) -> Option<f64> {
        self.ratio
    }
    pub fn last_turn(&self) -> usize {
        self.turns.last().map_or(0, |r| r.turn)
    }
    pub fn get(&self, turn: usize) -> Option<&TurnRecord> {
        self.turns.iter().find(|r| r.turn == turn)
    }

    pub fn save_ratio(&mut self, bytes_per_token: f64) {
        if self.ratio != Some(bytes_per_token) {
            self.ratio = Some(bytes_per_token);
            self.log_view(&ViewEvent::Ratio { bytes_per_token });
        }
    }

    /// Show turns up to `upto` stubbed. Never moves backwards.
    pub fn set_stub_watermark(&mut self, upto: usize) {
        let upto = upto.min(self.last_turn());
        if upto > self.view.stubbed_upto {
            self.view.stubbed_upto = upto;
            self.log_view(&ViewEvent::Stub { upto });
        }
    }

    /// Fold turns up to `summary.covers_upto` into `summary`.
    pub fn apply_compaction(&mut self, summary: SummaryRecord) {
        self.view.compacted_upto = summary.covers_upto.min(self.last_turn());
        self.view.stubbed_upto = self.view.stubbed_upto.max(self.view.compacted_upto);
        self.log_view(&ViewEvent::Compact {
            summary: summary.clone(),
        });
        self.view.summary = Some(summary);
    }

    /// Turns the request currently shows, as `(turn, stubbed)`.
    pub fn visible(&self) -> Vec<(usize, bool)> {
        self.turns
            .iter()
            .filter(|r| r.turn > self.view.compacted_upto)
            .map(|r| (r.turn, r.turn <= self.view.stubbed_upto))
            .collect()
    }

    /// The paths changed after `turn`, if `path` was among them: the turn
    /// of the latest such write. `None` means nothing recorded changed it.
    pub fn changed_after(&self, turn: usize, path: &str) -> Option<usize> {
        self.writes
            .iter()
            .filter(|(t, p)| *t > turn && crate::memory::same_path(p, path))
            .map(|(t, _)| *t)
            .max()
    }

    /// The whole text of one call's result, for paging a turn back in.
    pub fn result_text(&self, turn: usize, call: usize) -> Option<(&CallNote, String)> {
        let rec = self.get(turn)?;
        let note = rec.calls.get(call)?;
        let results = wire::results(self.protocol, &rec.response_parts);
        let r = match self.protocol {
            ProviderProtocol::Gemini => results.get(call),
            _ => results.iter().find(|r| r.id == note.id),
        }?;
        Some((note, r.content.clone()))
    }

    fn stub_text(&self, rec: &TurnRecord, idx: usize, content: &str, opts: &RenderOpts) -> String {
        let note = rec.calls.get(idx);
        let tool = note.map_or("tool", |n| n.name.as_str());
        let target = note
            .and_then(|n| n.target.as_deref())
            .map(|t| format!(" {}", short(t, 80)))
            .unwrap_or_default();
        let status = match note.and_then(|n| n.ok) {
            Some(true) => "ok",
            Some(false) => "FAILED",
            None => "done",
        };
        let hint = if opts.recall {
            format!("recall(turn={}, call={idx}) returns it in full", rec.turn)
        } else {
            "run the tool again if you need it".to_string()
        };
        format!(
            "[cleared from context: {tool}{target} -> {status}, {} chars. Began: \"{}\". {hint}.]",
            content.chars().count(),
            short(content, STUB_PREVIEW_CHARS)
        )
    }

    /// The provider messages for one turn at the requested fidelity.
    fn render_turn(&self, rec: &TurnRecord, stubbed: bool, opts: &RenderOpts) -> Vec<Value> {
        let mut model = rec.model_parts.clone();
        let mut response = rec.response_parts.clone();
        if stubbed {
            for p in &mut model {
                wire::prune_call_args(self.protocol, p, ARG_MAX_CHARS);
            }
        }
        let views = wire::results(self.protocol, &rec.response_parts);
        // after repair every response part is a recognised result, so the
        // views line up with the parts one to one
        for (i, v) in views.iter().enumerate() {
            let n = v.content.chars().count();
            let replacement = if stubbed && n > STUB_MIN_CHARS {
                Some(self.stub_text(rec, i, &v.content, opts))
            } else if n > opts.result_cap_chars {
                let hint = if opts.recall {
                    format!(
                        " recall(turn={}, call={i}) pages through the whole result",
                        rec.turn
                    )
                } else {
                    String::new()
                };
                Some(clip_chars(&v.content, opts.result_cap_chars, |omitted| {
                    format!(
                        "[... {omitted} characters left out to fit the context window.{hint} ...]"
                    )
                }))
            } else {
                None
            };
            if let (Some(text), Some(part)) = (replacement, response.get_mut(i)) {
                wire::set_result_content(self.protocol, part, &text);
            }
        }
        wire::messages(self.protocol, &model, &response)
    }

    /// Everything the request carries from history, oldest first.
    pub fn render(&self, opts: &RenderOpts) -> Vec<Value> {
        self.turns
            .iter()
            .filter(|r| r.turn > self.view.compacted_upto)
            .flat_map(|r| self.render_turn(r, r.turn <= self.view.stubbed_upto, opts))
            .collect()
    }

    /// Serialized size of one turn rendered at the given fidelity.
    pub fn turn_bytes(&self, rec: &TurnRecord, stubbed: bool, opts: &RenderOpts) -> usize {
        self.render_turn(rec, stubbed, opts)
            .iter()
            .map(|m| m.to_string().len())
            .sum()
    }

    /// A one-line description of every archived (not fully shown) turn, for
    /// the `recall` table of contents.
    pub fn outline(&self, max: usize) -> Vec<Value> {
        let archived: Vec<&TurnRecord> = self
            .turns
            .iter()
            .filter(|r| r.turn <= self.view.stubbed_upto)
            .collect();
        let skip = archived.len().saturating_sub(max);
        archived
            .into_iter()
            .skip(skip)
            .map(|r| {
                json!({
                    "turn": r.turn,
                    "folded_into_summary": r.turn <= self.view.compacted_upto,
                    "calls": r.calls.iter().enumerate().map(|(i, c)| json!({
                        "call": i,
                        "tool": c.name,
                        "target": c.target,
                        "ok": c.ok,
                        "chars": c.result_chars,
                    })).collect::<Vec<_>>(),
                })
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::wire::make_result;
    use ProviderProtocol::{Anthropic, Gemini, OpenAiCompatible};

    const OPTS: RenderOpts = RenderOpts {
        recall: true,
        result_cap_chars: 50_000,
    };

    fn read_turn(
        p: ProviderProtocol,
        n: usize,
        path: &str,
        body: &str,
    ) -> (Vec<Value>, Vec<Value>) {
        let id = format!("id{n}");
        match p {
            Gemini => (
                vec![json!({"functionCall":{"name":"read_file","args":{"path":path}}})],
                vec![make_result(p, "read_file", "", true, body)],
            ),
            Anthropic => (
                vec![json!({"type":"tool_use","id":id,"name":"read_file","input":{"path":path}})],
                vec![make_result(p, "read_file", &id, true, body)],
            ),
            OpenAiCompatible => (
                vec![
                    json!({"id":id,"type":"function","function":{"name":"read_file","arguments":json!({"path":path}).to_string()}}),
                ],
                vec![make_result(p, "read_file", &id, true, body)],
            ),
        }
    }

    fn filled(p: ProviderProtocol, turns: usize) -> Transcript {
        let mut t = Transcript::new(p);
        for n in 1..=turns {
            let (m, r) = read_turn(
                p,
                n,
                &format!("src/f{n}.rs"),
                &format!("fn f{n}() {{}}\n{}", "x".repeat(900)),
            );
            t.record(n, m, r);
        }
        t
    }

    #[test]
    fn notes_are_read_back_from_the_parts_for_every_protocol() {
        for p in [Gemini, Anthropic, OpenAiCompatible] {
            let t = filled(p, 2);
            let c = &t.turns()[1].calls[0];
            assert_eq!(c.name, "read_file", "{p:?}");
            assert_eq!(c.target.as_deref(), Some("src/f2.rs"));
            assert!(c.result_chars > 900);
        }
    }

    #[test]
    fn stubbing_replaces_bulky_results_and_keeps_the_pair_valid() {
        for p in [Gemini, Anthropic, OpenAiCompatible] {
            let mut t = filled(p, 4);
            let full = t.render(&OPTS);
            t.set_stub_watermark(2);
            let stubbed = t.render(&OPTS);
            let size = |v: &[Value]| v.iter().map(|m| m.to_string().len()).sum::<usize>();
            assert!(size(&stubbed) < size(&full), "{p:?} must shrink");
            let text = serde_json::to_string(&stubbed).unwrap();
            assert!(text.contains("cleared from context"), "{p:?}");
            assert!(text.contains("recall(turn=1, call=0)"), "{p:?}");
            // turn 3 and 4 are still whole
            assert!(text.contains("fn f3()") && text.contains("fn f4()"));
            // message count is unchanged: stubbing never drops a turn
            assert_eq!(stubbed.len(), full.len());
        }
    }

    #[test]
    fn small_results_are_never_stubbed() {
        let mut t = Transcript::new(Anthropic);
        let (m, r) = read_turn(Anthropic, 1, "a.rs", "tiny");
        t.record(1, m, r);
        t.set_stub_watermark(1);
        let text = serde_json::to_string(&t.render(&OPTS)).unwrap();
        assert!(text.contains("tiny") && !text.contains("cleared"));
    }

    #[test]
    fn stubs_only_mention_recall_when_the_tool_exists() {
        let mut t = filled(Anthropic, 2);
        t.set_stub_watermark(2);
        let off = RenderOpts {
            recall: false,
            result_cap_chars: 50_000,
        };
        let text = serde_json::to_string(&t.render(&off)).unwrap();
        assert!(!text.contains("recall("), "{text}");
        assert!(text.contains("run the tool again"));
    }

    #[test]
    fn long_arguments_are_cut_from_stubbed_turns_only() {
        let big = "z".repeat(3_000);
        let mut t = Transcript::new(Gemini);
        t.record(
            1,
            vec![json!({"functionCall":{"name":"create_file","args":{"path":"new.rs","content":big}}})],
            vec![make_result(Gemini, "create_file", "", true, "{\"ok\":true}")],
        );
        let whole = serde_json::to_string(&t.render(&OPTS)).unwrap();
        assert!(whole.contains(&big));
        t.set_stub_watermark(1);
        let cut = serde_json::to_string(&t.render(&OPTS)).unwrap();
        assert!(!cut.contains(&big) && cut.contains("new.rs") && cut.contains("left out"));
    }

    #[test]
    fn one_oversized_result_is_clipped_at_render_but_whole_in_the_log() {
        let mut t = Transcript::new(Anthropic);
        let body = format!("HEAD{}TAIL", "m".repeat(40_000));
        let (m, r) = read_turn(Anthropic, 1, "big.txt", &body);
        t.record(1, m, r);
        let small = RenderOpts {
            recall: true,
            result_cap_chars: 2_000,
        };
        let text = serde_json::to_string(&t.render(&small)).unwrap();
        assert!(text.len() < 4_000, "{}", text.len());
        assert!(text.contains("HEAD") && text.contains("TAIL"));
        assert!(text.contains("left out to fit the context window"));
        let (_, whole) = t.result_text(1, 0).unwrap();
        assert_eq!(whole, body, "the log keeps the whole result");
    }

    #[test]
    fn compaction_hides_turns_without_losing_them() {
        let mut t = filled(Anthropic, 5);
        t.apply_compaction(SummaryRecord {
            text: "did things".into(),
            covers_upto: 3,
            generation: 1,
            mode: SummaryMode::Mechanical,
            fingerprint: fingerprint("did things"),
        });
        let msgs = t.render(&OPTS);
        assert_eq!(msgs.len(), 4, "turns 4 and 5, two messages each");
        assert_eq!(t.visible(), vec![(4, false), (5, false)]);
        assert_eq!(t.turns().len(), 5, "nothing is deleted");
        assert!(t.result_text(1, 0).is_some());
        assert_eq!(t.view().compacted_upto, 3);
        assert!(
            t.view().stubbed_upto >= 3,
            "the stub watermark never trails the fold"
        );
    }

    #[test]
    fn the_watermark_never_moves_back_or_past_the_last_turn() {
        let mut t = filled(Gemini, 3);
        t.set_stub_watermark(2);
        t.set_stub_watermark(1);
        assert_eq!(t.view().stubbed_upto, 2);
        t.set_stub_watermark(99);
        assert_eq!(t.view().stubbed_upto, 3);
    }

    #[test]
    fn replay_rebuilds_the_exact_view_and_drops_turns_past_the_checkpoint() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("context");
        {
            let mut t = Transcript::open(Anthropic, dir.clone(), 99);
            for n in 1..=6 {
                let (m, r) = read_turn(Anthropic, n, &format!("src/f{n}.rs"), &"y".repeat(900));
                t.record(n, m, r);
            }
            t.set_stub_watermark(4);
            t.apply_compaction(SummaryRecord {
                text: "summary one".into(),
                covers_upto: 2,
                generation: 1,
                mode: SummaryMode::Model,
                fingerprint: fingerprint("summary one"),
            });
            t.save_ratio(3.1);
        }
        let again = Transcript::open(Anthropic, dir.clone(), 99);
        assert_eq!(again.turns().len(), 6);
        assert_eq!(again.view().compacted_upto, 2);
        assert_eq!(again.view().stubbed_upto, 4);
        assert_eq!(again.view().summary.as_ref().unwrap().text, "summary one");
        assert_eq!(again.saved_ratio(), Some(3.1));
        assert!(!again.index().is_empty(), "the retrieval index is rebuilt");

        // the checkpoint only got to turn 3: later turns are ignored, and
        // the view is clamped to what exists
        let early = Transcript::open(Anthropic, dir, 3);
        assert_eq!(early.turns().len(), 3);
        assert_eq!(early.view().stubbed_upto, 3);
        assert_eq!(early.view().compacted_upto, 2);
    }

    #[test]
    fn a_torn_final_line_is_skipped_not_fatal() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("context");
        {
            let mut t = Transcript::open(Gemini, dir.clone(), 99);
            let (m, r) = read_turn(Gemini, 1, "a.rs", "ok");
            t.record(1, m, r);
        }
        let mut f = OpenOptions::new()
            .append(true)
            .open(dir.join("log.jsonl"))
            .unwrap();
        f.write_all(b"{\"turn\":2,\"model_parts\":[{\"functionC")
            .unwrap();
        drop(f);
        let again = Transcript::open(Gemini, dir, 99);
        assert_eq!(again.turns().len(), 1);
    }

    #[test]
    fn rerecording_a_turn_replaces_it_instead_of_duplicating() {
        let mut t = Transcript::new(Anthropic);
        let (m, r) = read_turn(Anthropic, 1, "a.rs", "first");
        t.record(1, m, r);
        let (m, r) = read_turn(Anthropic, 1, "a.rs", "second");
        t.record(1, m, r);
        assert_eq!(t.turns().len(), 1);
        assert_eq!(t.result_text(1, 0).unwrap().1, "second");
    }

    #[test]
    fn a_malformed_pair_is_repaired_when_recorded() {
        let mut t = Transcript::new(Anthropic);
        t.record(
            1,
            vec![json!({"type":"tool_use","id":"a","name":"read_file","input":{"path":"x"}})],
            vec![],
        );
        assert_eq!(t.repairs(), 1);
        let msgs = t.render(&OPTS);
        assert_eq!(msgs.len(), 2, "the call now has a result to pair with");
    }

    #[test]
    fn writes_make_earlier_reads_of_the_same_file_stale() {
        let mut t = Transcript::new(Gemini);
        let (m, r) = read_turn(Gemini, 1, "src/a.rs", "old");
        t.record(1, m, r);
        t.record(
            2,
            vec![json!({"functionCall":{"name":"edit_file","args":{"path":"./src/a.rs","expected":"old","replacement":"new","replace_all":false}}})],
            vec![make_result(Gemini, "edit_file", "", true, "{\"ok\":true}")],
        );
        assert_eq!(t.changed_after(1, "src/a.rs"), Some(2));
        assert_eq!(
            t.changed_after(2, "src/a.rs"),
            None,
            "not changed after the edit itself"
        );
        assert_eq!(t.changed_after(1, "src/b.rs"), None);
    }

    #[test]
    fn a_failed_edit_does_not_stale_anything() {
        let mut t = Transcript::new(Anthropic);
        t.record(
            1,
            vec![json!({"type":"tool_use","id":"e","name":"edit_file","input":{"path":"a.rs","expected":"x","replacement":"y","replace_all":false}})],
            vec![make_result(Anthropic, "edit_file", "e", false, "no match")],
        );
        assert_eq!(t.changed_after(0, "a.rs"), None);
    }

    #[test]
    fn apply_patch_targets_every_file_in_the_patch() {
        let patch = "*** Begin Patch\n*** Update File: src/a.rs\n@@\n-x\n+y\n*** Add File: src/b.rs\n+z\n*** End Patch";
        let mut t = Transcript::new(Gemini);
        t.record(
            1,
            vec![json!({"functionCall":{"name":"apply_patch","args":{"patch":patch}}})],
            vec![make_result(
                Gemini,
                "apply_patch",
                "",
                true,
                "{\"ok\":true}",
            )],
        );
        assert_eq!(
            t.turns()[0].calls[0].target.as_deref(),
            Some("src/a.rs, src/b.rs")
        );
        assert!(t.changed_after(0, "src/b.rs").is_some());
    }

    #[test]
    fn the_outline_lists_archived_turns_newest_last_and_bounded() {
        let mut t = filled(OpenAiCompatible, 8);
        t.set_stub_watermark(6);
        t.apply_compaction(SummaryRecord {
            text: "s".into(),
            covers_upto: 2,
            generation: 1,
            mode: SummaryMode::Mechanical,
            fingerprint: fingerprint("s"),
        });
        let all = t.outline(100);
        assert_eq!(all.len(), 6);
        assert_eq!(all[0]["turn"], 1);
        assert_eq!(all[0]["folded_into_summary"], true);
        assert_eq!(all[5]["folded_into_summary"], false);
        let few = t.outline(2);
        assert_eq!(few.len(), 2);
        assert_eq!(few[1]["turn"], 6);
    }

    #[test]
    fn the_fingerprint_is_stable_and_sensitive() {
        assert_eq!(fingerprint("a"), fingerprint("a"));
        assert_ne!(fingerprint("a"), fingerprint("b"));
        assert_eq!(fingerprint("").len(), 16);
    }
}
