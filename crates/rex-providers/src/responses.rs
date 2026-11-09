//! OpenAI's Responses API, as a ChatGPT-plan OAuth token may use it.
//!
//! The contract is OpenAI's own ("Sign in with ChatGPT", ChatGPT plan usage,
//! *Preview limitations* and *Models and inference*, read 2026-11-09):
//!
//! * `POST {base}/responses` with `Authorization: Bearer <access token>`;
//! * `store: false` and `stream: true` on every request, history replayed in
//!   `input` (no `previous_response_id`);
//! * the system prompt goes in `instructions`; a `system`-role input item is
//!   rejected;
//! * function tools are grouped in a namespace;
//! * these fields are not accepted and are never sent: `background`,
//!   `conversation`, `max_output_tokens`, `max_tool_calls`, `metadata`,
//!   `moderation`, `multi_agent`, `prompt`, `prompt_cache_retention`,
//!   `safety_identifier`, `temperature`, `top_logprobs`, `top_p`,
//!   `truncation`, `user`;
//! * inference succeeded only when `response.completed` arrived.
//!
//! The transport hands back the whole event stream as text, so the reply is
//! read from the terminal event: `response.completed` carries the complete
//! response (its `output` items and `usage`). [`normalize`] turns the stream
//! into that one response object, which the rest of the loop reads like any
//! other provider's non-streaming reply.

use serde_json::{json, Value};

/// The single namespace all REX tools are grouped under.
pub const NAMESPACE: &str = "rex";

/// Fields the ChatGPT-plan route rejects. [`request`] never emits them; a
/// test pins that.
#[cfg(test)]
pub const UNSUPPORTED_FIELDS: &[&str] = &[
    "background",
    "conversation",
    "max_output_tokens",
    "max_tool_calls",
    "metadata",
    "moderation",
    "multi_agent",
    "prompt",
    "prompt_cache_retention",
    "safety_identifier",
    "temperature",
    "top_logprobs",
    "top_p",
    "truncation",
    "user",
    "previous_response_id",
];

/// Where users review and limit what an app may spend of their plan.
pub const USAGE_SETTINGS_URL: &str = "https://chatgpt.com/settings/usage";

/// A request body this route accepts. `tools` is the already-namespaced
/// tool list, or `None` for a tool-free call (a summary, a judge).
pub fn request(model: &str, instructions: &str, input: &[Value], tools: Option<Value>) -> String {
    let mut body = json!({
        "model": model,
        "instructions": instructions,
        "input": input,
        "store": false,
        "stream": true,
    });
    if let Some(tools) = tools {
        body["tools"] = tools;
    }
    body.to_string()
}

/// A user message as an input item.
pub fn user_item(text: &str) -> Value {
    json!({"role": "user", "content": text})
}

/// Group function declarations (`{name, description, parameters}`, with
/// lower-case JSON-schema types) into the one namespace the route accepts.
pub fn namespace_tools(functions: Vec<Value>) -> Value {
    json!([{
        "type": "namespace",
        "name": NAMESPACE,
        "description": "REX workspace tools: read, search, edit, run commands, plan and finish.",
        "tools": functions
            .into_iter()
            .map(|f| json!({
                "type": "function",
                "name": f["name"],
                "description": f["description"],
                "parameters": f["parameters"],
            }))
            .collect::<Vec<_>>(),
    }])
}

/// Why a stream did not end in a completed response.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StreamFailure {
    /// `response.failed` or an `error` event, with the code when one came.
    Failed { code: String },
    /// `response.incomplete`, with the reason when one came.
    Incomplete { reason: String },
    /// The stream ended with no terminal event (a dropped connection).
    Interrupted,
}

impl StreamFailure {
    /// One line for the run's error. Only identifier-shaped codes are kept:
    /// provider text can echo prompt fragments and never reaches durable
    /// state. A documented code gets its documented recovery.
    pub fn detail(&self) -> String {
        match self {
            StreamFailure::Failed { code } => match guidance(code) {
                Some(g) => format!("ChatGPT plan: {g} [{code}]"),
                None => format!("the Responses stream failed [{code}]"),
            },
            StreamFailure::Incomplete { reason } => {
                format!("the Responses stream ended incomplete [{reason}]")
            }
            StreamFailure::Interrupted => {
                "the Responses stream ended without response.completed (connection interrupted)"
                    .to_string()
            }
        }
    }
}

