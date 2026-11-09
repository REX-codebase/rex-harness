//! When to clear, when to compact, and how much to protect.
//!
//! Everything is a fraction of one number, the **input budget**: the part of
//! the window that is left for the request once the output reserve and a
//! safety margin are taken out. Thresholds are fractions of that budget, not
//! of the raw window, so they behave the same for an 8k local model and a
//! 1M hosted one.
//!
//! ```text
//!  0%            55%                75%             90%      100% of input budget
//!  |--- Low ------|--- Elevated -----|--- High ------|-- Critical
//!                  clear old tool     summarise the    summarise now,
//!                  results + big      old prefix       shrink the tail
//!                  tool arguments
//! ```

use super::limits::{LimitSource, ModelLimits, DEFAULT_WORKING_CAP};
use serde::{Deserialize, Serialize};

/// Below this the fixed part of every request (the system prompt and tool
/// declarations, measured at about 27 KB or 7.7k tokens for the full
/// offering) takes so large a share of the budget that there is no room for
/// a transcript worth keeping: every turn would compact. Windowed mode then
/// declines to run and the loop keeps its stateless behaviour. 20k tokens
/// keeps the fixed part near a third of the budget; it corresponds to a
/// window of roughly 29k tokens.
pub const MIN_INPUT_BUDGET: u64 = 20_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Pressure {
    Low,
    Elevated,
    High,
    Critical,
}

