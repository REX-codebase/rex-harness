use crate::catalog::{CatalogSource, ModelCatalog, ModelInfo};
use crate::chatgpt_oauth::{self, OAuthError, SignIn, SignOutReport, SignedIn};
use crate::error::ProviderError;
use crate::http::Transport;
use crate::policy::{access_policies, chatgpt_plan_route, AccessPolicy};
use crate::providers::{find_spec, registry, ModelDiscovery, ProviderProtocol, ProviderSpec};
use crate::secrets::SecretStore;
use serde::Serialize;
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Per-provider status as the UI is allowed to see it: identity, discovery
/// capability, whether a key exists, the last good catalog, and the last
/// error. No key material, ever.
#[derive(Debug, Clone, Serialize)]
pub struct ProviderSummary {
    pub id: String,
    pub name: String,
    pub protocol: ProviderProtocol,
    pub discovery: ModelDiscovery,
    pub access: Vec<AccessPolicy>,
    pub base_url: Option<String>,
    /// Whether a usable credential exists: a stored key, or for the ChatGPT
    /// plan route a sign-in that allows plan usage.
    pub has_key: bool,
    /// The ChatGPT sign-in, for the provider that has one. Never a credential.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signin: Option<chatgpt_oauth::Status>,
    pub catalog: Option<ModelCatalog>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_error: Option<ProviderError>,
}

/// How a sign-in failure reaches the rest of the backend: transient trouble
/// stays a network error (the credentials are intact and a retry may work),
/// storage failures stay storage failures, and everything else is a
/// credential problem whose message says what to do.
fn credential_error(e: OAuthError) -> ProviderError {
    match e {
        OAuthError::Transient(m) => ProviderError::Network(m),
        OAuthError::Store(m) => ProviderError::Store(m),
        other => ProviderError::Credential(other.to_string()),
    }
}

fn is_chatgpt_plan(provider: &str) -> bool {
    provider == chatgpt_oauth::PROVIDER_ID
}

/// The base-URL override is kept beside the key, under a name no provider id
/// can collide with, so every service instance that shares this store (the
/// UI's service and the agent's own copy) resolves the same endpoint. It used
/// to live only in the memory of the instance that handled the Connect
/// dialog, which made custom endpoints and non-default local URLs invisible
/// to runs: the agent failed with "no base URL configured".
fn base_url_slot(provider: &str) -> String {
    format!("{provider}#base_url")
}

struct ProviderState {
    base_url: Option<String>,
    catalog: Option<ModelCatalog>,
    last_error: Option<ProviderError>,
}

/// The backend the frontend talks to. Owns credentials, provider HTTP, and
/// catalog state behind one mutex-protected map.
pub struct ProviderService<S: SecretStore, T: Transport> {
    secrets: S,
    transport: T,
    states: Mutex<HashMap<String, ProviderState>>,
}

impl<S: SecretStore, T: Transport> ProviderService<S, T> {
    pub fn new(secrets: S, transport: T) -> Self {
        Self {
            secrets,
            transport,
            states: Mutex::new(HashMap::new()),
        }
    }

    fn state_for(&self, provider: &str) -> ProviderState {
        self.states
            .lock()
            .map(|map| {
                map.get(provider).map(|s| ProviderState {
                    base_url: s.base_url.clone(),
                    catalog: s.catalog.clone(),
                    last_error: s.last_error.clone(),
                })
            })
            .ok()
            .flatten()
            .unwrap_or(ProviderState {
                base_url: None,
                catalog: None,
                last_error: None,
            })
    }

    fn mutate(&self, provider: &str, f: impl FnOnce(&mut ProviderState)) {
        if let Ok(mut map) = self.states.lock() {
            f(map.entry(provider.to_string()).or_insert(ProviderState {
                base_url: None,
                catalog: None,
                last_error: None,
            }));
        }
    }

