//! Access policy: which account types REX may connect for each provider, and
//! why. Agrim's rule, encoded: REX only offers a subscription hook when the
//! provider's own current terms and authentication documentation clearly
//! permit third-party harness use. Explicit bans, undocumented routes, and partner-gated routes are distinct states.
//!
//! Every verdict here is grounded in official provider sources, checked on
//! `VERIFIED_ON`. The human-readable matrix with quotes lives in
//! `docs/subscription-policy.md`. When a provider changes its terms, update
//! the entry, the doc, and the date together.

use serde::Serialize;

/// Day the sources behind the verdict matrix were last checked (YYYY-MM-DD).
/// A row checked more recently carries its own, later date.
pub const VERIFIED_ON: &str = "2026-09-19";

/// Day OpenAI's "Sign in with ChatGPT" contract was read from its own
/// documentation (developers.openai.com/siwc). Only the OpenAI ChatGPT-plan
/// rows were re-verified that day; no other row's date moves with it.
pub const OPENAI_SIWC_VERIFIED_ON: &str = "2026-11-09";

/// **The switch for using a ChatGPT plan.**
///
/// OpenAI offers ChatGPT plan usage through Sign in with ChatGPT to
/// open-source, locally hosted clients, and to selected partners and private
/// clients on request: "ChatGPT plan usage is available to all open-source
/// partners and selected private clients." REX is open source (MIT OR
/// Apache-2.0, public since 2026-11-09), so this is `true` and the route is
/// open.
///
/// Set it to `false` to close the route everywhere (sign-in, token use and
/// runs all refuse) if REX ever stops qualifying: a relicence, the repository
/// going private, or OpenAI changing its terms. A test refuses a `true` while
/// any crate declares `license = "Proprietary"` or the repository has no
/// LICENSE file.
pub const OPEN_SOURCE_RELEASE: bool = true;

/// Whether a ChatGPT-plan sign-in may start or its token be used. Unit tests
/// inside this crate run with the route open so the flow can be exercised;
/// everything else follows [`OPEN_SOURCE_RELEASE`].
pub fn chatgpt_plan_route() -> Result<(), String> {
    gate(OPEN_SOURCE_RELEASE || cfg!(test))
}

fn gate(open: bool) -> Result<(), String> {
    if open {
        Ok(())
    } else {
        Err("Using a ChatGPT plan is not available. OpenAI offers it to open-source, locally hosted apps, and this build is not released as open source. Use an OpenAI API key instead (docs/subscription-policy.md).".to_string())
    }
}

/// How a provider account would be connected.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum AccessKind {
    /// Pay-as-you-go API key from the provider's developer platform.
    ApiKey,
    /// A consumer subscription whose provider officially issues an API key or
    /// endpoint for use inside third-party tools.
    SubscriptionKey,
    /// Signing in with a consumer subscription's login/OAuth credentials.
    SubscriptionOauth,
}

/// Whether REX offers the route, per official provider documentation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum PolicyStatus {
    /// Official docs explicitly support this route for third-party tools.
    Supported,
    /// The subscription exists, but its terms limit it to the provider's own
    /// list of supported tools. REX is not on the list, so REX does not offer
    /// it; joining the list is the only legitimate unlock.
    ToolScoped,
    /// Official terms explicitly forbid the route.
    NotPermitted,
    /// Provider has not published a general contract for arbitrary harnesses.
    /// Disabled without claiming the provider bans users or that the route is illegal.
    Undocumented,
    /// Provider officially supports named third-party integrations but has not
    /// published general self-service client registration; REX needs approval.
    PartnerGated,
    /// The provider documents the route for open-source, locally hosted
    /// clients, with self-service registration. REX qualifies once it is
    /// released as open source (`OPEN_SOURCE_RELEASE`); until then the route
    /// is closed, and it becomes `Supported` the moment the switch flips.
    OpenSourceOnly,
    /// Route is not offered for another documented reason.
    NotOffered,
}

/// One possible way to connect a provider account, with its verdict.
#[derive(Debug, Clone, Serialize)]
pub struct AccessPolicy {
    pub kind: AccessKind,
    pub status: PolicyStatus,
    /// User-facing name of the route, e.g. "Anthropic API key".
    pub label: &'static str,
    /// One sentence: what the verdict is and the official reason behind it.
    pub detail: &'static str,
    /// Official provider sources the verdict is grounded in.
    pub sources: &'static [&'static str],
    pub verified_on: &'static str,
}