/// The recovery OpenAI documents for each ChatGPT-plan error code.
pub fn guidance(code: &str) -> Option<&'static str> {
    Some(match code {
        "subscription_sharing_usage_limit_exceeded" => {
            "usage limit reached. Review the limit at https://chatgpt.com/settings/usage; it may be this app's own limit"
        }
        "subscription_sharing_usage_unavailable"
        | "subscription_sharing_user_unavailable" => {
            "usage could not be checked right now; try again shortly"
        }
        "subscription_sharing_user_not_eligible" => {
            "this account, workspace or policy cannot use ChatGPT plan usage here"
        }
        "subscription_sharing_unsupported_capability" => {
            "the request used something the ChatGPT-plan route does not support"
        }
        "subscription_sharing_route_not_supported" => {
            "this endpoint is not permitted with a ChatGPT-plan token"
        }
        "subscription_sharing_invalid_user" => {
            "the account could not be validated; sign in with ChatGPT again"
        }
        "chatpass_v2_scope_not_authorized" | "chatpass_v2_invalid_authorization_context" => {
            "the token's permissions do not cover this operation"
        }
        _ => return None,
    })
}

/// Codes after which repeating the same request, or looping through sign-in,
/// cannot help.
pub fn is_terminal_code(code: &str) -> bool {
    matches!(
        code,
        "subscription_sharing_usage_limit_exceeded"
            | "subscription_sharing_user_not_eligible"
            | "subscription_sharing_unsupported_capability"
            | "subscription_sharing_route_not_supported"
            | "subscription_sharing_invalid_user"
            | "chatpass_v2_scope_not_authorized"
            | "chatpass_v2_invalid_authorization_context"
    )
}

fn is_code_shaped(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 80
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.'))
}

/// The error code in a failure object, if it is identifier-shaped.
fn code_of(error: &Value) -> String {
    ["code", "type"]
        .iter()
        .filter_map(|k| error.get(*k).and_then(Value::as_str))
        .find(|c| is_code_shaped(c))
        .unwrap_or("unknown_error")
        .to_string()
}

/// Every `data:` payload of an SSE body, parsed. Lines that are not JSON
/// (`[DONE]`, comments, a torn last line) are skipped.
fn events(body: &str) -> Vec<Value> {
    body.lines()
        .filter_map(|l| l.strip_prefix("data:"))
        .map(str::trim)
        .filter(|d| !d.is_empty() && *d != "[DONE]")
        .filter_map(|d| serde_json::from_str::<Value>(d).ok())
        .collect()
}

/// Read a streamed reply down to its one response object, or say why the
/// stream did not complete. The object is the terminal event's `response`;
/// if its `output` is empty, the items collected from
/// `response.output_item.done` events fill it.
pub fn normalize(body: &str) -> Result<Value, StreamFailure> {
    let mut items: Vec<(u64, Value)> = Vec::new();
    for ev in events(body) {
        match ev.get("type").and_then(Value::as_str) {
            Some("response.output_item.done") => {
                if let Some(item) = ev.get("item") {
                    let idx = ev
                        .get("output_index")
                        .and_then(Value::as_u64)
                        .unwrap_or(items.len() as u64);
                    items.push((idx, item.clone()));
                }
            }
            Some("response.completed") => {
                let mut response = ev.get("response").cloned().unwrap_or_else(|| json!({}));
                let empty = response
                    .get("output")
                    .and_then(Value::as_array)
                    .is_none_or(Vec::is_empty);
                if empty && !items.is_empty() {
                    items.sort_by_key(|(i, _)| *i);
                    response["output"] =
                        Value::Array(items.iter().map(|(_, it)| it.clone()).collect());
                }
                return Ok(response);
            }
            Some("response.failed") => {
                let error = ev
                    .pointer("/response/error")
                    .cloned()
                    .unwrap_or(Value::Null);
                return Err(StreamFailure::Failed {
                    code: code_of(&error),
                });
            }
            Some("response.incomplete") => {
                let reason = ev
                    .pointer("/response/incomplete_details/reason")
                    .and_then(Value::as_str)
                    .filter(|r| is_code_shaped(r))
                    .unwrap_or("unknown")
                    .to_string();
                return Err(StreamFailure::Incomplete { reason });
            }
            Some("error") => {
                return Err(StreamFailure::Failed { code: code_of(&ev) });
            }
            _ => {}
        }
    }
    Err(StreamFailure::Interrupted)
}