    pub fn summaries(&self) -> Vec<ProviderSummary> {
        registry()
            .into_iter()
            .map(|spec| {
                let state = self.state_for(spec.id);
                let signin = if is_chatgpt_plan(spec.id) && chatgpt_plan_route().is_ok() {
                    chatgpt_oauth::status(&self.secrets).ok()
                } else {
                    None
                };
                ProviderSummary {
                    id: spec.id.to_string(),
                    name: spec.name.to_string(),
                    protocol: spec.protocol,
                    discovery: spec.discovery,
                    access: access_policies(spec.id),
                    base_url: self.resolve_base_url(spec.id, state.base_url, spec.default_base_url),
                    has_key: match &signin {
                        Some(status) => status.signed_in && status.plan_usage,
                        None => !is_chatgpt_plan(spec.id) && self.secrets.has_key(spec.id),
                    },
                    signin,
                    catalog: state.catalog,
                    last_error: state.last_error,
                }
            })
            .collect()
    }

    /// Store a key (and optional base URL override). The key goes straight to
    /// the secret store; nothing is logged or returned.
    pub fn set_key(
        &self,
        provider: &str,
        key: &str,
        base_url: Option<&str>,
    ) -> Result<(), ProviderError> {
        let spec =
            find_spec(provider).ok_or_else(|| ProviderError::Unsupported(provider.to_string()))?;
        if is_chatgpt_plan(provider) {
            // The credential for this route is only ever the one the sign-in
            // issued; a pasted string would be a token REX did not obtain.
            return Err(ProviderError::Unsupported(
                "this provider takes no pasted key; use Continue with ChatGPT".into(),
            ));
        }
        if key.trim().is_empty() {
            return Err(ProviderError::NotConfigured);
        }
        if spec.protocol == ProviderProtocol::OpenAiCompatible
            || spec.protocol == ProviderProtocol::Gemini
            || spec.protocol == ProviderProtocol::Anthropic
        {
            if let Some(url) = base_url {
                let trimmed = url.trim().trim_end_matches('/').to_string();
                if trimmed.is_empty() {
                    self.secrets.clear_key(&base_url_slot(provider))?;
                } else {
                    self.secrets.set_key(&base_url_slot(provider), &trimmed)?;
                }
                self.mutate(provider, |s| {
                    s.base_url = if trimmed.is_empty() {
                        None
                    } else {
                        Some(trimmed)
                    };
                });
            }
        }
        self.secrets.set_key(provider, key.trim())?;
        self.mutate(provider, |s| s.last_error = None);
        Ok(())
    }

    pub fn clear_key(&self, provider: &str) -> Result<(), ProviderError> {
        if is_chatgpt_plan(provider) {
            // Disconnecting is signing out: revoke at OpenAI, then clear.
            self.chatgpt_sign_out()?;
            self.mutate(provider, |s| {
                s.catalog = None;
                s.last_error = None;
            });
            return Ok(());
        }
        self.secrets.clear_key(provider)?;
        // Disconnecting also forgets the custom endpoint; leaving it behind
        // would point the next key at a host the user no longer chose.
        self.secrets.clear_key(&base_url_slot(provider))?;
        self.mutate(provider, |s| {
            s.base_url = None;
            s.catalog = None;
            s.last_error = None;
        });
        Ok(())
    }

    /// The shared store is the source of truth for the override; this
    /// instance's own memory is only a fallback, then the provider default.
    fn resolve_base_url(
        &self,
        provider: &str,
        in_memory: Option<String>,
        default: Option<&'static str>,
    ) -> Option<String> {
        self.secrets
            .get_key(&base_url_slot(provider))
            .ok()
            .flatten()
            .filter(|u| !u.trim().is_empty())
            .or(in_memory)
            .or_else(|| default.map(str::to_string))
    }

