export type ProviderProtocol = "gemini" | "anthropic" | "openai-compatible" | "openai-responses";
export type ProviderStatus = "not-configured" | "draft" | "needs-runtime" | "unsupported";
export type AccessStatus =
  | "supported"
  | "tool-scoped"
  | "not-permitted"
  | "not-offered"
  | "undocumented"
  | "partner-gated"
  | "open-source-only";

// One evaluated way to connect a provider account. Mirrors the backend's
// access policy (crates/rex-providers/src/policy.rs), which is the source of
// truth; verdicts are grounded in official provider documentation and
// re-verified on 2026-09-19. See docs/subscription-policy.md.
export type AccessLine = {
  kind: "api-key" | "subscription-key" | "subscription-oauth";
  status: AccessStatus;
  label: string;
  detail: string;
};

export type ProviderPreset = {
  id: string;
  name: string;
  protocol: ProviderProtocol;
  summary: string;
  baseUrl?: string;
  baseUrlLocked?: boolean;
  modelDiscovery: "native" | "openai-models" | "chatgpt-plan-models" | "manual";
  examples?: string;
  /** A practical, honest note shown in the Connect dialog (limits, quirks). */
  note?: string;
  /** Turn the lean prompt on automatically: this provider's free tier has
   *  small per-minute or per-request token budgets. The person can switch it
   *  off in Run options. */
  leanByDefault?: boolean;
  access: AccessLine[];
};

export type ProviderDraft = {
  presetId: string;
  displayName: string;
  protocol: ProviderProtocol;
  baseUrl: string;
  modelId: string;
  keyReference: "desktop-keychain";
  status: ProviderStatus;
};