const fn api_key(
    label: &'static str,
    detail: &'static str,
    sources: &'static [&'static str],
) -> AccessPolicy {
    AccessPolicy {
        kind: AccessKind::ApiKey,
        status: PolicyStatus::Supported,
        label,
        detail,
        sources,
        verified_on: VERIFIED_ON,
    }
}

const fn policy(
    kind: AccessKind,
    status: PolicyStatus,
    label: &'static str,
    detail: &'static str,
    sources: &'static [&'static str],
) -> AccessPolicy {
    AccessPolicy {
        kind,
        status,
        label,
        detail,
        sources,
        verified_on: VERIFIED_ON,
    }
}

const fn policy_on(
    verified_on: &'static str,
    kind: AccessKind,
    status: PolicyStatus,
    label: &'static str,
    detail: &'static str,
    sources: &'static [&'static str],
) -> AccessPolicy {
    AccessPolicy {
        kind,
        status,
        label,
        detail,
        sources,
        verified_on,
    }
}

/// OpenAI's ChatGPT-plan route: supported while REX is released as open
/// source (`OPEN_SOURCE_RELEASE`), and described as open-source-only if the
/// switch is ever turned off.
fn chatgpt_plan_policy() -> AccessPolicy {
    policy_on(
        OPENAI_SIWC_VERIFIED_ON,
        AccessKind::SubscriptionOauth,
        if OPEN_SOURCE_RELEASE {
            PolicyStatus::Supported
        } else {
            PolicyStatus::OpenSourceOnly
        },
        "Continue with ChatGPT",
        if OPEN_SOURCE_RELEASE {
            "OpenAI documents Sign in with ChatGPT for open-source, locally hosted apps: the app registers itself at first sign-in (no client secret, no copied client ID), the user consents to spending their ChatGPT plan, and requests go to the public Responses API. REX is open source and runs locally, so it qualifies."
        } else {
            "OpenAI documents Sign in with ChatGPT for open-source, locally hosted apps: the app registers itself at first sign-in (no client secret, no copied client ID), the user consents to spending their ChatGPT plan, and requests go to the public Responses API. REX qualifies only while it is released as open source; the route is closed."
        },
        &[
            "https://developers.openai.com/siwc/quickstart",
            "https://developers.openai.com/siwc/token-sharing-open-source",
            "https://developers.openai.com/siwc/token-sharing-open-source/sign-in",
            "https://developers.openai.com/siwc/token-sharing-open-source/preview-limitations",
        ],
    )
}