    /// Read a stored key for a provider call inside the backend. The value
    /// is never serialized, logged, or returned across the frontend bridge.
    ///
    /// For the ChatGPT plan route this is the signed-in account's access
    /// token, renewed when it is close to expiry, and only while the route is
    /// open (`policy::OPEN_SOURCE_RELEASE`). It lives about an hour, so a long
    /// run asks again at every turn instead of holding one.
    pub fn get_key(&self, provider: &str) -> Result<Option<String>, ProviderError> {
        if is_chatgpt_plan(provider) {
            chatgpt_plan_route().map_err(ProviderError::Credential)?;
            return chatgpt_oauth::access_token(&self.transport, &self.secrets)
                .map(Some)
                .map_err(credential_error);
        }
        self.secrets.get_key(provider)
    }

    /// Start a ChatGPT sign-in: bind the loopback listener and build the
    /// authorize URL. Refused until the route is open.
    pub fn chatgpt_begin_sign_in(&self, force_consent: bool) -> Result<SignIn, ProviderError> {
        chatgpt_plan_route().map_err(ProviderError::Credential)?;
        chatgpt_oauth::begin(&self.secrets, force_consent).map_err(credential_error)
    }

    /// Wait for the browser to come back and finish the sign-in.
    pub fn chatgpt_complete(
        &self,
        signin: &SignIn,
        timeout: std::time::Duration,
        cancel: &std::sync::atomic::AtomicBool,
    ) -> Result<SignedIn, ProviderError> {
        chatgpt_plan_route().map_err(ProviderError::Credential)?;
        let done = chatgpt_oauth::complete(&self.transport, &self.secrets, signin, timeout, cancel)
            .map_err(credential_error)?;
        self.mutate(chatgpt_oauth::PROVIDER_ID, |s| s.last_error = None);
        Ok(done)
    }

    /// Sign out: revoke the renewable session at OpenAI and clear the tokens
    /// here. Allowed whether or not the route is currently open, so a user is
    /// never stuck signed in.
    pub fn chatgpt_sign_out(&self) -> Result<SignOutReport, ProviderError> {
        chatgpt_oauth::sign_out(&self.transport, &self.secrets).map_err(credential_error)
    }

    /// The sign-in as the UI may see it (no credential).
    pub fn chatgpt_status(&self) -> Result<chatgpt_oauth::Status, ProviderError> {
        chatgpt_plan_route().map_err(ProviderError::Credential)?;
        chatgpt_oauth::status(&self.secrets).map_err(credential_error)
    }

    pub fn transport(&self) -> &T {
        &self.transport
    }

    /// Effective endpoint for provider calls. Credential material never
    /// crosses this boundary; autonomous adapters only receive the URL.
    pub fn base_url(&self, provider: &str) -> Result<String, ProviderError> {
        let spec =
            find_spec(provider).ok_or_else(|| ProviderError::Unsupported(provider.to_string()))?;
        self.resolve_base_url(
            provider,
            self.state_for(provider).base_url,
            spec.default_base_url,
        )
        .ok_or_else(|| ProviderError::Unsupported("no base URL configured".into()))
    }

    pub fn catalog(&self, provider: &str) -> Option<ModelCatalog> {
        self.state_for(provider).catalog
    }

    /// Seed a recorded catalog (development replay). Explicitly labeled so
    /// the UI can say "recorded" rather than implying a fresh live fetch.
    pub fn seed_replay(&self, provider: &str, catalog: ModelCatalog) {
        let mut catalog = catalog;
        catalog.source = CatalogSource::Replay;
        self.mutate(provider, |s| s.catalog = Some(catalog));
    }

