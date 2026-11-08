//! Redacted, machine-readable provider HTTP failures.
//!
//! Provider bodies can include prompt fragments or other request context.  We
//! never copy the free-form message into durable run state.  For Google 429s
//! we retain only the fields needed to diagnose quota and schedule a retry.

use regex::Regex;
use serde::Serialize;
use serde_json::Value;
use std::collections::BTreeMap;
use std::sync::OnceLock;

/// Longest single rate-limit wait the harness will sit through. A provider
/// that says "try again in 3h" (a daily quota) is not something a run should
/// block on; it fails with the number so the person can decide.
pub(crate) const MAX_SINGLE_WAIT_SECS: f64 = 90.0;
/// Total rate-limit waiting one provider request may accumulate.
pub(crate) const MAX_TOTAL_WAIT_SECS: f64 = 480.0;
/// Waits used when the provider gave no hint, one per consecutive 429.
const UNHINTED_BACKOFF_SECS: [f64; 5] = [2.0, 5.0, 10.0, 20.0, 20.0];

fn duration_token() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    // No trailing word boundary: "1m26.4s" has to split into "1m" and "26.4s".
    // Longer unit words come first so "ms" and "minutes" are not read as "m".
    RE.get_or_init(|| {
        Regex::new(r"(?i)(\d+(?:\.\d+)?)\s*(milliseconds?|msecs?|ms|hours?|hrs?|h|minutes?|mins?|m|seconds?|secs?|s)").expect("valid regex")
    })
}

fn retry_phrase() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"(?i)(?:try again|retry)\s+in\s+((?:\d+(?:\.\d+)?\s*(?:milliseconds?|msecs?|ms|hours?|hrs?|h|minutes?|mins?|m|seconds?|secs?|s)\s*)+)")
            .expect("valid regex")
    })
}

/// "37s", "1m26.4s", "250ms", "1h2m" -> seconds.
fn parse_duration_secs(text: &str) -> Option<f64> {
    let mut total = 0.0;
    let mut any = false;
    for cap in duration_token().captures_iter(text) {
        let n: f64 = cap[1].parse().ok()?;
        let unit = cap[2].to_ascii_lowercase();
        total += if unit.starts_with("ms") || unit.starts_with("milli") {
            n / 1000.0
        } else if unit.starts_with('h') {
            n * 3600.0
        } else if unit.starts_with('m') {
            n * 60.0
        } else {
            n
        };
        any = true;
    }
    (any && total.is_finite() && total >= 0.0).then_some(total.min(86_400.0))
}

/// Seconds the provider asked us to wait before retrying, if it said so in a
/// form we recognise: Google's `RetryInfo.retryDelay`, or the "Please try
/// again in 33.7s" sentence OpenAI-compatible hosts put in the message. Only
/// the NUMBER leaves this function. The surrounding text is never kept, so
/// the redaction rule for provider bodies still holds.
pub(crate) fn retry_hint_secs(body: &str) -> Option<f64> {
    let value: Value = serde_json::from_str(body).unwrap_or(Value::Null);
    let error = value.get("error").unwrap_or(&value);
    for detail in error
        .get("details")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let kind = detail.get("@type").and_then(Value::as_str).unwrap_or("");
        if kind.ends_with("google.rpc.RetryInfo") {
            if let Some(secs) = detail
                .get("retryDelay")
                .and_then(Value::as_str)
                .and_then(parse_duration_secs)
            {
                return Some(secs);
            }
        }
    }
    let message = error.get("message").and_then(Value::as_str).unwrap_or("");
    retry_phrase()
        .captures(message)
        .and_then(|c| parse_duration_secs(&c[1]))
}

/// What to do about a 429.
#[derive(Debug, PartialEq)]
pub(crate) enum RateWait {
    /// Sleep this many seconds, then try the same request again.
    Wait(f64),
    /// Not worth waiting for; fail the turn and surface the number.
    GiveUp,
}

