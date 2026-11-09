use serde::Serialize;

/// Wire protocol spoken by a provider.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ProviderProtocol {
    Gemini,
    Anthropic,
    OpenAiCompatible,
    /// OpenAI's Responses API, as served to a ChatGPT-plan OAuth token:
    /// `POST /responses` with `store: false` and `stream: true`, the whole
    /// history replayed in `input`, and the reply read from the stream's
    /// terminal event.
    OpenAiResponses,
}

/// How the backend can learn the provider's model list.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ModelDiscovery {
    /// Provider-native listing endpoint.
    Native,
    /// OpenAI-compatible `GET /models`.
    OpenAiModels,
    /// `GET /models` as a ChatGPT-plan token sees it: a `models` array of
    /// `{slug, display_name, visibility}`, of which `visibility: "list"` are
    /// meant for display.
    ChatGptPlanModels,
    /// No listing API; the user must type a model ID.
    Manual,
}

/// Static description of a supported provider. Mirrors the frontend presets
/// in `src/data/providers.ts`; the backend is the source of truth for what
/// actually connects.
#[derive(Debug, Clone, Serialize)]
pub struct ProviderSpec {
    pub id: &'static str,
    pub name: &'static str,
    pub protocol: ProviderProtocol,
    pub default_base_url: Option<&'static str>,
    pub discovery: ModelDiscovery,
}

pub fn registry() -> Vec<ProviderSpec> {
    vec![
        ProviderSpec {
            id: "gemini",
            name: "Google Gemini",
            protocol: ProviderProtocol::Gemini,
            default_base_url: Some("https://generativelanguage.googleapis.com"),
            discovery: ModelDiscovery::Native,
        },
        ProviderSpec {
            id: "anthropic",
            name: "Anthropic",
            protocol: ProviderProtocol::Anthropic,
            default_base_url: Some("https://api.anthropic.com"),
            discovery: ModelDiscovery::Native,
        },
        ProviderSpec {
            id: "openai",
            name: "OpenAI",
            protocol: ProviderProtocol::OpenAiCompatible,
            default_base_url: Some("https://api.openai.com/v1"),
            discovery: ModelDiscovery::OpenAiModels,
        },
        // Your ChatGPT plan, through OpenAI's own "Sign in with ChatGPT" OAuth
        // for open-source clients. Credentials come only from that sign-in
        // (`chatgpt_oauth`), never from a pasted key, and the route is gated
        // by `policy::OPEN_SOURCE_RELEASE`.
        ProviderSpec {
            id: "openai-chatgpt",
            name: "OpenAI (ChatGPT plan)",
            protocol: ProviderProtocol::OpenAiResponses,
            default_base_url: Some("https://api.openai.com/v1"),
            discovery: ModelDiscovery::ChatGptPlanModels,
        },
        ProviderSpec {
            id: "xai",
            name: "xAI (Grok)",
            protocol: ProviderProtocol::OpenAiCompatible,
            default_base_url: Some("https://api.x.ai/v1"),
            discovery: ModelDiscovery::OpenAiModels,
        },
        ProviderSpec {
            id: "deepseek",
            name: "DeepSeek",
            protocol: ProviderProtocol::OpenAiCompatible,
            default_base_url: Some("https://api.deepseek.com"),
            discovery: ModelDiscovery::OpenAiModels,
        },
        ProviderSpec {
            id: "kimi",
            name: "Kimi",
            protocol: ProviderProtocol::OpenAiCompatible,
            default_base_url: Some("https://api.moonshot.ai/v1"),
            discovery: ModelDiscovery::OpenAiModels,
        },
        ProviderSpec {
            id: "kimi-coding",
            name: "Kimi for Coding",
            protocol: ProviderProtocol::Anthropic,
            default_base_url: Some("https://api.kimi.com/coding"),
            discovery: ModelDiscovery::Manual,
        },
        ProviderSpec {
            id: "qwen",
            name: "Qwen (Alibaba)",
            protocol: ProviderProtocol::OpenAiCompatible,
            default_base_url: Some("https://dashscope.aliyuncs.com/compatible-mode/v1"),
            discovery: ModelDiscovery::OpenAiModels,
        },
        ProviderSpec {
            id: "glm",
            name: "GLM",
            protocol: ProviderProtocol::OpenAiCompatible,
            default_base_url: Some("https://open.bigmodel.cn/api/paas/v4"),
            discovery: ModelDiscovery::OpenAiModels,
        },
        // Hosted OpenAI-compatible services with a free tier. They are ordinary
        // API-key providers; the free tier is the user's own account and is
        // meant for development and evaluation (see policy.rs).
        ProviderSpec {
            id: "nvidia",
            name: "NVIDIA NIM",
            protocol: ProviderProtocol::OpenAiCompatible,
            default_base_url: Some("https://integrate.api.nvidia.com/v1"),
            discovery: ModelDiscovery::OpenAiModels,
        },
        ProviderSpec {
            id: "groq",
            name: "Groq",
            protocol: ProviderProtocol::OpenAiCompatible,
            default_base_url: Some("https://api.groq.com/openai/v1"),
            discovery: ModelDiscovery::OpenAiModels,
        },
        ProviderSpec {
            id: "openrouter",
            name: "OpenRouter",
            protocol: ProviderProtocol::OpenAiCompatible,
            default_base_url: Some("https://openrouter.ai/api/v1"),
            discovery: ModelDiscovery::OpenAiModels,
        },
        ProviderSpec {
            id: "local",
            name: "Local server",
            protocol: ProviderProtocol::OpenAiCompatible,
            default_base_url: Some("http://127.0.0.1:11434/v1"),
            discovery: ModelDiscovery::OpenAiModels,
        },
        ProviderSpec {
            id: "custom",
            name: "Custom endpoint",
            protocol: ProviderProtocol::OpenAiCompatible,
            default_base_url: None,
            discovery: ModelDiscovery::Manual,
        },
    ]
}

pub fn find_spec(id: &str) -> Option<ProviderSpec> {
    registry().into_iter().find(|p| p.id == id)
}