    /// Fetch the provider's live model catalog and cache it. On failure the
    /// previous catalog (if any) stays and the error is recorded.
    pub fn refresh(&self, provider: &str) -> Result<ModelCatalog, ProviderError> {
        let spec =
            find_spec(provider).ok_or_else(|| ProviderError::Unsupported(provider.to_string()))?;
        let key = self
            .get_key(provider)?
            .ok_or(ProviderError::NotConfigured)?;
        let base = self
            .resolve_base_url(
                provider,
                self.state_for(provider).base_url,
                spec.default_base_url,
            )
            .ok_or_else(|| ProviderError::Unsupported("no base URL configured".into()))?;

        let result = fetch_models(&self.transport, &spec, &base, &key);
        match result {
            Ok(models) => {
                if models.is_empty() {
                    self.mutate(provider, |s| {
                        s.last_error = Some(ProviderError::EmptyCatalog)
                    });
                    return Err(ProviderError::EmptyCatalog);
                }
                let catalog = ModelCatalog::live(provider, now_secs(), models);
                self.mutate(provider, |s| {
                    s.catalog = Some(catalog.clone());
                    s.last_error = None;
                });
                Ok(catalog)
            }
            Err(e) => {
                self.mutate(provider, |s| s.last_error = Some(e.clone()));
                Err(e)
            }
        }
    }
}

fn check_status(status: u16) -> Result<(), ProviderError> {
    match status {
        200..=299 => Ok(()),
        401 | 403 => Err(ProviderError::AuthFailed),
        429 => Err(ProviderError::RateLimited),
        404 => Err(ProviderError::Unsupported(
            "endpoint has no model listing (404)".into(),
        )),
        other => Err(ProviderError::InvalidResponse(format!("HTTP {other}"))),
    }
}

fn fetch_models(
    transport: &dyn Transport,
    spec: &ProviderSpec,
    base: &str,
    key: &str,
) -> Result<Vec<ModelInfo>, ProviderError> {
    if spec.discovery == ModelDiscovery::Manual {
        return Err(ProviderError::Unsupported(
            "this endpoint has no discovery API; enter a model ID manually".into(),
        ));
    }
    match spec.protocol {
        ProviderProtocol::Gemini => fetch_gemini(transport, base, key, spec.id),
        ProviderProtocol::Anthropic => fetch_anthropic(transport, base, key, spec.id),
        ProviderProtocol::OpenAiCompatible => fetch_openai_models(transport, base, key, spec.id),
        ProviderProtocol::OpenAiResponses => {
            fetch_chatgpt_plan_models(transport, base, key, spec.id)
        }
    }
}

/// `GET /models` for a ChatGPT-plan token: a `models` array of
/// `{slug, display_name, visibility}`. Only `visibility: "list"` entries are
/// meant for a picker, and the server's order is kept: the first one is the
/// default, so it is not re-sorted.
fn fetch_chatgpt_plan_models(
    transport: &dyn Transport,
    base: &str,
    key: &str,
    provider: &str,
) -> Result<Vec<ModelInfo>, ProviderError> {
    let url = format!("{base}/models");
    let headers = vec![("authorization".to_string(), format!("Bearer {key}"))];
    let (status, body) = transport.get(&url, &headers)?;
    check_status(status)?;
    let listed =
        crate::responses::parse_model_list(&body).map_err(ProviderError::InvalidResponse)?;
    Ok(listed
        .into_iter()
        .map(|(slug, label)| ModelInfo {
            id: slug,
            label,
            provider: provider.to_string(),
        })
        .collect())
}

