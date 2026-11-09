// Bridge to the real Rust backend. Two transports:
// - "tauri": inside the desktop app, through Tauri commands.
// - "sidecar": during vite dev, through the rex-dev-server localhost binary.
// When neither is reachable the app stays a labeled frontend preview.
// No credential is ever stored in frontend state beyond the entry field
// that sends it; the backend returns has_key booleans, catalogs, and
// truthful errors only.

export type BackendKind = "tauri" | "sidecar";

export type ProviderErrorShape =
  | { kind: "not_configured" }
  | { kind: "auth_failed" }
  | { kind: "rate_limited" }
  | { kind: "network"; detail: string }
  | { kind: "unsupported"; detail: string }
  | { kind: "empty_catalog" }
  | { kind: "invalid_response"; detail: string }
  | { kind: "store"; detail: string }
  // A credential that cannot be used, with the words to act on (sign in
  // again, enable plan usage, the route is not open yet). Never a secret.
  | { kind: "credential"; detail: string };

export interface ModelInfo {
  id: string;
  label: string;
  provider: string;
}

export interface ModelCatalog {
  provider: string;
  fetched_at: number;
  source: "live" | "replay";
  models: ModelInfo[];
}

// Access policy verdict mirrored from the Rust backend (policy.rs).
export interface AccessPolicy {
  kind: "api-key" | "subscription-key" | "subscription-oauth";
  status: "supported" | "tool-scoped" | "not-permitted" | "not-offered" | "undocumented" | "partner-gated" | "open-source-only";
  label: string;
  detail: string;
  sources: string[];
  verified_on: string;
}

export interface ProviderSummary {
  id: string;
  name: string;
  protocol: "gemini" | "anthropic" | "openai-compatible" | "open-ai-compatible" | "open-ai-responses";
  discovery: "native" | "openai-models" | "open-ai-models" | "chat-gpt-plan-models" | "manual";
  access?: AccessPolicy[];
  base_url: string | null;
  /** A stored key, or for the ChatGPT plan a sign-in that allows plan usage. */
  has_key: boolean;
  /** The ChatGPT sign-in (no credential). Present only while that route is open. */
  signin?: ChatGptStatus;
  catalog: ModelCatalog | null;
  last_error?: ProviderErrorShape;
}

// ---- Continue with ChatGPT (use your ChatGPT plan) --------------------------
// The backend runs the whole OAuth flow; these only start it, ask how it went,
// and sign out. Nothing here ever holds a token or the real authorization URL.

export interface ChatGptStatus {
  signed_in: boolean;
  email: string | null;
  plan_usage: boolean;
  access_token_expires_at: number | null;
}

export interface SignInTicket {
  attempt: string;
  /** One-shot loopback link that redirects to OpenAI. Names no account. */
  launch_url: string;
  /** First sign-in on this machine: OpenAI asks the user to approve REX. */
  registering: boolean;
  timeout_secs: number;
  /** The backend already opened the system browser. */
  opened: boolean;
}

export type SignInState =
  | { state: "waiting" }
  | { state: "done"; email: string | null; plan_usage: boolean }
  | { state: "failed"; error: ProviderErrorShape }
  | { state: "cancelled" };

export interface SignOutReport {
  revoked: boolean;
  note: string | null;
}

export type SignInStart = { ok: true; ticket: SignInTicket } | { ok: false; error: ProviderErrorShape };

export const CHATGPT_PROVIDER = "openai-chatgpt";
/** Where users review and limit what REX may spend of their ChatGPT plan. */
export const CHATGPT_USAGE_URL = "https://chatgpt.com/settings/usage";

export type RefreshResult = { ok: true; catalog: ModelCatalog } | { ok: false; error: ProviderErrorShape };
export type WriteResult = { ok: true } | { ok: false; error: ProviderErrorShape };

export function describeError(error: ProviderErrorShape): string {
  switch (error.kind) {
    case "not_configured":
      return "No API key stored yet";
    case "auth_failed":
      return "Provider rejected the API key";
    case "rate_limited":
      return "Rate limited - try again shortly";
    case "network":
      return "Network error - check the connection";
    case "unsupported":
      return "Model listing unsupported here";
    case "empty_catalog":
      return "Provider returned no usable models";
    case "invalid_response":
      return "Unexpected provider response";
    case "store":
      return "Credential store error";
    case "credential":
      return error.detail;
  }
}

const SIDECAR = "http://127.0.0.1:8787";