/// The access routes evaluated for one registry provider. The frontend reads
/// these through `ProviderSummary::access`; anything not marked
/// `PolicyStatus::Supported` is not connectable in REX, only explained.
pub fn access_policies(provider: &str) -> Vec<AccessPolicy> {
    match provider {
        "gemini" => vec![
            api_key(
                "Gemini API key",
                "Google documents API keys from AI Studio as the programmatic path for your own applications, with a free tier.",
                &["https://ai.google.dev/gemini-api/docs/api-key", "https://ai.google.dev/gemini-api/docs/billing"],
            ),
            policy(
                AccessKind::SubscriptionOauth,
                PolicyStatus::NotOffered,
                "Google AI Pro / Ultra",
                "These consumer plans are documented for Google's own products only; REX found no official route that lets a third-party harness spend the subscription, so REX omits it.",
                &["https://ai.google.dev/gemini-api/docs/billing"],
            ),
        ],
        "anthropic" => vec![
            api_key(
                "Anthropic API key",
                "Anthropic directs developers building products or services to API key authentication through the Claude Console.",
                &["https://code.claude.com/docs/en/legal-and-compliance"],
            ),
            policy(
                AccessKind::SubscriptionOauth,
                PolicyStatus::NotPermitted,
                "Claude Pro / Max sign-in",
                "Anthropic states it does not permit third-party developers to offer Claude.ai login or to route requests through Free, Pro, or Max plan credentials, and reserves the right to enforce without notice.",
                &["https://code.claude.com/docs/en/legal-and-compliance"],
            ),
        ],
        "openai" => vec![
            api_key(
                "OpenAI API key",
                "OpenAI documents API keys for programmatic access, billed through the Platform account at standard API rates.",
                &["https://developers.openai.com/codex/auth"],
            ),
            chatgpt_plan_policy(),
        ],
        // ChatGPT plan usage: its own provider, because it speaks the
        // Responses API and its credential comes only from the sign-in.
        "openai-chatgpt" => vec![chatgpt_plan_policy()],
        "xai" => vec![
            api_key(
                "xAI API key",
                "xAI's API is compatible with the OpenAI and Anthropic SDKs and is billed per token through console.x.ai.",
                &["https://x.ai/api", "https://docs.x.ai/developers/pricing"],
            ),
            policy(
                AccessKind::SubscriptionOauth,
                PolicyStatus::PartnerGated,
                "X Premium / SuperGrok",
                "xAI officially enables subscription OAuth in named third-party harnesses, while publishing no general client-registration route. REX needs its own approved client and will not copy another app client ID.",
                &[
                    "https://x.ai/news/grok-hermes",
                    "https://x.ai/news/grok-openclaw",
                    "https://x.ai/news/grok-opencode",
                    "https://x.ai/news/grok-warp",
                    "https://github.com/earendil-works/pi/blob/36b60d2e8985899743c4cf5bd5f8929832a3f05d/packages/ai/src/auth/oauth/xai.ts",
                ],
            ),
        ],
        "deepseek" => vec![api_key(
            "DeepSeek API key",
            "DeepSeek's official docs describe top-up, balance-based API access for any client.",
            &["https://api-docs.deepseek.com/quick_start/pricing"],
        )],
        "kimi" => vec![api_key(
            "Moonshot API key",
            "Moonshot's platform documents API-key access to its OpenAI-compatible chat API.",
            &["https://platform.moonshot.ai/docs/api/chat"],
        )],
        "kimi-coding" => vec![policy(
            AccessKind::SubscriptionKey,
            PolicyStatus::Supported,
            "Kimi for Coding",
            "Kimi membership officially includes an API key for third-party development tools; REX connects to https://api.kimi.com/coding with that key. Available models depend on the plan tier, so the model ID is entered manually.",
            &["https://www.kimi.com/code/docs/en/", "https://www.kimi.com/code/docs/en/third-party-tools/claude-code.html"],
        )],
        "glm" => vec![
            api_key(
                "Zhipu GLM API key",
                "Z.AI's open platform sells pay-as-you-go GLM API access usable from any client.",
                &["https://docs.z.ai/guides/overview/pricing.md"],
            ),
            policy(
                AccessKind::SubscriptionKey,
                PolicyStatus::ToolScoped,
                "GLM Coding Plan",
                "Z.AI limits the plan to its officially supported tools (Claude Code, Cline, OpenCode, Kilo Code, and others on its list) and warns that benefits may be restricted on unsupported tools; REX is not on the list, so REX does not offer it.",
                &["https://docs.z.ai/devpack/overview"],
            ),
        ],
        "qwen" => vec![
            api_key(
                "Alibaba ModelStudio API key",
                "Qwen's own tooling documents the standard ModelStudio API key against DashScope's OpenAI-compatible endpoint.",
                &["https://qwenlm.github.io/qwen-code-docs/en/users/configuration/auth/"],
            ),
            policy(
                AccessKind::SubscriptionOauth,
                PolicyStatus::NotOffered,
                "Qwen OAuth free tier",
                "Qwen Code's docs state this free tier was discontinued on 2026-04-15; new requests are rejected.",
                &["https://qwenlm.github.io/qwen-code-docs/en/users/configuration/auth/"],
            ),
            policy(
                AccessKind::SubscriptionKey,
                PolicyStatus::ToolScoped,
                "Alibaba Cloud Coding Plan",
                "The plan is documented for Qwen Code with a dedicated endpoint and subscription key; Alibaba documents no route for arbitrary third-party harnesses, so REX omits it.",
                &["https://qwenlm.github.io/qwen-code-docs/en/users/configuration/auth/"],
            ),
        ],
        // Free tiers exist, but they are the account holder's own allowance for
        // development and evaluation. These entries say so and make no
        // verification claim: the provider's current terms are what govern.
        "nvidia" => vec![api_key(
            "NVIDIA NIM API key",
            "A key from your own build.nvidia.com account. The free tier is rate limited and meant for development and evaluation, not for serving other people; check NVIDIA's current terms before relying on it for anything else.",
            &[],
        )],
        "groq" => vec![api_key(
            "Groq API key",
            "A key from your own Groq account. The free tier has small per-minute token limits, which REX works around by waiting out rate limits; building an app on Groq is one thing, reselling access to it is another, so read Groq's current terms before sharing a key.",
            &[],
        )],
        "openrouter" => vec![api_key(
            "OpenRouter API key",
            "A key from your own OpenRouter account. Models whose id ends in :free have tight daily request limits; failed requests count against them.",
            &[],
        )],
        "local" => vec![api_key("Local server", "Your own server on your own machine; a key only if the server asks for one.", &[])],
        "custom" => vec![api_key(
            "Custom endpoint",
            "Any OpenAI-compatible endpoint you are authorized to use; authorization is the user's responsibility.",
            &[],
        )],
        _ => vec![],
    }
}