/// A documented ChatGPT-plan error code in a non-2xx body (`{"error":{"code":
/// ...}}`), turned into the line a run reports, with its recovery.
/// `{"detail": ...}` bodies (admission failures before a stream opens) carry
/// no stable code and return `None`: they are diagnostic text only.
/// The second value says whether repeating the request could help.
pub fn http_failure(status: u16, body: &str) -> Option<(String, bool)> {
    let v: Value = serde_json::from_str(body).ok()?;
    let code = code_of(v.get("error")?);
    let g = guidance(&code)?;
    Some((
        format!("provider HTTP {status}: ChatGPT plan: {g} [{code}]"),
        !is_terminal_code(&code),
    ))
}

/// The text of the assistant's `message` items in a response.
pub fn output_texts(response: &Value) -> Vec<String> {
    response
        .get("output")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|item| item.get("type").and_then(Value::as_str) == Some("message"))
        .flat_map(|item| {
            item.get("content")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter(|c| c.get("type").and_then(Value::as_str) == Some("output_text"))
                .filter_map(|c| c.get("text").and_then(Value::as_str).map(str::to_string))
        })
        .collect()
}

/// One function call out of a response, before it is mapped to a tool.
#[derive(Debug, Clone, PartialEq)]
pub struct FunctionCall {
    pub call_id: String,
    pub name: String,
    pub arguments: String,
    /// The whole output item, to replay verbatim in the next request.
    pub item: Value,
}

pub fn function_calls(response: &Value) -> Vec<FunctionCall> {
    response
        .get("output")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|i| i.get("type").and_then(Value::as_str) == Some("function_call"))
        .filter_map(|i| {
            Some(FunctionCall {
                call_id: i.get("call_id")?.as_str()?.to_string(),
                name: i.get("name")?.as_str()?.to_string(),
                arguments: i
                    .get("arguments")
                    .and_then(Value::as_str)
                    .unwrap_or("{}")
                    .to_string(),
                item: i.clone(),
            })
        })
        .collect()
}

/// Output items that must travel with the calls into the next request:
/// everything the model produced before its first call, in order
/// (`reasoning` items with their encrypted content, and any `message`).
/// Replaying a `function_call` without the reasoning item it followed is
/// refused when the history is replayed statelessly.
pub fn leading_items(response: &Value) -> Vec<Value> {
    response
        .get("output")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .take_while(|i| i.get("type").and_then(Value::as_str) != Some("function_call"))
        .filter(|i| {
            matches!(
                i.get("type").and_then(Value::as_str),
                Some("reasoning") | Some("message")
            )
        })
        .cloned()
        .collect()
}

