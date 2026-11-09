//! REX Harness provider core.
//!
//! First real backend slice: provider connectivity. Owns everything that
//! touches a provider API or a credential so the frontend never holds a key:
//! the frontend sees catalog data, status, and `has_key` booleans only.
//!
//! Security rules enforced here:
//! - API keys enter through `ProviderService::set_key`, land in a
//!   `SecretStore`, and are never returned, logged, or serialized.
//! - Error messages describing failures never include the key material.
//! - Every state the UI can render is truthful: unsupported, auth failure,
//!   network failure, rate limit, empty catalog, and stale cache are distinct.

pub mod agent_loop;
pub mod catalog;
mod chatgpt_oauth;
mod context;
pub mod conversation;
pub mod custody_link;
pub mod error;
mod history;
pub mod http;
mod memory;
mod output_store;
pub mod paths;
pub mod policy;
pub mod protect;
mod provider_failure;
pub mod providers;
mod responses;
pub mod run_history;
pub mod search;
pub mod secrets;
pub mod service;
pub mod workspace_notes;

pub mod autonomous;
pub mod live;
pub mod oneshot;

pub use agent_loop::{AgentLoop, LoopEvent, DEFAULT_MAX_STEPS, HARD_MAX_STEPS};
pub use autonomous::{
    windowed_token_ceiling, AgentEvent, AgentPreview, AgentSnapshot, AgentStatus,
    AutonomousRunService, Budgets, PlanItem, PlanStatus, RunOptions, TerminalReason,
};
pub use catalog::{CatalogSource, ModelCatalog, ModelInfo};
pub use chatgpt_oauth::{
    open_in_browser, SignIn as ChatGptSignIn, SignInSessions, SignInState, SignInTicket,
    SignOutReport, SignedIn, Status as ChatGptStatus, SIGN_IN_TIMEOUT as CHATGPT_SIGN_IN_TIMEOUT,
};
pub use context::ContextStats;
pub use conversation::{
    decode_turn, encode_tool_outcomes, NormalizedToolCall, NormalizedTurn, ToolOutcome,
};
pub use custody_link::{
    ui_managed_request, CustodiedRun, CustodiedRunView, CustodyRunService, ManagedTaskRequest,
};
pub use error::ProviderError;
pub use http::{Transport, UreqTransport};
pub use live::{LiveRunService, RunSnapshot, RunStatus};
pub use policy::{
    access_policies, AccessKind, AccessPolicy, PolicyStatus, OPENAI_SIWC_VERIFIED_ON,
    OPEN_SOURCE_RELEASE, VERIFIED_ON,
};
pub use protect::{platform_protector, Protector};
pub use providers::{find_spec, registry, ModelDiscovery, ProviderProtocol, ProviderSpec};
pub use responses::USAGE_SETTINGS_URL as CHATGPT_USAGE_URL;
pub use search::{SearchProvider, SearchProviderSummary, SearchRouter};
pub use secrets::{FileSecretStore, MemorySecretStore, SecretStore};
pub use service::{ProviderService, ProviderSummary};