export const PROVIDER_PRESETS: ProviderPreset[] = [
  {
    id: "gemini",
    name: "Google Gemini",
    protocol: "gemini",
    summary: "Native Gemini generateContent and model listing.",
    modelDiscovery: "native",
    access: [
      {
        kind: "api-key",
        status: "supported",
        label: "Gemini API key",
        detail: "Google documents API keys from AI Studio as the programmatic path for your own applications, with a free tier.",
      },
      {
        kind: "subscription-oauth",
        status: "not-offered",
        label: "Google AI Pro / Ultra",
        detail: "These consumer plans are documented for Google's own products only; no official route lets a third-party harness spend the subscription.",
      },
    ],
  },
  {
    id: "anthropic",
    name: "Anthropic",
    protocol: "anthropic",
    summary: "Native Claude Messages API and model listing.",
    modelDiscovery: "native",
    access: [
      {
        kind: "api-key",
        status: "supported",
        label: "Anthropic API key",
        detail: "Anthropic directs developers building products or services to API key authentication through the Claude Console.",
      },
      {
        kind: "subscription-oauth",
        status: "not-permitted",
        label: "Claude Pro / Max sign-in",
        detail: "Anthropic does not permit third-party developers to offer Claude.ai login or to route requests through Free, Pro, or Max plan credentials. Never connectable here.",
      },
    ],
  },
  {
    id: "openai",
    name: "OpenAI",
    protocol: "openai-compatible",
    summary: "Preset for OpenAI's platform API at standard API rates.",
    baseUrl: "https://api.openai.com/v1",
    modelDiscovery: "openai-models",
    access: [
      {
        kind: "api-key",
        status: "supported",
        label: "OpenAI API key",
        detail: "OpenAI documents API keys for programmatic access, billed through the Platform account at standard API rates.",
      },
    ],
  },
  {
    // Your ChatGPT plan, through OpenAI's Sign in with ChatGPT for open-source,
    // locally hosted apps. The credential is only ever the one the sign-in
    // issued (never pasted), and the route stays closed until REX is released
    // as open source. The backend decides; `accessStatusFor` reads its verdict
    // and this static line is only the fallback.
    id: "openai-chatgpt",
    name: "OpenAI (ChatGPT plan)",
    protocol: "openai-responses",
    summary: "Use the ChatGPT plan you already pay for, through OpenAI's Sign in with ChatGPT, instead of an API key.",
    baseUrl: "https://api.openai.com/v1",
    modelDiscovery: "chatgpt-plan-models",
    note: "Eligible requests use your ChatGPT plan's usage or credits, not API billing. You can review and limit that usage in ChatGPT settings. REX never sees your ChatGPT password and never reads your conversations.",
    access: [
      {
        kind: "subscription-oauth",
        status: "open-source-only",
        label: "Continue with ChatGPT",
        detail: "OpenAI offers this to open-source, locally hosted apps: REX registers itself at your first sign-in and you approve spending your plan. It opens when REX is released as open source.",
      },
    ],
  },
  {
    id: "xai",
    name: "xAI (Grok)",
    protocol: "openai-compatible",
    summary: "Preset for xAI's Grok models through console.x.ai.",
    baseUrl: "https://api.x.ai/v1",
    modelDiscovery: "openai-models",
    access: [
      {
        kind: "api-key",
        status: "supported",
        label: "xAI API key",
        detail: "xAI's API is compatible with the OpenAI and Anthropic SDKs and is billed per token through console.x.ai.",
      },
      {
        kind: "subscription-oauth",
        status: "not-offered",
        label: "X Premium / SuperGrok",
        detail: "xAI keeps consumer Grok subscriptions and API access strictly separate; the subscriptions carry no programmatic entitlement.",
      },
    ],
  },
  {
    id: "deepseek",
    name: "DeepSeek",
    protocol: "openai-compatible",
    summary: "Preset for DeepSeek's OpenAI-compatible API.",
    baseUrl: "https://api.deepseek.com",
    modelDiscovery: "openai-models",
    access: [
      {
        kind: "api-key",
        status: "supported",
        label: "DeepSeek API key",
        detail: "DeepSeek's official docs describe top-up, balance-based API access for any client.",
      },
    ],
  },
  {
    id: "kimi",
    name: "Kimi",
    protocol: "openai-compatible",
    summary: "Preset for Moonshot AI's OpenAI-compatible API.",
    baseUrl: "https://api.moonshot.ai/v1",
    modelDiscovery: "openai-models",
    access: [
      {
        kind: "api-key",
        status: "supported",
        label: "Moonshot API key",
        detail: "Moonshot's platform documents API-key access to its OpenAI-compatible chat API.",
      },
    ],
  },
  {
    id: "kimi-coding",
    name: "Kimi for Coding",
    protocol: "anthropic",
    summary:
      "Kimi membership subscription, officially connectable from third-party tools with a Kimi Code API key. The endpoint speaks the Anthropic Messages shape.",
    baseUrl: "https://api.kimi.com/coding",
    modelDiscovery: "manual",
    examples: "kimi-for-coding, k3-256k, k3, kimi-for-coding-highspeed - which models you can use depends on your membership tier",
    access: [
      {
        kind: "subscription-key",
        status: "supported",
        label: "Kimi for Coding",
        detail:
          "Kimi membership officially includes an API key for third-party development tools; REX connects to api.kimi.com/coding with that key. This is the one consumer subscription REX offers.",
      },
    ],
  },
  {
    id: "qwen",
    name: "Qwen (Alibaba)",
    protocol: "openai-compatible",
    summary: "Preset for Alibaba ModelStudio's DashScope compatible-mode endpoint.",
    baseUrl: "https://dashscope.aliyuncs.com/compatible-mode/v1",
    modelDiscovery: "openai-models",
    access: [
      {
        kind: "api-key",
        status: "supported",
        label: "Alibaba ModelStudio API key",
        detail: "Qwen's own tooling documents the standard ModelStudio API key against DashScope's OpenAI-compatible endpoint.",
      },
      {
        kind: "subscription-oauth",
        status: "not-offered",
        label: "Qwen OAuth free tier",
        detail: "Discontinued on 2026-04-15 per Qwen Code's docs; new requests are rejected.",
      },
      {
        kind: "subscription-key",
        status: "tool-scoped",
        label: "Alibaba Cloud Coding Plan",
        detail: "Documented for Qwen Code only; no documented route for arbitrary third-party harnesses.",
      },
    ],
  },
  {
    id: "glm",
    name: "GLM",
    protocol: "openai-compatible",
    summary: "Preset for Zhipu AI's OpenAI-compatible API.",
    baseUrl: "https://open.bigmodel.cn/api/paas/v4",
    modelDiscovery: "openai-models",
    access: [
      {
        kind: "api-key",
        status: "supported",
        label: "Zhipu GLM API key",
        detail: "Z.AI's open platform sells pay-as-you-go GLM API access usable from any client.",
      },
      {
        kind: "subscription-key",
        status: "tool-scoped",
        label: "GLM Coding Plan",
        detail:
          "Z.AI limits the plan to its officially supported tools (Claude Code, Cline, OpenCode, Kilo Code, others on its list) and warns of restricted benefits on unsupported tools. REX is not listed.",
      },
    ],
  },
  {
    id: "nvidia",
    name: "NVIDIA NIM",
    protocol: "openai-compatible",
    summary: "Hosted open models on NVIDIA's NIM endpoints; a free developer key.",
    baseUrl: "https://integrate.api.nvidia.com/v1",
    modelDiscovery: "openai-models",
    note: "The free tier is for development and evaluation and is rate limited (community reports say about 40 requests a minute). Many listed models are slow or not callable for a free account; pick one that answers in a few seconds.",
    leanByDefault: true,
    access: [
      {
        kind: "api-key",
        status: "supported",
        label: "NVIDIA NIM API key",
        detail: "A key from your own build.nvidia.com account. The free tier is meant for development and evaluation, not for serving other people.",
      },
    ],
  },
  {
    id: "groq",
    name: "Groq",
    protocol: "openai-compatible",
    summary: "Very fast hosted open models; the free tier needs no card.",
    baseUrl: "https://api.groq.com/openai/v1",
    modelDiscovery: "openai-models",
    note: "The free tier caps tokens per minute (8,000 on the models we measured). REX waits out the limit and its lean prompt keeps each request small, but long tasks will be slow.",
    leanByDefault: true,
    access: [
      {
        kind: "api-key",
        status: "supported",
        label: "Groq API key",
        detail: "A key from your own Groq account. Read Groq's current terms before sharing a key with anyone else.",
      },
    ],
  },
  {
    id: "openrouter",
    name: "OpenRouter",
    protocol: "openai-compatible",
    summary: "One key for many hosted models, including some free ones.",
    baseUrl: "https://openrouter.ai/api/v1",
    modelDiscovery: "openai-models",
    note: "Models whose id ends in :free have small daily request limits (about 50 a day without credits, per OpenRouter) and failed requests count against them.",
    leanByDefault: true,
    access: [
      {
        kind: "api-key",
        status: "supported",
        label: "OpenRouter API key",
        detail: "A key from your own OpenRouter account.",
      },
    ],
  },
  {
    id: "local",
    name: "Local server",
    protocol: "openai-compatible",
    summary: "Ollama, LM Studio, vLLM, or another local compatible server.",
    baseUrl: "http://127.0.0.1:11434/v1",
    modelDiscovery: "openai-models",
    note: "Ollama and LM Studio ignore the key, but REX needs one saved: type any placeholder such as ollama.",
    access: [
      {
        kind: "api-key",
        status: "supported",
        label: "Local server",
        detail: "Your own server on your own machine; a key only if the server asks for one.",
      },
    ],
  },
  {
    id: "custom",
    name: "Custom endpoint",
    protocol: "openai-compatible",
    summary: "Any endpoint that implements the calls REX needs.",
    baseUrl: "",
    modelDiscovery: "manual",
    examples: "OpenRouter, Azure-style gateways, proxies, and self-hosted runtimes",
    access: [
      {
        kind: "api-key",
        status: "supported",
        label: "Custom endpoint",
        detail: "Any OpenAI-compatible endpoint you are authorized to use; authorization is the user's responsibility.",
      },
    ],
  },
];

