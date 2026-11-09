//! Recognising "the request does not fit the context window".
//!
//! Estimating the window will sometimes be wrong: a table floor that is too
//! generous, a local server started with a small `num_ctx`, a tokenizer
//! denser than the estimate. The provider then rejects the request, and the
//! right response is to compact harder and retry, not to end the run.
//!
//! Provider bodies are redacted before they reach durable state (see
//! `provider_failure`), so this check has to run where the raw body is
//! still in hand, and it keeps only a yes or no.
//!
//! The phrases below are paraphrased from each provider's documented
//! context-length errors. They were not captured from live calls. A provider
//! that words the error differently is simply not recognised, and the run
//! fails with the provider's own error as it did before this module existed.

/// Appended to the failure detail so the loop can tell an overflow from any
/// other 4xx without parsing provider text again.
pub const OVERFLOW_MARK: &str = "context_window_exceeded";

const PHRASES: &[&str] = &[
    // OpenAI and compatible hosts (OpenRouter, DeepSeek, xAI, Mistral)
    "context_length_exceeded",
    "maximum context length",
    "reduce the length of the messages",
    // Anthropic
    "prompt is too long",
    "exceed context limit",
    // Google
    "exceeds the maximum number of tokens",
    "input token count",
    // llama.cpp, Ollama, vLLM and other local servers
    "exceeds the available context size",
    "context size has been exceeded",
    "exceeds the context window",
    "n_ctx",
    // generic
    "context window",
    "prompt too long",
    "too many tokens in the prompt",
];

/// Wording that marks a quota, not a window. Shrinking the context can help
/// a tokens-per-minute limit, but the loop already waits those out, and
/// calling it an overflow would trigger summaries for no reason.
const QUOTA_PHRASES: &[&str] = &[
    "tokens per minute",
    "tokens per day",
    "requests per minute",
    "rate limit",
    "rate_limit",
    "quota",
    "tpm",
];

/// Whether an HTTP response is a context-window overflow.
pub fn is_context_overflow(status: u16, body: &str) -> bool {
    if !matches!(status, 400 | 413 | 422) {
        return false;
    }
    let lower = body.to_ascii_lowercase();
    if QUOTA_PHRASES.iter().any(|q| lower.contains(q)) {
        return false;
    }
    PHRASES.iter().any(|p| lower.contains(p))
}

/// Whether a failure detail produced by the loop carries the overflow mark.
pub fn detail_is_overflow(detail: &str) -> bool {
    detail.contains(OVERFLOW_MARK)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn documented_overflow_messages_are_recognised() {
        let cases: &[(u16, &str)] = &[
            (
                400,
                r#"{"error":{"code":"context_length_exceeded","message":"This model's maximum context length is 128000 tokens."}}"#,
            ),
            (
                400,
                r#"{"type":"error","error":{"type":"invalid_request_error","message":"prompt is too long: 215000 tokens > 200000 maximum"}}"#,
            ),
            (
                400,
                r#"{"error":{"code":400,"message":"The input token count (1200000) exceeds the maximum number of tokens allowed (1048576).","status":"INVALID_ARGUMENT"}}"#,
            ),
            (
                400,
                r#"{"error":{"message":"the request exceeds the available context size, try increasing it"}}"#,
            ),
            (
                413,
                r#"{"error":"Request too large: maximum context length exceeded"}"#,
            ),
            (
                400,
                r#"{"detail":"Input is too long for the context window of this model"}"#,
            ),
        ];
        for (status, body) in cases {
            assert!(is_context_overflow(*status, body), "{body}");
        }
    }

    #[test]
    fn quotas_and_unrelated_errors_are_not_overflows() {
        // Groq's free tier answers an oversized request with a 413 that is a
        // tokens-per-minute quota, not a window problem.
        assert!(!is_context_overflow(
            413,
            r#"{"error":{"message":"Request too large for model on tokens per minute (TPM): Limit 8000, Requested 9000","type":"tokens"}}"#
        ));
        assert!(!is_context_overflow(
            400,
            r#"{"error":{"message":"invalid api key"}}"#
        ));
        assert!(!is_context_overflow(
            400,
            r#"{"error":{"message":"tool schema is invalid"}}"#
        ));
        assert!(!is_context_overflow(400, ""));
    }

    #[test]
    fn only_client_error_statuses_count() {
        let body = r#"{"error":{"code":"context_length_exceeded"}}"#;
        for status in [200, 401, 403, 404, 429, 500, 503] {
            assert!(!is_context_overflow(status, body), "{status}");
        }
        for status in [400, 413, 422] {
            assert!(is_context_overflow(status, body), "{status}");
        }
    }

    #[test]
    fn matching_ignores_case() {
        assert!(is_context_overflow(400, "PROMPT IS TOO LONG"));
    }

    #[test]
    fn the_mark_round_trips_through_a_detail_string() {
        let detail = format!("provider HTTP 400: {OVERFLOW_MARK}");
        assert!(detail_is_overflow(&detail));
        assert!(!detail_is_overflow(
            "provider HTTP 400: invalid_request_error"
        ));
    }
}
