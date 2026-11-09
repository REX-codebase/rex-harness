//! Token estimation, calibrated against what the provider reports.
//!
//! REX has no tokenizer for every provider it speaks to, and shipping one
//! per model family would be both heavy and wrong for most hosted models.
//! The standard answer (Claude Code, Codex, opencode all do a version of
//! this) is a bytes-per-token ratio, corrected by the usage numbers each
//! response returns. This estimator starts from a conservative ratio for
//! code and JSON and learns the real one for the run's model from the
//! provider's own `prompt tokens` figure.
//!
//! The correction is deliberately **asymmetric**:
//!
//! * If the provider says the request held *more* tokens than estimated
//!   (the ratio is too high), the estimator moves toward it quickly. An
//!   underestimate is the dangerous direction: it is how a window overflows.
//! * If the provider says it held *fewer* (the ratio is too low), the
//!   estimator moves slowly. An overestimate only compacts a little early,
//!   and an inflated request (a base64 image counts many bytes and few
//!   tokens) must not teach the estimator to under-count text.

use serde::{Deserialize, Serialize};

/// Starting ratio. English prose is nearer 4 bytes per token, source code
/// and JSON nearer 3 to 3.5; the lower number errs toward more tokens.
pub const DEFAULT_BYTES_PER_TOKEN: f64 = 3.5;
const MIN_BYTES_PER_TOKEN: f64 = 1.5;
const MAX_BYTES_PER_TOKEN: f64 = 6.0;
/// Samples smaller than this are mostly fixed overhead and teach nothing.
const MIN_SAMPLE_BYTES: usize = 1_024;
const MIN_SAMPLE_TOKENS: u64 = 256;
/// Weight of a sample that says "more tokens than estimated".
const ALPHA_UP: f64 = 0.7;
/// Weight of a sample that says "fewer tokens than estimated".
const ALPHA_DOWN: f64 = 0.2;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct TokenEstimator {
    bytes_per_token: f64,
    samples: u32,
}

impl Default for TokenEstimator {
    fn default() -> Self {
        Self {
            bytes_per_token: DEFAULT_BYTES_PER_TOKEN,
            samples: 0,
        }
    }
}

impl TokenEstimator {
    /// Restore a ratio saved in a run's view log. Out-of-range or non-finite
    /// values fall back to the default rather than poisoning the estimate.
    pub fn with_ratio(bytes_per_token: f64) -> Self {
        let r = if bytes_per_token.is_finite() {
            bytes_per_token.clamp(MIN_BYTES_PER_TOKEN, MAX_BYTES_PER_TOKEN)
        } else {
            DEFAULT_BYTES_PER_TOKEN
        };
        Self {
            bytes_per_token: r,
            samples: 1,
        }
    }

    pub fn ratio(&self) -> f64 {
        self.bytes_per_token
    }

    pub fn samples(&self) -> u32 {
        self.samples
    }

    /// Estimated tokens in `bytes` of serialized request or message text.
    pub fn tokens(&self, bytes: usize) -> u64 {
        (bytes as f64 / self.bytes_per_token).ceil() as u64
    }

    /// Fold in one observation: a request of `request_bytes` that the
    /// provider counted as `reported_prompt_tokens` input tokens. Returns
    /// whether the sample was used.
    pub fn calibrate(&mut self, request_bytes: usize, reported_prompt_tokens: u64) -> bool {
        if request_bytes < MIN_SAMPLE_BYTES || reported_prompt_tokens < MIN_SAMPLE_TOKENS {
            return false;
        }
        let observed = (request_bytes as f64 / reported_prompt_tokens as f64)
            .clamp(MIN_BYTES_PER_TOKEN, MAX_BYTES_PER_TOKEN);
        // observed < current: the request held more tokens than we thought.
        let alpha = if observed < self.bytes_per_token {
            ALPHA_UP
        } else {
            ALPHA_DOWN
        };
        self.bytes_per_token += alpha * (observed - self.bytes_per_token);
        self.bytes_per_token = self
            .bytes_per_token
            .clamp(MIN_BYTES_PER_TOKEN, MAX_BYTES_PER_TOKEN);
        self.samples = self.samples.saturating_add(1);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_ratio_rounds_up() {
        let e = TokenEstimator::default();
        assert_eq!(e.tokens(0), 0);
        assert_eq!(e.tokens(1), 1);
        assert_eq!(e.tokens(7), 2);
        assert_eq!(e.tokens(3_500), 1_000);
    }

    #[test]
    fn learns_a_denser_tokenizer_quickly() {
        // 100 KB that the provider counts as 50k tokens is 2 bytes/token
        let mut e = TokenEstimator::default();
        assert!(e.calibrate(100_000, 50_000));
        assert!(
            e.ratio() < 2.5,
            "an underestimate must be corrected fast, got {}",
            e.ratio()
        );
        assert!(e.tokens(100_000) > 38_000);
    }

    #[test]
    fn learns_a_sparser_tokenizer_slowly() {
        // an image-heavy request: 100 KB that is only 10k tokens
        let mut e = TokenEstimator::default();
        assert!(e.calibrate(100_000, 10_000));
        assert!(
            e.ratio() < 5.0,
            "an overestimate must be corrected slowly, got {}",
            e.ratio()
        );
        assert!(e.ratio() > DEFAULT_BYTES_PER_TOKEN);
    }

    #[test]
    fn tiny_or_empty_samples_are_ignored() {
        let mut e = TokenEstimator::default();
        assert!(!e.calibrate(200, 1_000));
        assert!(!e.calibrate(50_000, 10));
        assert!(!e.calibrate(0, 0));
        assert_eq!(e, TokenEstimator::default());
    }

    #[test]
    fn ratio_stays_in_bounds_whatever_the_provider_reports() {
        let mut e = TokenEstimator::default();
        for _ in 0..50 {
            e.calibrate(10_000_000, 256);
        }
        assert!(e.ratio() <= MAX_BYTES_PER_TOKEN);
        for _ in 0..50 {
            e.calibrate(1_100, 10_000_000);
        }
        assert!(e.ratio() >= MIN_BYTES_PER_TOKEN);
    }

    #[test]
    fn a_restored_ratio_is_sanitised() {
        assert_eq!(
            TokenEstimator::with_ratio(f64::NAN).ratio(),
            DEFAULT_BYTES_PER_TOKEN
        );
        assert_eq!(
            TokenEstimator::with_ratio(f64::INFINITY).ratio(),
            DEFAULT_BYTES_PER_TOKEN
        );
        assert_eq!(
            TokenEstimator::with_ratio(0.01).ratio(),
            MIN_BYTES_PER_TOKEN
        );
        assert_eq!(
            TokenEstimator::with_ratio(99.0).ratio(),
            MAX_BYTES_PER_TOKEN
        );
        assert_eq!(TokenEstimator::with_ratio(4.2).ratio(), 4.2);
    }
}