/** Whether the lean prompt starts on for this provider (see leanByDefault). */
export const defaultLean = (providerId: string): boolean =>
  PROVIDER_PRESETS.find((p) => p.id === providerId)?.leanByDefault === true;

// Wire-shape labels, not vendor claims: Kimi for Coding legitimately speaks
// the Anthropic shape without being Anthropic.
export const protocolLabel: Record<ProviderProtocol, string> = {
  gemini: "Gemini API",
  anthropic: "Anthropic API shape",
  "openai-compatible": "OpenAI-compatible",
  "openai-responses": "OpenAI Responses (ChatGPT plan)",
};

export const accessStatusLabel: Record<AccessStatus, string> = {
  supported: "Offered",
  "tool-scoped": "Tool-scoped",
  "not-permitted": "Forbidden",
  "not-offered": "Not offered",
  undocumented: "Not documented",
  "partner-gated": "Partner approval",
  "open-source-only": "After open-source release",
};

/** The verdict for one access line: the backend's, when it is running and
 *  knows the line, and the bundled one otherwise. The backend is the source
 *  of truth, so a route that opens (the ChatGPT plan at the open-source
 *  release) opens here without a frontend release. */
export function accessStatusFor(
  summaries: { id: string; access?: { label: string; status: AccessStatus }[] }[],
  providerId: string,
  line: AccessLine
): AccessStatus {
  const live = summaries.find((s) => s.id === providerId)?.access?.find((a) => a.label === line.label);
  return live?.status ?? line.status;
}
