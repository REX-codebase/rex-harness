//! The three provider wire shapes, seen from the transcript.
//!
//! A recorded turn is the provider-native pair the loop already builds:
//! `model_parts` (the assistant's tool calls) and `response_parts` (the
//! results REX returned). Everything the context engine needs to know about
//! a turn (which tool, which file, did it work, what did it write) is read
//! back out of those parts here, so the engine needs no hook inside the
//! loop's tool dispatch and stays correct for every protocol.
//!
//! | protocol | call part | result part |
//! |---|---|---|
//! | Gemini | `{functionCall:{name,args}}` | `{functionResponse:{name,response:{ok,content}}}` |
//! | Anthropic | `{type:tool_use,id,name,input}` | `{type:tool_result,tool_use_id,is_error,content}` |
//! | OpenAI-compatible | `{id,type:function,function:{name,arguments}}` | `{role:tool,tool_call_id,content}` |
//! | OpenAI Responses | `{type:function_call,call_id,name,arguments}` | `{type:function_call_output,call_id,output}` |
//!
//! Gemini results carry no id, so they pair with calls by position.
//!
//! The Responses shape differs in one way that matters here: a model turn is
//! the list of *output items* (`reasoning`, `message`, `function_call`),
//! replayed whole because the route runs with `store: false`. Reasoning items
//! carry encrypted content and must stay beside the call they preceded, so
//! they are kept as they came and only the calls are looked into.

use crate::providers::ProviderProtocol;
use serde_json::{json, Value};

/// Text that stands in for a result the loop never recorded.
pub const MISSING_RESULT: &str = "(no result was recorded for this call)";