type InvokeFn = <T>(cmd: string, args?: Record<string, unknown>) => Promise<T>;

let cached: { kind: BackendKind; invoke: InvokeFn | null } | null | undefined;

async function probe(): Promise<{ kind: BackendKind; invoke: InvokeFn | null } | null> {
  const w = window as unknown as { __TAURI_INTERNALS__?: unknown };
  if (w.__TAURI_INTERNALS__) {
    try {
      const mod = await import("@tauri-apps/api/core");
      return { kind: "tauri", invoke: mod.invoke as InvokeFn };
    } catch {
      return null;
    }
  }
  const host = window.location.hostname;
  const localHost = host === "localhost" || host === "127.0.0.1" || host === "[::1]";
  if (import.meta.env.DEV || localHost) {
    try {
      const controller = new AbortController();
      const timer = window.setTimeout(() => controller.abort(), 900);
      const response = await fetch(`${SIDECAR}/api/status`, { signal: controller.signal });
      window.clearTimeout(timer);
      if (response.ok) {
        const info = await response.json();
        if (info?.kind === "sidecar") return { kind: "sidecar", invoke: null };
      }
    } catch {
      /* no sidecar running */
    }
  }
  return null;
}

export async function backendKind(): Promise<BackendKind | null> {
  if (cached === undefined) cached = await probe();
  return cached ? cached.kind : null;
}

async function backend() {
  if (cached === undefined) cached = await probe();
  if (!cached) throw new Error("no backend");
  return cached;
}

async function request<T>(path: string, init?: RequestInit): Promise<T> {
  const response = await fetch(`${sidecarBase()}${path}`, {
    ...init,
    headers: { "content-type": "application/json", ...(init?.headers ?? {}) },
  });
  if (!response.ok) throw new Error(`sidecar HTTP ${response.status}`);
  return (await response.json()) as T;
}
function sidecarBase() {
  return SIDECAR;
}

export type InstalledAgentId = "codex";
export type InstalledAgentState = "ready" | "installed_auth_unknown" | "missing" | "unsupported_version" | "support_needs_review";
export interface InstalledAgentSummary {
  id: InstalledAgentId; name: string; executable: string; state: InstalledAgentState; version?: string; automation: string; auth_boundary: string; docs_url: string; approval_boundary: string;
  compatibility: { verified_on: string; documented_interface: string; entitlement_note: string; fail_closed: boolean };
}
export async function listInstalledAgents(): Promise<InstalledAgentSummary[]> {
  const b = await backend();
  if (b.kind !== "tauri") throw new Error("installed agents require the desktop runtime");
  return b.invoke!<InstalledAgentSummary[]>("installed_agent_summaries");
}

export async function listSummaries(): Promise<ProviderSummary[]> {
  const b = await backend();
  if (b.kind === "tauri") return b.invoke!<ProviderSummary[]>("provider_summaries");
  return request<ProviderSummary[]>("/api/providers");
}

export async function setProviderKey(provider: string, key: string, baseUrl?: string): Promise<WriteResult> {
  const b = await backend();
  if (b.kind === "tauri") {
    try {
      await b.invoke!("provider_set_key", { provider, key, baseUrl: baseUrl ?? null });
      return { ok: true };
    } catch (error) {
      return { ok: false, error: error as ProviderErrorShape };
    }
  }
  return request<WriteResult>(`/api/providers/${provider}/key`, {
    method: "POST",
    body: JSON.stringify({ key, base_url: baseUrl ?? null }),
  });
}

export async function clearProviderKey(provider: string): Promise<WriteResult> {
  const b = await backend();
  if (b.kind === "tauri") {
    try {
      await b.invoke!("provider_clear_key", { provider });
      return { ok: true };
    } catch (error) {
      return { ok: false, error: error as ProviderErrorShape };
    }
  }
  return request<WriteResult>(`/api/providers/${provider}/key`, { method: "DELETE" });
}

export async function refreshCatalog(provider: string): Promise<RefreshResult> {
  const b = await backend();
  if (b.kind === "tauri") {
    try {
      const catalog = await b.invoke!<ModelCatalog>("provider_refresh", { provider });
      return { ok: true, catalog };
    } catch (error) {
      return { ok: false, error: error as ProviderErrorShape };
    }
  }
  return request<RefreshResult>(`/api/providers/${provider}/refresh`, { method: "POST", body: "{}" });
}