/// Decide whether to wait out a rate limit. `waits_so_far` counts earlier
/// 429s for this request and `waited_total` the seconds already slept.
pub(crate) fn plan_rate_limit_wait(body: &str, waits_so_far: u32, waited_total: f64) -> RateWait {
    match retry_hint_secs(body) {
        // A half second of slack so the window has really reopened.
        Some(hint) => {
            let wait = hint + 0.5;
            if hint > MAX_SINGLE_WAIT_SECS || waited_total + wait > MAX_TOTAL_WAIT_SECS {
                RateWait::GiveUp
            } else {
                RateWait::Wait(wait)
            }
        }
        None => match UNHINTED_BACKOFF_SECS.get(waits_so_far as usize) {
            Some(wait) if waited_total + wait <= MAX_TOTAL_WAIT_SECS => RateWait::Wait(*wait),
            _ => RateWait::GiveUp,
        },
    }
}

/// A person-readable line for the activity feed. Numbers only, no provider text.
pub(crate) fn describe_wait(secs: f64) -> String {
    if secs >= 90.0 {
        format!(
            "rate limited by the provider; waiting {:.0} min before retrying",
            secs / 60.0
        )
    } else {
        format!(
            "rate limited by the provider; waiting {:.0}s before retrying",
            secs.ceil()
        )
    }
}

/// Provider error `type` / `code` strings are short identifiers
/// (`rate_limit_exceeded`, `tokens`). Keep one only if it really looks like
/// one, so no free-form sentence can ride through in either field.
fn identifier(value: Option<&Value>) -> Option<String> {
    let text = value?.as_str()?;
    (!text.is_empty()
        && text.len() <= 48
        && text
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '-')))
    .then(|| text.to_owned())
}

#[derive(Debug, Serialize)]
struct HttpFailure {
    http_status: u16,
    #[serde(skip_serializing_if = "Option::is_none")]
    provider_code: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    provider_status: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    quota_violations: Vec<QuotaViolation>,
    #[serde(skip_serializing_if = "Option::is_none")]
    retry_delay: Option<String>,
    /// Seconds the provider asked us to wait, whatever shape it used.
    #[serde(skip_serializing_if = "Option::is_none")]
    retry_after_secs: Option<f64>,
    /// Identifier-shaped `error.type` / `error.code` (OpenAI-compatible hosts).
    #[serde(skip_serializing_if = "Option::is_none")]
    provider_error_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    provider_error_code: Option<String>,
    redacted: bool,
}

#[derive(Debug, Serialize)]
struct QuotaViolation {
    #[serde(skip_serializing_if = "Option::is_none")]
    metric: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    limit: Option<String>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    scope: BTreeMap<String, String>,
}