/// Model ids for a picker from `GET /models` as a ChatGPT-plan token sees it:
/// a `models` array of `{slug, display_name, visibility}`; only
/// `visibility: "list"` entries are meant for display. Server order is kept.
pub fn parse_model_list(body: &str) -> Result<Vec<(String, String)>, String> {
    let v: Value = serde_json::from_str(body).map_err(|e| format!("invalid JSON: {e}"))?;
    let models = v
        .get("models")
        .and_then(Value::as_array)
        .ok_or("response has no models array")?;
    Ok(models
        .iter()
        .filter(|m| m.get("visibility").and_then(Value::as_str) == Some("list"))
        .filter_map(|m| {
            let slug = m.get("slug")?.as_str()?.to_string();
            let label = m
                .get("display_name")
                .and_then(Value::as_str)
                .unwrap_or(&slug)
                .to_string();
            Some((slug, label))
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sse(events: &[Value]) -> String {
        events
            .iter()
            .map(|e| {
                format!(
                    "event: {}\ndata: {}\n\n",
                    e["type"].as_str().unwrap_or("x"),
                    e
                )
            })
            .collect()
    }

    #[test]
    fn a_request_carries_what_the_route_requires_and_nothing_it_rejects() {
        let body: Value = serde_json::from_str(&request(
            "gpt-test",
            "be brief",
            &[user_item("hi")],
            Some(namespace_tools(vec![json!({
                "name":"read_file","description":"d","parameters":{"type":"object"}
            })])),
        ))
        .unwrap();
        assert_eq!(body["store"], false);
        assert_eq!(body["stream"], true);
        assert_eq!(body["instructions"], "be brief");
        assert_eq!(body["input"][0]["role"], "user");
        for banned in UNSUPPORTED_FIELDS {
            assert!(body.get(*banned).is_none(), "{banned} must never be sent");
        }
        // no system-role item either: the prompt is `instructions`
        assert!(!body["input"].to_string().contains("\"role\":\"system\""));
    }

    #[test]
    fn tools_are_grouped_in_one_namespace_and_keep_their_schemas() {
        let t = namespace_tools(vec![
            json!({"name":"a","description":"da","parameters":{"type":"object","properties":{"x":{"type":"string"}}}}),
            json!({"name":"b","description":"db","parameters":{"type":"object"}}),
        ]);
        assert_eq!(t.as_array().unwrap().len(), 1);
        assert_eq!(t[0]["type"], "namespace");
        assert_eq!(t[0]["name"], NAMESPACE);
        assert_eq!(t[0]["tools"][0]["type"], "function");
        assert_eq!(t[0]["tools"][0]["name"], "a");
        assert_eq!(
            t[0]["tools"][0]["parameters"]["properties"]["x"]["type"],
            "string"
        );
        assert_eq!(t[0]["tools"].as_array().unwrap().len(), 2);
    }

    #[test]
    fn the_completed_event_is_the_reply() {
        let body = sse(&[
            json!({"type":"response.created","response":{"id":"r"}}),
            json!({"type":"response.output_text.delta","delta":"Hel"}),
            json!({"type":"response.completed","response":{
                "output":[{"type":"message","role":"assistant","content":[{"type":"output_text","text":"Hello"}]}],
                "usage":{"input_tokens":10,"output_tokens":3,"total_tokens":13}}}),
        ]);
        let r = normalize(&body).unwrap();
        assert_eq!(output_texts(&r), ["Hello"]);
        // the usage the loop's token accounting reads is kept on the reply
        assert_eq!(r["usage"]["total_tokens"], 13);
    }

    #[test]
    fn items_streamed_before_an_empty_completed_output_fill_it_in_order() {
        let body = sse(&[
            json!({"type":"response.output_item.done","output_index":1,"item":{"type":"function_call","call_id":"c1","name":"read_file","arguments":"{}"}}),
            json!({"type":"response.output_item.done","output_index":0,"item":{"type":"reasoning","id":"rs_1","encrypted_content":"abc"}}),
            json!({"type":"response.completed","response":{"output":[],"usage":{"input_tokens":1,"output_tokens":1}}}),
        ]);
        let r = normalize(&body).unwrap();
        let out = r["output"].as_array().unwrap();
        assert_eq!(out[0]["type"], "reasoning");
        assert_eq!(out[1]["type"], "function_call");
    }

    #[test]
    fn a_stream_without_a_terminal_event_is_an_interruption_not_a_success() {
        let body = sse(&[
            json!({"type":"response.created","response":{}}),
            json!({"type":"response.output_text.delta","delta":"partial"}),
        ]);
        assert_eq!(normalize(&body), Err(StreamFailure::Interrupted));
        assert_eq!(normalize(""), Err(StreamFailure::Interrupted));
        // a torn last line is skipped, not fatal
        let torn = format!(
            "{}data: {{\"type\":\"response.compl",
            sse(&[json!({"type":"response.created"})])
        );
        assert_eq!(normalize(&torn), Err(StreamFailure::Interrupted));
    }

    #[test]
    fn failures_carry_only_an_identifier_shaped_code() {
        let body = sse(&[json!({"type":"response.failed","response":{"error":{
            "code":"subscription_sharing_usage_limit_exceeded",
            "message":"you sent: SECRET prompt fragment"}}})]);
        let f = normalize(&body).unwrap_err();
        let d = f.detail();
        assert!(d.contains("subscription_sharing_usage_limit_exceeded"));
        assert!(d.contains(USAGE_SETTINGS_URL));
        assert!(
            !d.contains("SECRET"),
            "provider text must not reach durable state"
        );
        // a hostile code is not kept
        let body =
            sse(&[json!({"type":"response.failed","response":{"error":{"code":"x y\nz; drop"}}})]);
        assert_eq!(
            normalize(&body),
            Err(StreamFailure::Failed {
                code: "unknown_error".into()
            })
        );
        // an `error` event too
        let body = sse(&[json!({"type":"error","code":"rate_limit_exceeded","message":"m"})]);
        assert_eq!(
            normalize(&body),
            Err(StreamFailure::Failed {
                code: "rate_limit_exceeded".into()
            })
        );
    }

    #[test]
    fn a_documented_http_error_gets_its_recovery_and_a_retry_verdict() {
        let limit =
            r#"{"error":{"code":"subscription_sharing_usage_limit_exceeded","message":"x"}}"#;
        let (line, retry) = http_failure(429, limit).unwrap();
        assert!(line.contains("HTTP 429") && line.contains("chatgpt.com/settings/usage"));
        assert!(!retry, "a usage limit is not cleared by asking again");
        let busy = r#"{"error":{"code":"subscription_sharing_usage_unavailable"}}"#;
        assert!(
            http_failure(503, busy).unwrap().1,
            "a temporary condition may be retried"
        );
        // admission failures and unknown codes carry no stable meaning
        assert!(http_failure(403, r#"{"detail":"region not permitted"}"#).is_none());
        assert!(http_failure(400, r#"{"error":{"code":"something_else"}}"#).is_none());
        assert!(http_failure(500, "not json").is_none());
    }

    #[test]
    fn incomplete_responses_name_their_reason() {
        let body = sse(&[
            json!({"type":"response.incomplete","response":{"incomplete_details":{"reason":"max_output_tokens"}}}),
        ]);
        assert_eq!(
            normalize(&body),
            Err(StreamFailure::Incomplete {
                reason: "max_output_tokens".into()
            })
        );
    }

    #[test]
    fn every_documented_code_has_guidance_and_the_terminal_ones_are_marked() {
        for code in [
            "subscription_sharing_user_not_eligible",
            "subscription_sharing_usage_limit_exceeded",
            "subscription_sharing_usage_unavailable",
            "subscription_sharing_unsupported_capability",
            "subscription_sharing_route_not_supported",
            "subscription_sharing_invalid_user",
            "chatpass_v2_scope_not_authorized",
            "chatpass_v2_invalid_authorization_context",
            "subscription_sharing_user_unavailable",
        ] {
            assert!(guidance(code).is_some(), "{code}");
        }
        assert!(guidance("some_other_code").is_none());
        // temporary conditions are retried with backoff; the rest are not
        assert!(!is_terminal_code("subscription_sharing_usage_unavailable"));
        assert!(!is_terminal_code("subscription_sharing_user_unavailable"));
        assert!(is_terminal_code(
            "subscription_sharing_usage_limit_exceeded"
        ));
        assert!(is_terminal_code("subscription_sharing_user_not_eligible"));
    }

    #[test]
    fn calls_and_leading_items_are_read_from_the_output() {
        let r = json!({"output":[
            {"type":"reasoning","id":"rs_1","encrypted_content":"x"},
            {"type":"message","role":"assistant","content":[{"type":"output_text","text":"let me look"}]},
            {"type":"function_call","id":"fc_1","call_id":"call_1","name":"read_file","namespace":"rex","arguments":"{\"path\":\"a\"}"},
            {"type":"function_call","id":"fc_2","call_id":"call_2","name":"glob_files","namespace":"rex","arguments":"{}"}
        ]});
        let calls = function_calls(&r);
        assert_eq!(calls.len(), 2);
        assert_eq!(calls[0].call_id, "call_1");
        assert_eq!(calls[0].name, "read_file");
        assert_eq!(
            calls[0].item["namespace"], "rex",
            "the whole item is kept for replay"
        );
        let lead = leading_items(&r);
        assert_eq!(lead.len(), 2);
        assert_eq!(lead[0]["type"], "reasoning");
        assert_eq!(lead[1]["type"], "message");
    }

    #[test]
    fn the_model_list_keeps_listed_models_in_server_order() {
        let body = json!({"models":[
            {"slug":"m-b","display_name":"Model B","visibility":"list"},
            {"slug":"hidden","display_name":"Hidden","visibility":"hide"},
            {"slug":"m-a","visibility":"list"}
        ]})
        .to_string();
        assert_eq!(
            parse_model_list(&body).unwrap(),
            [
                ("m-b".to_string(), "Model B".to_string()),
                ("m-a".to_string(), "m-a".to_string())
            ]
        );
        assert!(parse_model_list("{}").is_err());
        assert!(parse_model_list("nope").is_err());
    }
}