export type SearchProviderId = "rex" | "exa" | "tinyfish";
export interface SearchProviderSummary { id: SearchProviderId; name: string; active: boolean; built_in: boolean; has_key: boolean; endpoint: string; docs_url: string; }
export async function listSearchProviders(): Promise<SearchProviderSummary[]> {
  const b=await backend();
  if (b.kind === "tauri") return b.invoke!<SearchProviderSummary[]>("search_provider_summaries");
  throw new Error("search provider routing requires the desktop runtime");
}
export async function setSearchProviderKey(provider: SearchProviderId, key: string): Promise<WriteResult> {
  const b=await backend(); if(b.kind!=="tauri") throw new Error("desktop runtime required");
  try { await b.invoke!("search_provider_set_key",{provider,key}); return {ok:true}; } catch(error){ return {ok:false,error:error as ProviderErrorShape}; }
}
export async function clearSearchProviderKey(provider: SearchProviderId): Promise<WriteResult> {
  const b=await backend(); if(b.kind!=="tauri") throw new Error("desktop runtime required");
  try { await b.invoke!("search_provider_clear_key",{provider}); return {ok:true}; } catch(error){ return {ok:false,error:error as ProviderErrorShape}; }
}
export async function selectSearchProvider(provider: SearchProviderId): Promise<WriteResult> {
  const b=await backend(); if(b.kind!=="tauri") throw new Error("desktop runtime required");
  try { await b.invoke!("search_provider_select",{provider}); return {ok:true}; } catch(error){ return {ok:false,error:error as ProviderErrorShape}; }
}

export type RiskClass = "read" | "write" | "execute" | "denied";
export interface PreparedToolCall { call_id:string; tool:string; summary:string; risk:RiskClass; approval_required:boolean; policy_reason:string; standing_key?:string; }
export interface ToolReceipt { started_at_ms:number; duration_ms:number; target:string|null; command:string[]|null; exit_code:number|null; bytes_read:number; bytes_written:number; output_truncated:boolean; diff:string|null; redactions:number; }
export interface ToolResult { call_id:string; ok:boolean; tool:string; state:"pending_approval"|"ready"|"denied"|"cancelled"|"executed"; output:string|null; error:{kind:string;detail:string}|null; receipt:ToolReceipt; }
export async function resolveToolApproval(callId:string,approved:boolean):Promise<ToolResult>{const b=await backend();if(b.kind!=="tauri")throw new Error("desktop runtime required");await b.invoke!("tool_resolve_approval",{callId,approved});return b.invoke!<ToolResult>("tool_execute",{callId});}
export async function cancelToolCall(callId:string):Promise<void>{const b=await backend();if(b.kind!=="tauri")return;await b.invoke!("tool_cancel",{callId});}

export async function startChatGptSignIn(forceConsent = false): Promise<SignInStart> {
  const b = await backend();
  if (b.kind === "tauri") {
    try {
      const ticket = await b.invoke!<SignInTicket>("chatgpt_sign_in_start", { forceConsent });
      return { ok: true, ticket };
    } catch (error) {
      return { ok: false, error: error as ProviderErrorShape };
    }
  }
  return request<SignInStart>(`/api/providers/${CHATGPT_PROVIDER}/signin`, {
    method: "POST",
    body: JSON.stringify({ force_consent: forceConsent }),
  });
}

export async function pollChatGptSignIn(attempt: string): Promise<SignInState | null> {
  const b = await backend();
  if (b.kind === "tauri") return b.invoke!<SignInState | null>("chatgpt_sign_in_poll", { attempt });
  try {
    return await request<SignInState>(`/api/providers/${CHATGPT_PROVIDER}/signin/${encodeURIComponent(attempt)}`);
  } catch {
    return null;
  }
}

export async function cancelChatGptSignIn(attempt: string): Promise<void> {
  const b = await backend();
  if (b.kind === "tauri") {
    await b.invoke!("chatgpt_sign_in_cancel", { attempt });
    return;
  }
  await request(`/api/providers/${CHATGPT_PROVIDER}/signin/${encodeURIComponent(attempt)}`, { method: "DELETE" });
}

export async function signOutChatGpt(): Promise<{ ok: true; report: SignOutReport } | { ok: false; error: ProviderErrorShape }> {
  const b = await backend();
  if (b.kind === "tauri") {
    try {
      return { ok: true, report: await b.invoke!<SignOutReport>("chatgpt_sign_out") };
    } catch (error) {
      return { ok: false, error: error as ProviderErrorShape };
    }
  }
  return request(`/api/providers/${CHATGPT_PROVIDER}/signout`, { method: "POST", body: "{}" });
}