/// Produce a stable JSON detail without preserving the provider's free-form
/// message or any unknown fields.
pub(crate) fn structured_http_failure(status: u16, body: &str) -> String {
    let value: Value = serde_json::from_str(body).unwrap_or(Value::Null);
    let error = value.get("error").unwrap_or(&value);
    let mut out = HttpFailure {
        http_status: status,
        provider_code: error.get("code").and_then(Value::as_u64),
        provider_status: error
            .get("status")
            .and_then(Value::as_str)
            .map(str::to_owned),
        quota_violations: Vec::new(),
        retry_delay: None,
        retry_after_secs: retry_hint_secs(body).map(|s| (s * 10.0).round() / 10.0),
        provider_error_type: identifier(error.get("type")),
        provider_error_code: identifier(error.get("code")),
        redacted: true,
    };

    if let Some(details) = error.get("details").and_then(Value::as_array) {
        for detail in details {
            let kind = detail.get("@type").and_then(Value::as_str).unwrap_or("");
            if kind.ends_with("google.rpc.QuotaFailure") {
                for violation in detail
                    .get("violations")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                {
                    let scope = violation
                        .get("quotaDimensions")
                        .and_then(Value::as_object)
                        .map(|m| {
                            m.iter()
                                .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_owned())))
                                .collect()
                        })
                        .unwrap_or_default();
                    out.quota_violations.push(QuotaViolation {
                        metric: violation
                            .get("quotaMetric")
                            .and_then(Value::as_str)
                            .map(str::to_owned),
                        limit: violation
                            .get("quotaId")
                            .and_then(Value::as_str)
                            .map(str::to_owned),
                        scope,
                    });
                }
            } else if kind.ends_with("google.rpc.RetryInfo") {
                out.retry_delay = detail
                    .get("retryDelay")
                    .and_then(Value::as_str)
                    .map(str::to_owned);
            }
        }
    }

    serde_json::to_string(&out).expect("HTTP failure record is serializable")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn google_429_is_structured_and_free_form_text_is_redacted() {
        let body = r#"{
          "error": {
            "code": 429,
            "status": "RESOURCE_EXHAUSTED",
            "message": "prompt fragment and key must not survive",
            "details": [
              {"@type":"type.googleapis.com/google.rpc.QuotaFailure","violations":[{
                "quotaMetric":"generativelanguage.googleapis.com/generate_content_free_tier_requests",
                "quotaId":"GenerateRequestsPerDayPerProjectPerModel-FreeTier",
                "quotaDimensions":{"location":"global","model":"gemini-3.5-flash-lite"}
              }]},
              {"@type":"type.googleapis.com/google.rpc.RetryInfo","retryDelay":"37s"}
            ]
          }
        }"#;
        let detail = structured_http_failure(429, body);
        let v: Value = serde_json::from_str(&detail).unwrap();
        assert_eq!(v["http_status"], 429);
        assert_eq!(v["provider_status"], "RESOURCE_EXHAUSTED");
        assert_eq!(
            v["quota_violations"][0]["limit"],
            "GenerateRequestsPerDayPerProjectPerModel-FreeTier"
        );
        assert_eq!(
            v["quota_violations"][0]["scope"]["model"],
            "gemini-3.5-flash-lite"
        );
        assert_eq!(v["retry_delay"], "37s");
        assert_eq!(v["redacted"], true);
        assert!(!detail.contains("prompt fragment"));
        assert!(!detail.contains("must not survive"));
    }

    // The shape Groq returned in a live run (organization id removed).
    const GROQ_429: &str = r#"{"error":{"message":"Rate limit reached for model `openai/gpt-oss-120b` in organization `org_x` service tier `on_demand` on tokens per minute (TPM): Limit 8000, Used 6062, Requested 6439. Please try again in 33.7575s. Need more tokens? Upgrade to Dev Tier today.","type":"tokens","code":"rate_limit_exceeded"}}"#;

    #[test]
    fn openai_compatible_retry_hint_is_read_as_a_number() {
        let hint = retry_hint_secs(GROQ_429).unwrap();
        assert!((hint - 33.7575).abs() < 1e-9, "{hint}");
    }

    #[test]
    fn durations_in_every_notation_parse() {
        assert_eq!(parse_duration_secs("37s"), Some(37.0));
        assert_eq!(parse_duration_secs("1m26.4s"), Some(86.4));
        assert_eq!(parse_duration_secs("250ms"), Some(0.25));
        assert_eq!(parse_duration_secs("1h2m"), Some(3720.0));
        assert_eq!(parse_duration_secs("5 minutes"), Some(300.0));
        assert_eq!(parse_duration_secs("2 seconds"), Some(2.0));
        assert_eq!(parse_duration_secs("1 hour 30 minutes"), Some(5400.0));
        assert_eq!(parse_duration_secs("500 milliseconds"), Some(0.5));
        assert_eq!(parse_duration_secs("soon"), None);
        let msg = |t: &str| format!(r#"{{"error":{{"message":"{t}"}}}}"#);
        assert_eq!(
            retry_hint_secs(&msg("Too many requests. Retry in 6.5s.")),
            Some(6.5)
        );
        assert_eq!(
            retry_hint_secs(&msg("Please try again in 1m26.4s")),
            Some(86.4)
        );
        assert_eq!(retry_hint_secs(&msg("Please try again later")), None);
    }

    #[test]
    fn google_retry_info_still_wins() {
        let body = r#"{"error":{"code":429,"details":[{"@type":"type.googleapis.com/google.rpc.RetryInfo","retryDelay":"37s"}]}}"#;
        assert_eq!(retry_hint_secs(body), Some(37.0));
    }

    #[test]
    fn the_rate_limit_record_keeps_numbers_and_identifiers_only() {
        let detail = structured_http_failure(429, GROQ_429);
        let v: Value = serde_json::from_str(&detail).unwrap();
        assert_eq!(v["retry_after_secs"], 33.8);
        assert_eq!(v["provider_error_type"], "tokens");
        assert_eq!(v["provider_error_code"], "rate_limit_exceeded");
        for leaked in ["org_x", "Limit 8000", "Upgrade", "gpt-oss", "service tier"] {
            assert!(
                !detail.contains(leaked),
                "free-form text leaked: {leaked} in {detail}"
            );
        }
        assert_eq!(v["redacted"], true);
    }

    #[test]
    fn a_sentence_in_the_type_or_code_field_cannot_ride_through() {
        let body = r#"{"error":{"type":"Please paste your key here","code":"sk-live-abc def","message":"x"}}"#;
        let detail = structured_http_failure(400, body);
        assert_eq!(detail, r#"{"http_status":400,"redacted":true}"#);
    }

    #[test]
    fn a_hint_is_waited_out_and_a_long_one_is_not() {
        match plan_rate_limit_wait(GROQ_429, 0, 0.0) {
            RateWait::Wait(s) => assert!((s - 34.2575).abs() < 1e-9, "{s}"),
            other => panic!("{other:?}"),
        }
        let daily = r#"{"error":{"message":"Please try again in 3h2m"}}"#;
        assert_eq!(plan_rate_limit_wait(daily, 0, 0.0), RateWait::GiveUp);
    }

    #[test]
    fn total_waiting_per_request_is_bounded() {
        // 80s hints are individually fine, but not forever.
        let body = r#"{"error":{"message":"Please try again in 80s"}}"#;
        let mut waited = 0.0;
        let mut waits = 0;
        while let RateWait::Wait(s) = plan_rate_limit_wait(body, waits, waited) {
            waited += s;
            waits += 1;
            assert!(waits < 20, "must terminate");
        }
        assert!(waited <= MAX_TOTAL_WAIT_SECS);
        assert!(
            waits >= 5,
            "should honour several hints before giving up: {waits}"
        );
    }

    #[test]
    fn without_a_hint_backoff_grows_then_gives_up() {
        let plain = r#"{"error":{"message":"slow down"}}"#;
        let mut got = vec![];
        let mut total = 0.0;
        for n in 0..10 {
            match plan_rate_limit_wait(plain, n, total) {
                RateWait::Wait(s) => {
                    got.push(s);
                    total += s;
                }
                RateWait::GiveUp => break,
            }
        }
        assert_eq!(got, vec![2.0, 5.0, 10.0, 20.0, 20.0]);
    }

    #[test]
    fn wait_descriptions_carry_only_a_number() {
        assert_eq!(
            describe_wait(34.2575),
            "rate limited by the provider; waiting 35s before retrying"
        );
        assert_eq!(
            describe_wait(150.0),
            "rate limited by the provider; waiting 2 min before retrying"
        );
    }

    #[test]
    fn malformed_body_stays_structured_and_redacted() {
        let detail = structured_http_failure(503, "not json and possibly sensitive");
        assert_eq!(detail, r#"{"http_status":503,"redacted":true}"#);
    }
}