fn fetch_gemini(
    transport: &dyn Transport,
    base: &str,
    key: &str,
    provider: &str,
) -> Result<Vec<ModelInfo>, ProviderError> {
    let mut models = Vec::new();
    let mut page_token: Option<String> = None;
    loop {
        let mut url = format!("{base}/v1beta/models?pageSize=100");
        if let Some(token) = &page_token {
            url.push_str("&pageToken=");
            url.push_str(token);
        }
        let headers = vec![("x-goog-api-key".to_string(), key.to_string())];
        let (status, body) = transport.get(&url, &headers)?;
        check_status(status)?;
        let value: serde_json::Value = serde_json::from_str(&body)
            .map_err(|e| ProviderError::InvalidResponse(format!("invalid JSON: {e}")))?;
        let list = value
            .get("models")
            .and_then(|m| m.as_array())
            .ok_or_else(|| ProviderError::InvalidResponse("missing models array".into()))?;
        for entry in list {
            let name = entry.get("name").and_then(|n| n.as_str()).unwrap_or("");
            let id = name.strip_prefix("models/").unwrap_or(name);
            if id.is_empty() {
                continue;
            }
            let generates = entry
                .get("supportedGenerationMethods")
                .and_then(|m| m.as_array())
                .map(|methods| {
                    methods
                        .iter()
                        .any(|m| m.as_str() == Some("generateContent"))
                })
                .unwrap_or(false);
            if !generates {
                continue;
            }
            let label = entry
                .get("displayName")
                .and_then(|d| d.as_str())
                .filter(|d| !d.is_empty())
                .unwrap_or(id)
                .to_string();
            models.push(ModelInfo {
                id: id.to_string(),
                label,
                provider: provider.to_string(),
            });
        }
        page_token = value
            .get("nextPageToken")
            .and_then(|t| t.as_str())
            .filter(|t| !t.is_empty())
            .map(str::to_string);
        if page_token.is_none() {
            break;
        }
    }
    models.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(models)
}

fn fetch_anthropic(
    transport: &dyn Transport,
    base: &str,
    key: &str,
    provider: &str,
) -> Result<Vec<ModelInfo>, ProviderError> {
    let mut models = Vec::new();
    let mut after: Option<String> = None;
    loop {
        let mut url = format!("{base}/v1/models?limit=100");
        if let Some(id) = &after {
            url.push_str("&after_id=");
            url.push_str(id);
        }
        let headers = vec![
            ("x-api-key".to_string(), key.to_string()),
            ("anthropic-version".to_string(), "2023-06-01".to_string()),
        ];
        let (status, body) = transport.get(&url, &headers)?;
        check_status(status)?;
        let value: serde_json::Value = serde_json::from_str(&body)
            .map_err(|e| ProviderError::InvalidResponse(format!("invalid JSON: {e}")))?;
        let list = value
            .get("data")
            .and_then(|d| d.as_array())
            .ok_or_else(|| ProviderError::InvalidResponse("missing data array".into()))?;
        for entry in list {
            let id = entry.get("id").and_then(|i| i.as_str()).unwrap_or("");
            if id.is_empty() {
                continue;
            }
            let label = entry
                .get("display_name")
                .and_then(|d| d.as_str())
                .filter(|d| !d.is_empty())
                .unwrap_or(id)
                .to_string();
            models.push(ModelInfo {
                id: id.to_string(),
                label,
                provider: provider.to_string(),
            });
        }
        let has_more = value
            .get("has_more")
            .and_then(|h| h.as_bool())
            .unwrap_or(false);
        if !has_more {
            break;
        }
        after = value
            .get("last_id")
            .and_then(|t| t.as_str())
            .filter(|t| !t.is_empty())
            .map(str::to_string);
        if after.is_none() {
            break;
        }
    }
    models.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(models)
}

fn fetch_openai_models(
    transport: &dyn Transport,
    base: &str,
    key: &str,
    provider: &str,
) -> Result<Vec<ModelInfo>, ProviderError> {
    let url = format!("{base}/models");
    let headers = vec![("authorization".to_string(), format!("Bearer {key}"))];
    let (status, body) = transport.get(&url, &headers)?;
    check_status(status)?;
    let value: serde_json::Value = serde_json::from_str(&body)
        .map_err(|e| ProviderError::InvalidResponse(format!("invalid JSON: {e}")))?;
    let list = value
        .get("data")
        .and_then(|d| d.as_array())
        .ok_or_else(|| ProviderError::InvalidResponse("missing data array".into()))?;
    let mut models: Vec<ModelInfo> = list
        .iter()
        .filter_map(|entry| entry.get("id").and_then(|i| i.as_str()))
        .filter(|id| !id.is_empty())
        .map(|id| ModelInfo {
            id: id.to_string(),
            label: id.to_string(),
            provider: provider.to_string(),
        })
        .collect();
    models.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(models)
}