#[derive(Debug, Clone, PartialEq)]
pub struct CallView {
    pub id: String,
    pub name: String,
    pub args: Value,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ResultView {
    pub id: String,
    pub name: String,
    /// `None` when the wire shape carries no success flag (OpenAI).
    pub ok: Option<bool>,
    pub content: String,
}

fn text_of(content: &Value) -> String {
    match content {
        Value::String(s) => s.clone(),
        Value::Array(blocks) => blocks
            .iter()
            .filter_map(|b| b.get("text").and_then(Value::as_str))
            .collect::<Vec<_>>()
            .join("\n"),
        Value::Null => String::new(),
        other => other.to_string(),
    }
}

/// The calls in `parts`, in order. Parts that are not a well-formed call
/// for `protocol` are skipped.
pub fn calls(protocol: ProviderProtocol, parts: &[Value]) -> Vec<CallView> {
    parts
        .iter()
        .filter_map(|p| match protocol {
            ProviderProtocol::Gemini => {
                let f = p.get("functionCall")?;
                Some(CallView {
                    id: String::new(),
                    name: f.get("name")?.as_str()?.to_string(),
                    args: f.get("args").cloned().unwrap_or_else(|| json!({})),
                })
            }
            ProviderProtocol::Anthropic => {
                if p.get("type").and_then(Value::as_str) != Some("tool_use") {
                    return None;
                }
                Some(CallView {
                    id: p.get("id")?.as_str()?.to_string(),
                    name: p.get("name")?.as_str()?.to_string(),
                    args: p.get("input").cloned().unwrap_or_else(|| json!({})),
                })
            }
            ProviderProtocol::OpenAiCompatible => {
                let f = p.get("function")?;
                let raw = f.get("arguments").and_then(Value::as_str).unwrap_or("{}");
                Some(CallView {
                    id: p.get("id")?.as_str()?.to_string(),
                    name: f.get("name")?.as_str()?.to_string(),
                    args: serde_json::from_str(raw).unwrap_or_else(|_| json!({})),
                })
            }
            ProviderProtocol::OpenAiResponses => {
                if p.get("type").and_then(Value::as_str) != Some("function_call") {
                    return None;
                }
                let raw = p.get("arguments").and_then(Value::as_str).unwrap_or("{}");
                Some(CallView {
                    id: p.get("call_id")?.as_str()?.to_string(),
                    name: p.get("name")?.as_str()?.to_string(),
                    args: serde_json::from_str(raw).unwrap_or_else(|_| json!({})),
                })
            }
        })
        .collect()
}

/// The results in `parts`, in order.
pub fn results(protocol: ProviderProtocol, parts: &[Value]) -> Vec<ResultView> {
    parts
        .iter()
        .filter_map(|p| match protocol {
            ProviderProtocol::Gemini => {
                let f = p.get("functionResponse")?;
                let r = f.get("response")?;
                Some(ResultView {
                    id: String::new(),
                    name: f.get("name")?.as_str()?.to_string(),
                    ok: r.get("ok").and_then(Value::as_bool),
                    content: text_of(r.get("content").unwrap_or(&Value::Null)),
                })
            }
            ProviderProtocol::Anthropic => {
                if p.get("type").and_then(Value::as_str) != Some("tool_result") {
                    return None;
                }
                Some(ResultView {
                    id: p.get("tool_use_id")?.as_str()?.to_string(),
                    name: String::new(),
                    ok: p.get("is_error").and_then(Value::as_bool).map(|e| !e),
                    content: text_of(p.get("content").unwrap_or(&Value::Null)),
                })
            }
            ProviderProtocol::OpenAiCompatible => {
                if p.get("role").and_then(Value::as_str) != Some("tool") {
                    return None;
                }
                Some(ResultView {
                    id: p.get("tool_call_id")?.as_str()?.to_string(),
                    name: String::new(),
                    ok: None,
                    content: text_of(p.get("content").unwrap_or(&Value::Null)),
                })
            }
            ProviderProtocol::OpenAiResponses => {
                if p.get("type").and_then(Value::as_str) != Some("function_call_output") {
                    return None;
                }
                Some(ResultView {
                    id: p.get("call_id")?.as_str()?.to_string(),
                    name: String::new(),
                    ok: None,
                    content: text_of(p.get("output").unwrap_or(&Value::Null)),
                })
            }
        })
        .collect()
}

/// A result part in the provider's own shape.
pub fn make_result(
    protocol: ProviderProtocol,
    name: &str,
    id: &str,
    ok: bool,
    content: &str,
) -> Value {
    match protocol {
        ProviderProtocol::Gemini => {
            json!({"functionResponse":{"name":name,"response":{"ok":ok,"content":content}}})
        }
        ProviderProtocol::Anthropic => {
            json!({"type":"tool_result","tool_use_id":id,"is_error":!ok,"content":content})
        }
        ProviderProtocol::OpenAiCompatible => {
            json!({"role":"tool","tool_call_id":id,"content":content})
        }
        // The Responses item has no success flag; the text carries the outcome.
        ProviderProtocol::OpenAiResponses => {
            json!({"type":"function_call_output","call_id":id,"output":content})
        }
    }
}

/// Replace a result part's text, leaving its id, name and flags alone.
pub fn set_result_content(protocol: ProviderProtocol, part: &mut Value, content: &str) {
    match protocol {
        ProviderProtocol::Gemini => {
            if let Some(r) = part.pointer_mut("/functionResponse/response") {
                r["content"] = json!(content);
            }
        }
        ProviderProtocol::Anthropic | ProviderProtocol::OpenAiCompatible => {
            part["content"] = json!(content);
        }
        ProviderProtocol::OpenAiResponses => {
            part["output"] = json!(content);
        }
    }
}

/// Shorten every string inside `value` that is longer than `max` chars,
/// keeping a head and saying how much was left out. Returns whether
/// anything changed.
fn prune_strings(value: &mut Value, max: usize) -> bool {
    match value {
        Value::String(s) => {
            let n = s.chars().count();
            if n <= max {
                return false;
            }
            let head: String = s.chars().take(max.min(120)).collect();
            *s = format!(
                "{head}… [{} chars left out of older history]",
                n - head.chars().count()
            );
            true
        }
        Value::Array(items) => items
            .iter_mut()
            .fold(false, |c, v| prune_strings(v, max) | c),
        Value::Object(map) => map
            .values_mut()
            .fold(false, |c, v| prune_strings(v, max) | c),
        _ => false,
    }
}

/// Cut long string arguments (file bodies in `create_file`, patch text,
/// `edit_file` before/after) out of an old tool call. The file is on disk,
/// so the argument is the least valuable thing to keep. The call keeps its
/// id, name and shape, so the pair stays valid.
pub fn prune_call_args(protocol: ProviderProtocol, part: &mut Value, max: usize) -> bool {
    match protocol {
        ProviderProtocol::Gemini => part
            .pointer_mut("/functionCall/args")
            .is_some_and(|a| prune_strings(a, max)),
        ProviderProtocol::Anthropic => part.get_mut("input").is_some_and(|a| prune_strings(a, max)),
        ProviderProtocol::OpenAiCompatible | ProviderProtocol::OpenAiResponses => {
            // arguments are a JSON string, under `function` for chat
            // completions and at the top of a Responses `function_call`
            let responses = protocol == ProviderProtocol::OpenAiResponses;
            let Some(raw) = part
                .pointer(if responses {
                    "/arguments"
                } else {
                    "/function/arguments"
                })
                .and_then(Value::as_str)
                .map(str::to_string)
            else {
                return false;
            };
            let Ok(mut args) = serde_json::from_str::<Value>(&raw) else {
                return false;
            };
            if !prune_strings(&mut args, max) {
                return false;
            }
            if responses {
                part["arguments"] = json!(args.to_string());
            } else {
                part["function"]["arguments"] = json!(args.to_string());
            }
            true
        }
    }
}

/// Make a recorded pair acceptable to the provider forever.
///
/// The loop sends one pair once, so a malformed pair costs one request. A
/// transcript replays every pair on every request, so one malformed pair
/// would make the whole run fail permanently. This fixes the cases that
/// providers reject:
///
/// * a call with no result gets a placeholder result,
/// * a result with no call is dropped,
/// * (Anthropic, OpenAI chat) a model part that is not a call for this
///   protocol is dropped. Responses turns keep every output item: reasoning
///   and message items belong beside the calls they preceded.
///
/// Returns the number of repairs made.
pub fn repair_pairing(
    protocol: ProviderProtocol,
    model_parts: &mut Vec<Value>,
    response_parts: &mut Vec<Value>,
) -> usize {
    let mut repairs = 0;
    if !matches!(
        protocol,
        ProviderProtocol::Gemini | ProviderProtocol::OpenAiResponses
    ) {
        let before = model_parts.len();
        let keep: Vec<bool> = model_parts
            .iter()
            .map(|p| !calls(protocol, std::slice::from_ref(p)).is_empty())
            .collect();
        let mut it = keep.iter();
        model_parts.retain(|_| *it.next().unwrap_or(&true));
        repairs += before - model_parts.len();
    }
    let call_list = calls(protocol, model_parts);
    match protocol {
        ProviderProtocol::Gemini => {
            let mut have = results(protocol, response_parts).len();
            // too many results: drop the extras from the end
            while have > call_list.len() {
                if let Some(pos) = response_parts
                    .iter()
                    .rposition(|p| p.get("functionResponse").is_some())
                {
                    response_parts.remove(pos);
                    have -= 1;
                    repairs += 1;
                } else {
                    break;
                }
            }
            // too few: placeholder for each call without one
            for c in call_list.iter().skip(have) {
                response_parts.push(make_result(protocol, &c.name, "", false, MISSING_RESULT));
                repairs += 1;
            }
        }
        ProviderProtocol::Anthropic
        | ProviderProtocol::OpenAiCompatible
        | ProviderProtocol::OpenAiResponses => {
            let ids: Vec<&str> = call_list.iter().map(|c| c.id.as_str()).collect();
            let before = response_parts.len();
            response_parts.retain(|p| {
                let id = match protocol {
                    ProviderProtocol::Anthropic => p.get("tool_use_id"),
                    ProviderProtocol::OpenAiResponses => p.get("call_id"),
                    _ => p.get("tool_call_id"),
                }
                .and_then(Value::as_str);
                id.is_some_and(|i| ids.contains(&i))
            });
            repairs += before - response_parts.len();
            let answered: Vec<String> = results(protocol, response_parts)
                .into_iter()
                .map(|r| r.id)
                .collect();
            for c in &call_list {
                if !answered.contains(&c.id) {
                    response_parts.push(make_result(
                        protocol,
                        &c.name,
                        &c.id,
                        false,
                        MISSING_RESULT,
                    ));
                    repairs += 1;
                }
            }
        }
    }
    repairs
}

/// The provider messages for one recorded turn, in the same shape the
/// loop's own request builder uses for the previous turn.
pub fn messages(
    protocol: ProviderProtocol,
    model_parts: &[Value],
    response_parts: &[Value],
) -> Vec<Value> {
    let mut out = Vec::new();
    match protocol {
        ProviderProtocol::Gemini => {
            if !model_parts.is_empty() {
                out.push(json!({"role":"model","parts": model_parts}));
            }
            if !response_parts.is_empty() {
                out.push(json!({"role":"user","parts": response_parts}));
            }
        }
        ProviderProtocol::Anthropic => {
            if !model_parts.is_empty() {
                out.push(json!({"role":"assistant","content": model_parts}));
            }
            if !response_parts.is_empty() {
                out.push(json!({"role":"user","content": response_parts}));
            }
        }
        ProviderProtocol::OpenAiCompatible => {
            if !model_parts.is_empty() {
                out.push(
                    json!({"role":"assistant","content":Value::Null,"tool_calls": model_parts}),
                );
            }
            out.extend(response_parts.iter().cloned());
        }
        // Responses input is a flat list of items: the turn's output items,
        // then the outputs of its calls.
        ProviderProtocol::OpenAiResponses => {
            out.extend(model_parts.iter().cloned());
            out.extend(response_parts.iter().cloned());
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use ProviderProtocol::{Anthropic, Gemini, OpenAiCompatible, OpenAiResponses};

    fn gemini_pair() -> (Vec<Value>, Vec<Value>) {
        (
            vec![
                json!({"functionCall":{"name":"read_file","args":{"path":"a.rs"}},"thoughtSignature":"sig"}),
            ],
            vec![make_result(Gemini, "read_file", "", true, "fn a() {}")],
        )
    }
    fn anthropic_pair() -> (Vec<Value>, Vec<Value>) {
        (
            vec![json!({"type":"tool_use","id":"t1","name":"read_file","input":{"path":"a.rs"}})],
            vec![make_result(Anthropic, "read_file", "t1", true, "fn a() {}")],
        )
    }
    fn responses_pair() -> (Vec<Value>, Vec<Value>) {
        (
            vec![
                json!({"type":"reasoning","id":"rs_1","summary":[],"encrypted_content":"ENC"}),
                json!({"type":"function_call","id":"fc_1","call_id":"c1","name":"read_file","namespace":"rex","arguments":"{\"path\":\"a.rs\"}"}),
            ],
            vec![make_result(
                OpenAiResponses,
                "read_file",
                "c1",
                true,
                "fn a() {}",
            )],
        )
    }
    fn openai_pair() -> (Vec<Value>, Vec<Value>) {
        (
            vec![
                json!({"id":"c1","type":"function","function":{"name":"read_file","arguments":"{\"path\":\"a.rs\"}"}}),
            ],
            vec![make_result(
                OpenAiCompatible,
                "read_file",
                "c1",
                true,
                "fn a() {}",
            )],
        )
    }

    #[test]
    fn calls_and_results_read_back_out_of_every_shape() {
        for (p, (m, r)) in [
            (Gemini, gemini_pair()),
            (Anthropic, anthropic_pair()),
            (OpenAiCompatible, openai_pair()),
            (OpenAiResponses, responses_pair()),
        ] {
            let c = calls(p, &m);
            assert_eq!(c.len(), 1, "{p:?}");
            assert_eq!(c[0].name, "read_file");
            assert_eq!(c[0].args["path"], "a.rs");
            let v = results(p, &r);
            assert_eq!(v.len(), 1);
            assert_eq!(v[0].content, "fn a() {}");
            // chat-completion tool messages and Responses outputs carry no
            // success flag; the text carries the outcome
            assert_eq!(
                v[0].ok,
                if matches!(p, OpenAiCompatible | OpenAiResponses) {
                    None
                } else {
                    Some(true)
                }
            );
        }
    }

    #[test]
    fn a_result_flag_survives_for_failures() {
        let r = vec![make_result(Anthropic, "run_command", "t9", false, "boom")];
        assert_eq!(results(Anthropic, &r)[0].ok, Some(false));
        let g = vec![make_result(Gemini, "run_command", "", false, "boom")];
        assert_eq!(results(Gemini, &g)[0].ok, Some(false));
    }

    #[test]
    fn set_result_content_keeps_ids_and_flags() {
        for (p, (_, mut r)) in [
            (Gemini, gemini_pair()),
            (Anthropic, anthropic_pair()),
            (OpenAiCompatible, openai_pair()),
            (OpenAiResponses, responses_pair()),
        ] {
            let before = results(p, &r)[0].clone();
            set_result_content(p, &mut r[0], "[cleared]");
            let after = results(p, &r)[0].clone();
            assert_eq!(after.content, "[cleared]");
            assert_eq!(after.id, before.id);
            assert_eq!(after.ok, before.ok);
        }
    }

    #[test]
    fn pruning_cuts_long_arguments_but_keeps_the_call_valid() {
        let big = "x".repeat(5_000);
        let mut g = json!({"functionCall":{"name":"create_file","args":{"path":"a","content":big}},"thoughtSignature":"s"});
        assert!(prune_call_args(Gemini, &mut g, 400));
        assert!(g["functionCall"]["args"]["content"].as_str().unwrap().len() < 300);
        assert_eq!(g["functionCall"]["args"]["path"], "a");
        assert_eq!(g["thoughtSignature"], "s", "the signature must survive");

        let mut a = json!({"type":"tool_use","id":"t1","name":"apply_patch","input":{"patch":big}});
        assert!(prune_call_args(Anthropic, &mut a, 400));
        assert_eq!(a["id"], "t1");

        let args = json!({"path":"a","content":big}).to_string();
        let mut o =
            json!({"id":"c1","type":"function","function":{"name":"create_file","arguments":args}});
        assert!(prune_call_args(OpenAiCompatible, &mut o, 400));
        let parsed: Value =
            serde_json::from_str(o["function"]["arguments"].as_str().unwrap()).unwrap();
        assert_eq!(
            parsed["path"], "a",
            "OpenAI arguments must remain valid JSON"
        );
        assert!(parsed["content"].as_str().unwrap().contains("left out"));

        let mut short = json!({"functionCall":{"name":"read_file","args":{"path":"a"}}});
        assert!(
            !prune_call_args(Gemini, &mut short, 400),
            "short args are untouched"
        );
    }

    #[test]
    fn pruning_is_safe_on_multibyte_text() {
        let big = "é".repeat(3_000);
        let mut g = json!({"functionCall":{"name":"create_file","args":{"content":big}}});
        assert!(prune_call_args(Gemini, &mut g, 400));
        assert!(g["functionCall"]["args"]["content"]
            .as_str()
            .unwrap()
            .starts_with('é'));
    }

    #[test]
    fn repair_adds_placeholders_for_calls_without_results() {
        let (m, _) = anthropic_pair();
        let (mut m, mut r) = (m, Vec::new());
        assert_eq!(repair_pairing(Anthropic, &mut m, &mut r), 1);
        let v = results(Anthropic, &r);
        assert_eq!(v[0].id, "t1");
        assert_eq!(v[0].content, MISSING_RESULT);
        assert_eq!(v[0].ok, Some(false));

        let (m, _) = openai_pair();
        let (mut m, mut r) = (m, Vec::new());
        assert_eq!(repair_pairing(OpenAiCompatible, &mut m, &mut r), 1);
        assert_eq!(results(OpenAiCompatible, &r)[0].id, "c1");

        let (m, _) = gemini_pair();
        let (mut m, mut r) = (m, Vec::new());
        assert_eq!(repair_pairing(Gemini, &mut m, &mut r), 1);
        assert_eq!(results(Gemini, &r)[0].name, "read_file");
    }

    #[test]
    fn repair_drops_orphan_results_and_foreign_model_parts() {
        let (mut m, mut r) = anthropic_pair();
        r.push(make_result(Anthropic, "x", "ghost", true, "orphan"));
        assert_eq!(repair_pairing(Anthropic, &mut m, &mut r), 1);
        assert_eq!(r.len(), 1);

        // a Gemini-shaped fallback part inside an Anthropic turn is invalid
        let mut m = vec![json!({"functionCall":{"name":"read_file","args":{}}})];
        let mut r = vec![make_result(Anthropic, "read_file", "t1", true, "x")];
        let n = repair_pairing(Anthropic, &mut m, &mut r);
        assert!(m.is_empty() && r.is_empty(), "{n} repairs");

        let (m, mut r) = gemini_pair();
        let mut m = m;
        r.push(make_result(Gemini, "read_file", "", true, "extra"));
        assert_eq!(repair_pairing(Gemini, &mut m, &mut r), 1);
        assert_eq!(results(Gemini, &r).len(), 1);
    }

    #[test]
    fn a_well_formed_pair_needs_no_repair() {
        for (p, (mut m, mut r)) in [
            (Gemini, gemini_pair()),
            (Anthropic, anthropic_pair()),
            (OpenAiCompatible, openai_pair()),
            (OpenAiResponses, responses_pair()),
        ] {
            assert_eq!(repair_pairing(p, &mut m, &mut r), 0, "{p:?}");
        }
    }

    #[test]
    fn messages_match_the_loops_own_shape() {
        let (m, r) = anthropic_pair();
        let msgs = messages(Anthropic, &m, &r);
        assert_eq!(msgs[0]["role"], "assistant");
        assert_eq!(msgs[1]["role"], "user");
        let (m, r) = gemini_pair();
        let msgs = messages(Gemini, &m, &r);
        assert_eq!(msgs[0]["role"], "model");
        assert_eq!(msgs[1]["role"], "user");
        let (m, r) = openai_pair();
        let msgs = messages(OpenAiCompatible, &m, &r);
        assert_eq!(msgs[0]["role"], "assistant");
        assert!(msgs[0]["content"].is_null());
        assert_eq!(msgs[1]["role"], "tool");
        assert!(messages(Gemini, &[], &[]).is_empty());
    }
    #[test]
    fn a_responses_turn_keeps_its_reasoning_items_through_repair() {
        // dropping a reasoning item while keeping the call it preceded is
        // refused when history is replayed statelessly
        let (mut m, mut r) = responses_pair();
        assert_eq!(repair_pairing(OpenAiResponses, &mut m, &mut r), 0);
        assert_eq!(m[0]["type"], "reasoning");
        assert_eq!(m[0]["encrypted_content"], "ENC");
        // a call with no output gets a placeholder; the reasoning stays
        let (mut m, mut r) = responses_pair();
        r.clear();
        assert_eq!(repair_pairing(OpenAiResponses, &mut m, &mut r), 1);
        assert_eq!(m.len(), 2);
        assert_eq!(r[0]["type"], "function_call_output");
        assert_eq!(r[0]["call_id"], "c1");
        assert_eq!(r[0]["output"], MISSING_RESULT);
        // an output with no call is dropped
        let (mut m, mut r) = responses_pair();
        r.push(make_result(OpenAiResponses, "x", "ghost", true, "orphan"));
        assert_eq!(repair_pairing(OpenAiResponses, &mut m, &mut r), 1);
        assert_eq!(r.len(), 1);
    }

    #[test]
    fn responses_history_is_a_flat_item_list() {
        let (m, r) = responses_pair();
        let items = messages(OpenAiResponses, &m, &r);
        assert_eq!(items.len(), 3);
        assert_eq!(items[0]["type"], "reasoning");
        assert_eq!(items[1]["type"], "function_call");
        assert_eq!(items[2]["type"], "function_call_output");
        // no role wrappers: every item is already an input item
        assert!(items.iter().all(|i| i.get("role").is_none()));
    }

    #[test]
    fn responses_arguments_are_pruned_at_the_top_of_the_call_item() {
        let big = "q".repeat(4_000);
        let mut call = json!({"type":"function_call","id":"fc_1","call_id":"c1","name":"create_file",
            "namespace":"rex","arguments": json!({"path":"a","content":big}).to_string()});
        assert!(prune_call_args(OpenAiResponses, &mut call, 400));
        let args: Value = serde_json::from_str(call["arguments"].as_str().unwrap()).unwrap();
        assert_eq!(args["path"], "a", "the arguments must remain valid JSON");
        assert!(args["content"].as_str().unwrap().contains("left out"));
        assert_eq!(call["call_id"], "c1");
        assert_eq!(call["namespace"], "rex");
    }

    #[test]
    fn a_stub_replaces_a_responses_output_in_place() {
        let (_, mut r) = responses_pair();
        set_result_content(OpenAiResponses, &mut r[0], "[cleared]");
        assert_eq!(r[0]["output"], "[cleared]");
        assert_eq!(r[0]["call_id"], "c1");
    }
}