impl Pressure {
    pub fn label(self) -> &'static str {
        match self {
            Pressure::Low => "low",
            Pressure::Elevated => "elevated",
            Pressure::High => "high",
            Pressure::Critical => "critical",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ContextPolicy {
    /// The provider window (or override) in tokens.
    pub window: u64,
    /// Tokens kept free for the model's reply.
    pub max_output: u64,
    /// Never plan to use more than this many tokens of the window.
    pub working_cap: u64,
    pub source: LimitSource,
    pub clear_at: f64,
    pub compact_at: f64,
    pub emergency_at: f64,
    /// Newest turns that always stay at full fidelity.
    pub tail_turns: usize,
    /// Share of the input budget the protected tail may use.
    pub tail_fraction: f64,
}

impl ContextPolicy {
    /// `None` when the window leaves less than [`MIN_INPUT_BUDGET`] tokens.
    pub fn for_limits(limits: &ModelLimits) -> Option<Self> {
        let working_cap = if limits.source == LimitSource::Override {
            // An operator who states a window means to use it.
            limits.context_window
        } else {
            limits.context_window.min(DEFAULT_WORKING_CAP)
        };
        let policy = Self {
            window: limits.context_window,
            max_output: limits.max_output,
            working_cap,
            source: limits.source,
            clear_at: 0.55,
            compact_at: 0.75,
            emergency_at: 0.90,
            tail_turns: 3,
            tail_fraction: 0.15,
        };
        (policy.input_budget() >= MIN_INPUT_BUDGET).then_some(policy)
    }

    /// Tokens of the window usable for the request.
    pub fn input_budget(&self) -> u64 {
        let usable = self.window.min(self.working_cap);
        // 3% margin for estimator error, never less than 512 tokens.
        let margin = (usable / 32).max(512);
        usable.saturating_sub(self.max_output + margin)
    }

    pub fn pressure(&self, est_input_tokens: u64) -> Pressure {
        let b = self.input_budget().max(1) as f64;
        let used = est_input_tokens as f64 / b;
        if used >= self.emergency_at {
            Pressure::Critical
        } else if used >= self.compact_at {
            Pressure::High
        } else if used >= self.clear_at {
            Pressure::Elevated
        } else {
            Pressure::Low
        }
    }

    pub fn threshold_tokens(&self, fraction: f64) -> u64 {
        (self.input_budget() as f64 * fraction) as u64
    }

    /// Tokens the protected tail may occupy.
    pub fn tail_budget(&self) -> u64 {
        self.threshold_tokens(self.tail_fraction)
    }

    /// Largest share of the budget one tool result may take at full
    /// fidelity. A longer result is shown head-and-tail and stays whole in
    /// the log, so a single huge read cannot overflow a small window.
    pub fn result_cap_tokens(&self) -> u64 {
        self.threshold_tokens(0.25)
    }

    /// Characters allowed in a compaction summary.
    pub fn summary_max_chars(&self) -> usize {
        let chars = (self.input_budget() as f64 * 0.05 * 3.5) as usize;
        chars.clamp(3_000, 14_000)
    }

    /// Characters of old turns one summarizer request may carry. The request
    /// also holds the prompt, the previous summary and the reply, so the turn
    /// text takes under half the budget; large windows are capped because a
    /// summarizer does better on focused chunks than on a flood.
    pub fn summary_chunk_chars(&self) -> usize {
        ((self.input_budget() as f64 * 0.45 * 3.5) as usize).clamp(6_000, 60_000)
    }

    /// A provider just rejected a request estimated at `estimated_tokens`
    /// as too long, so the true window is smaller than assumed. Plan around
    /// 85% of that request from now on, never below the minimum budget.
    pub fn tightened_to(&self, estimated_tokens: u64) -> Self {
        let mut p = *self;
        let want = ((estimated_tokens as f64 * 0.85) as u64).max(MIN_INPUT_BUDGET);
        p.working_cap = p.working_cap.min(self.cap_for_budget(want));
        p
    }

    /// The working cap whose input budget is `budget` (or a token or two
    /// above it): the inverse of [`Self::input_budget`], which subtracts the
    /// output reserve and a margin of 1/32 of the usable window (at least
    /// 512 tokens).
    fn cap_for_budget(&self, budget: u64) -> u64 {
        let needed = budget + self.max_output;
        let cap = (needed as f64 * 32.0 / 31.0).ceil() as u64;
        if cap / 32 >= 512 {
            cap
        } else {
            needed + 512
        }
    }

    /// Spend ceiling for a run that did not choose its own budget. A
    /// windowed run re-sends its history on every turn, so the stateless
    /// default (250k tokens) would end it after a handful of turns. The
    /// ceiling is twenty full windows of input, never above the hard clamp
    /// the loop applies to every budget.
    pub fn default_run_token_budget(&self, stateless_default: u64, hard_max: u64) -> u64 {
        (self.input_budget().saturating_mul(20))
            .max(stateless_default)
            .min(hard_max)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::limits::{resolve, DEFAULT_MAX_OUTPUT};

    fn policy(window: u64) -> ContextPolicy {
        ContextPolicy::for_limits(&ModelLimits {
            context_window: window,
            max_output: DEFAULT_MAX_OUTPUT,
            source: LimitSource::Override,
        })
        .unwrap()
    }

    #[test]
    fn windows_too_small_for_the_prompt_are_declined() {
        let tiny = ModelLimits {
            context_window: 16_384,
            max_output: DEFAULT_MAX_OUTPUT,
            source: LimitSource::Override,
        };
        assert!(ContextPolicy::for_limits(&tiny).is_none());
        assert!(ContextPolicy::for_limits(&resolve("local", "x", None)).is_some());
    }

    #[test]
    fn input_budget_subtracts_output_and_margin() {
        let p = policy(200_000);
        // 200_000 - 8_192 - 6_250
        assert_eq!(p.input_budget(), 185_558);
        let small = policy(32_768);
        assert_eq!(small.input_budget(), 32_768 - 8_192 - 1_024);
    }

    #[test]
    fn a_table_window_is_capped_but_an_override_is_honoured() {
        let table = ContextPolicy::for_limits(&resolve("gemini", "gemini-2.5-pro", None)).unwrap();
        assert_eq!(table.window, 1_000_000);
        assert_eq!(table.working_cap, DEFAULT_WORKING_CAP);
        assert!(table.input_budget() < DEFAULT_WORKING_CAP);
        let stated = policy(1_000_000);
        assert_eq!(stated.working_cap, 1_000_000);
        assert!(stated.input_budget() > 900_000);
    }

    #[test]
    fn pressure_bands_follow_the_fractions_of_the_budget() {
        let p = policy(200_000);
        let b = p.input_budget();
        assert_eq!(p.pressure(0), Pressure::Low);
        assert_eq!(p.pressure(p.threshold_tokens(0.549)), Pressure::Low);
        assert_eq!(p.pressure(p.threshold_tokens(0.56)), Pressure::Elevated);
        assert_eq!(p.pressure(p.threshold_tokens(0.76)), Pressure::High);
        assert_eq!(p.pressure(p.threshold_tokens(0.91)), Pressure::Critical);
        assert_eq!(p.pressure(b * 3), Pressure::Critical);
        assert!(Pressure::Critical > Pressure::High && Pressure::High > Pressure::Elevated);
    }

    #[test]
    fn summary_and_result_caps_scale_with_the_budget_inside_bounds() {
        // budget 23_552 tokens * 5% * 3.5 chars per token = 4_121.6
        assert_eq!(policy(32_768).summary_max_chars(), 4_121);
        assert_eq!(policy(1_000_000).summary_max_chars(), 14_000);
        let p = policy(32_768);
        assert!(p.result_cap_tokens() * 4 <= p.input_budget() + 4);
    }

    #[test]
    fn chunk_size_scales_with_the_budget_inside_bounds() {
        assert_eq!(policy(32_768).summary_chunk_chars(), 37_094);
        assert_eq!(policy(1_000_000).summary_chunk_chars(), 60_000);
        // a window just above the minimum (about 29k) still gets a usable chunk
        let tight = policy(29_500);
        assert!(tight.summary_chunk_chars() >= 6_000);
        // the request (prompt + previous summary + chunk) fits the budget
        let tokens = tight.summary_chunk_chars() as f64 / 3.5;
        assert!(tokens < tight.input_budget() as f64 * 0.5);
    }

    #[test]
    fn a_rejected_request_tightens_the_window_but_never_below_the_minimum() {
        let p = policy(200_000);
        let t = p.tightened_to(120_000);
        assert!(t.input_budget() < p.input_budget());
        // 85% of the 120k request that was refused, to within rounding
        assert!(
            (102_000..=102_010).contains(&t.input_budget()),
            "got {}",
            t.input_budget()
        );
        // it never loosens
        assert_eq!(p.tightened_to(10_000_000).working_cap, p.working_cap);
        // nonsense small estimates are floored at the minimum viable budget
        assert!(p.tightened_to(10).input_budget() >= MIN_INPUT_BUDGET);
    }

    #[test]
    fn default_run_budget_scales_but_respects_the_hard_clamp() {
        let p = policy(200_000);
        let b = p.default_run_token_budget(250_000, 4_000_000);
        assert_eq!(b, 3_711_160);
        let huge = policy(1_000_000);
        assert_eq!(huge.default_run_token_budget(250_000, 4_000_000), 4_000_000);
        // a small window never goes below the stateless default
        let small = policy(32_768);
        // 23_552 * 20 = 471_040, above the 250k stateless default
        assert_eq!(small.default_run_token_budget(250_000, 4_000_000), 471_040);
        assert!(small.default_run_token_budget(900_000, 4_000_000) >= 900_000);
    }
}
