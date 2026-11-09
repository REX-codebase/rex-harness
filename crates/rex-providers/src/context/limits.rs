//! What a model's context window is, and how sure REX is about it.
//!
//! The catalog normalises only `id`, `label` and `provider`, so the window
//! is not known from the wire. This module resolves it in three steps, and
//! always says which step answered:
//!
//! 1. an explicit override (run option or `REX_CONTEXT_WINDOW`),
//! 2. a table of model families,
//! 3. a conservative default.
//!
//! The table holds **floors, not marketing maxima**. Compacting a little
//! early costs some summary tokens; overflowing a real window fails a turn.
//! A provider that serves a larger window than listed is used fully by
//! passing the override. Local and custom endpoints skip the table on
//! purpose: the same model name behind Ollama or llama.cpp commonly runs
//! with a far smaller `num_ctx` than the model's architectural maximum, and
//! REX cannot see that setting.

use serde::{Deserialize, Serialize};

/// Window assumed when nothing better is known (tokens).
pub const DEFAULT_CONTEXT_WINDOW: u64 = 32_768;
/// Output reserve. This matches the `max_tokens` the loop already sends.
pub const DEFAULT_MAX_OUTPUT: u64 = 8_192;
/// Smallest override accepted. Below this the system prompt and tool
/// declarations alone would not fit.
pub const MIN_WINDOW: u64 = 8_192;
/// Largest override accepted.
pub const MAX_WINDOW: u64 = 10_000_000;
/// Working-set ceiling used when the window comes from the table or the
/// default rather than an explicit override. Very large windows are priced
/// per input token and long contexts degrade retrieval quality, so REX does
/// not fill a 1M window just because it exists. An explicit override lifts
/// the ceiling to the override.
pub const DEFAULT_WORKING_CAP: u64 = 400_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LimitSource {
    /// The caller or the environment said so.
    Override,
    /// A model-family floor from [`FAMILIES`].
    Table,
    /// Nothing known: [`DEFAULT_CONTEXT_WINDOW`].
    Default,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct ModelLimits {
    pub context_window: u64,
    pub max_output: u64,
    pub source: LimitSource,
}

/// (substring of the lower-cased model id, window floor in tokens). First
/// match wins, so more specific needles come first.
const FAMILIES: &[(&str, u64)] = &[
    // Anthropic: every current family is at least 200k.
    ("claude", 200_000),
    // Google: the Gemini 1.5/2.x/3 lines serve 1M or 2M. Gemma models are
    // served from the same API with far smaller windows (8k for Gemma 2),
    // so they are listed first and land below the windowed-mode minimum.
    ("gemma", 8_192),
    ("gemini-1.5-pro", 1_000_000),
    ("gemini", 1_000_000),
    // OpenAI.
    ("gpt-4.1", 1_000_000),
    ("gpt-5", 400_000),
    ("gpt-4o", 128_000),
    ("gpt-4", 128_000),
    ("o1", 200_000),
    ("o3", 200_000),
    ("o4", 200_000),
    // xAI.
    ("grok", 128_000),
    // Open-weight families served by hosted providers.
    ("deepseek", 128_000),
    ("kimi", 128_000),
    ("moonshot", 128_000),
    ("glm", 128_000),
    ("qwen", 128_000),
    ("llama-3.1", 128_000),
    ("llama-3.3", 128_000),
    ("llama-4", 128_000),
    ("nemotron", 128_000),
    ("mixtral", 32_000),
    ("mistral", 32_000),
];

/// Provider-wide floors, used when the model id matched nothing above (or is
/// not known yet: a run's brief is written before its model is picked).
/// Only providers whose every current model clears the floor are listed.
const PROVIDER_FLOORS: &[(&str, u64)] = &[("anthropic", 200_000), ("gemini", 1_000_000)];

/// Providers whose endpoint, not the model name, decides the window.
fn endpoint_decides(provider: &str) -> bool {
    matches!(provider, "local" | "custom")
}

/// A positive integer from `REX_CONTEXT_WINDOW`, if set and sane.
pub fn env_override() -> Option<u64> {
    std::env::var("REX_CONTEXT_WINDOW")
        .ok()
        .and_then(|v| parse_window(&v))
}

/// `"1_000_000"`, `" 65536 "` -> tokens; anything else, or a value outside
/// the accepted range, is `None`.
fn parse_window(text: &str) -> Option<u64> {
    text.trim()
        .replace('_', "")
        .parse::<u64>()
        .ok()
        .and_then(clamp_override)
}

fn clamp_override(v: u64) -> Option<u64> {
    (MIN_WINDOW..=MAX_WINDOW).contains(&v).then_some(v)
}

/// Resolve the window for `model` on `provider`.
pub fn resolve(provider: &str, model: &str, override_window: Option<u64>) -> ModelLimits {
    if let Some(w) = override_window.and_then(clamp_override) {
        return ModelLimits {
            context_window: w,
            max_output: DEFAULT_MAX_OUTPUT,
            source: LimitSource::Override,
        };
    }
    if !endpoint_decides(provider) {
        let id = model.to_ascii_lowercase();
        if let Some((_, w)) = FAMILIES.iter().find(|(needle, _)| id.contains(needle)) {
            return ModelLimits {
                context_window: *w,
                max_output: DEFAULT_MAX_OUTPUT,
                source: LimitSource::Table,
            };
        }
        if let Some((_, w)) = PROVIDER_FLOORS.iter().find(|(p, _)| *p == provider) {
            return ModelLimits {
                context_window: *w,
                max_output: DEFAULT_MAX_OUTPUT,
                source: LimitSource::Table,
            };
        }
    }
    ModelLimits {
        context_window: DEFAULT_CONTEXT_WINDOW,
        max_output: DEFAULT_MAX_OUTPUT,
        source: LimitSource::Default,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn override_wins_and_is_clamped() {
        let l = resolve("anthropic", "claude-sonnet-5-5", Some(1_000_000));
        assert_eq!(l.context_window, 1_000_000);
        assert_eq!(l.source, LimitSource::Override);
        // below the floor or above the ceiling the override is ignored,
        // not silently clamped to something the caller did not ask for
        let low = resolve("anthropic", "claude-sonnet-5-5", Some(100));
        assert_eq!(low.source, LimitSource::Table);
        let high = resolve("anthropic", "claude-sonnet-5-5", Some(MAX_WINDOW + 1));
        assert_eq!(high.source, LimitSource::Table);
    }

    #[test]
    fn families_resolve_case_insensitively_and_most_specific_first() {
        assert_eq!(
            resolve("anthropic", "Claude-Opus-5-5", None).context_window,
            200_000
        );
        assert_eq!(
            resolve("gemini", "gemini-2.5-flash", None).context_window,
            1_000_000
        );
        assert_eq!(
            resolve("openai", "gpt-4.1-mini", None).context_window,
            1_000_000
        );
        // gpt-4o must not fall into the gpt-4.1 or gpt-5 rows
        assert_eq!(
            resolve("openai", "gpt-4o-mini", None).context_window,
            128_000
        );
        assert_eq!(
            resolve("groq", "llama-3.3-70b-versatile", None).context_window,
            128_000
        );
        assert_eq!(
            resolve("nvidia", "mistralai/mixtral-8x7b", None).context_window,
            32_000
        );
    }

    #[test]
    fn a_provider_floor_answers_before_the_model_is_known() {
        // the brief is written before the model is chosen
        assert_eq!(resolve("anthropic", "", None).context_window, 200_000);
        assert_eq!(resolve("gemini", "", None).context_window, 1_000_000);
        assert_eq!(resolve("openai", "", None).source, LimitSource::Default);
        // a Gemma model on the Gemini API must not inherit the Gemini floor
        let gemma = resolve("gemini", "gemma-2-9b-it", None);
        assert_eq!(gemma.context_window, 8_192);
        assert!(
            crate::context::policy::ContextPolicy::for_limits(&gemma).is_none(),
            "too small for windowed mode, so the stateless loop is kept"
        );
    }

    #[test]
    fn unknown_models_and_endpoint_decided_providers_get_the_safe_default() {
        let unknown = resolve("openrouter", "acme/mystery-7b", None);
        assert_eq!(unknown.context_window, DEFAULT_CONTEXT_WINDOW);
        assert_eq!(unknown.source, LimitSource::Default);
        // A model name Ollama serves with its own num_ctx is not trusted.
        for provider in ["local", "custom"] {
            let l = resolve(provider, "llama-3.1-8b", None);
            assert_eq!(l.source, LimitSource::Default, "{provider}");
            assert_eq!(l.context_window, DEFAULT_CONTEXT_WINDOW);
        }
        // ...unless the operator states the window.
        let stated = resolve("local", "llama-3.1-8b", Some(65_536));
        assert_eq!(stated.context_window, 65_536);
        assert_eq!(stated.source, LimitSource::Override);
    }

    #[test]
    fn override_text_accepts_digits_and_underscores_inside_the_range_only() {
        assert_eq!(parse_window("1_000_000"), Some(1_000_000));
        assert_eq!(parse_window(" 65536\n"), Some(65_536));
        assert_eq!(parse_window("8192"), Some(8_192));
        assert_eq!(parse_window("8191"), None);
        assert_eq!(parse_window("200k"), None, "no unit suffixes: say 200000");
        assert_eq!(parse_window("-5"), None);
        assert_eq!(parse_window(""), None);
        assert_eq!(parse_window(&(MAX_WINDOW + 1).to_string()), None);
        assert_eq!(parse_window(&MAX_WINDOW.to_string()), Some(MAX_WINDOW));
    }
}
