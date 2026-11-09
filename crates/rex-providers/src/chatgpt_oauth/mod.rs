//! Sign in with ChatGPT: using a ChatGPT plan from an open-source client.
//!
//! This implements OpenAI's own contract for open-source, locally hosted
//! clients (developers.openai.com/siwc, *ChatGPT plan usage*, read
//! 2026-11-09). Nothing here is borrowed from another product:
//!
//! * the client is **registered dynamically** at first sign-in
//!   (`client_id=dynamic_agent_client`, `agent_name_hint`, a persisted
//!   `ext_agent_host_id`), and OpenAI returns REX's own issued `client_id`.
//!   There is no client secret, no partner API key, and no first-party client
//!   id copied from another application;
//! * authorization is OpenID Connect with PKCE (S256), a `state` and a
//!   `nonce`, to a loopback redirect on `127.0.0.1`;
//! * the ID token is verified against OpenAI's published JWKS, and the
//!   granted scopes are checked for `chatgpt.tokens.use.direct` before any
//!   inference;
//! * inference uses the public Responses API (`responses.rs`), never
//!   ChatGPT's `backend-api`;
//! * tokens are stored protected, replaced together on refresh, refreshed
//!   under a lock, and revoked at sign-out.
//!
//! OpenAI offers this to open-source and locally hosted apps (and to selected
//! partners). Whether REX may use it therefore depends on REX being released
//! as open source: see `policy::OPEN_SOURCE_RELEASE`, which gates every
//! entry point that reaches the network.

pub mod jwt;
pub mod launch;
pub mod loopback;
pub mod store;

use crate::error::ProviderError;
use crate::http::Transport;
use crate::secrets::SecretStore;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
pub use launch::{open_in_browser, SignInSessions, SignInState, SignInTicket};
use serde::Serialize;
use serde_json::Value;
use std::sync::atomic::AtomicBool;
use std::time::Duration;
pub use store::{Record, Registration, Tokens};

/// The registry id of the provider this sign-in feeds.
pub const PROVIDER_ID: &str = "openai-chatgpt";
pub const ISSUER: &str = "https://auth.openai.com";
pub const AUTHORIZE_URL: &str = "https://auth.openai.com/api/accounts/authorize";
pub const TOKEN_URL: &str = "https://auth.openai.com/api/accounts/oauth/token";
pub const DISCOVERY_URL: &str = "https://auth.openai.com/.well-known/openid-configuration";
/// The resource the token is for: the public API, not `backend-api`.
pub const RESOURCE: &str = "https://api.openai.com/v1";
/// The entry point for a first registration; never saved or used for exchange.
pub const DYNAMIC_CLIENT_ID: &str = "dynamic_agent_client";
/// The name shown to the user at consent, used consistently across installs.
pub const AGENT_NAME: &str = "REX";
const IDENTITY_SCOPES: &str = "openid profile email";
const PLAN_SCOPES: &str = "offline_access resource.invoke chatgpt.tokens.use.direct";
/// The scope that authorises spending the user's ChatGPT plan.
pub const PLAN_SCOPE: &str = "chatgpt.tokens.use.direct";
/// Refresh this long before the access token expires.
pub const REFRESH_SKEW_SECS: u64 = 300;
/// How long a browser sign-in may take.
pub const SIGN_IN_TIMEOUT: Duration = Duration::from_secs(300);
const HOST: &str = "auth.openai.com";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OAuthError {
    Io(String),
    Cancelled,
    TimedOut,
    /// The user declined consent (`access_denied`).
    Denied,
    /// An OAuth error code from OpenAI (identifier-shaped only).
    Provider(String),
    /// A new registration came back without an issued client id.
    RegistrationIncomplete,
    /// The callback named a different client than the sign-in began with.
    ClientMismatch,
    /// The signed-in account is not the one the registration belongs to.
    AccountMismatch,
    IdToken(String),
    Discovery(String),
    Exchange(String),
    NotSignedIn,
    /// Signed in, but the user did not grant ChatGPT plan usage.
    PlanUsageDisabled,
    /// The refresh token is unusable: sign in again.
    NeedsSignIn(String),
    /// A network or server failure; credentials were kept.
    Transient(String),
    Store(String),
}

impl std::fmt::Display for OAuthError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        use OAuthError::*;
        match self {
            Io(m) => write!(f, "cannot run the sign-in: {m}"),
            Cancelled => write!(f, "sign-in cancelled"),
            TimedOut => write!(f, "sign-in timed out; nothing came back from the browser"),
            Denied => write!(
                f,
                "ChatGPT plan usage was declined; REX can sign you in but will not spend your plan until you allow it (or use an API key)"
            ),
            Provider(c) => write!(f, "OpenAI refused the sign-in [{c}]"),
            RegistrationIncomplete => {
                write!(f, "OpenAI did not return a client id for this registration")
            }
            ClientMismatch => write!(f, "the sign-in came back for a different client; refusing it"),
            AccountMismatch => write!(
                f,
                "that is a different ChatGPT account from the one this registration belongs to; sign out first to switch accounts"
            ),
            IdToken(m) => write!(f, "{m}"),
            Discovery(m) => write!(f, "cannot reach OpenAI's sign-in configuration: {m}"),
            Exchange(m) => write!(f, "the token request failed: {m}"),
            NotSignedIn => write!(
                f,
                "not signed in with ChatGPT; sign in first (`rex login chatgpt`, or Continue with ChatGPT in the app)"
            ),
            PlanUsageDisabled => write!(
                f,
                "signed in, but ChatGPT plan usage is not enabled for REX. Enable it, or use an API key instead"
            ),
            NeedsSignIn(c) => write!(f, "the ChatGPT sign-in has expired or was revoked [{c}]; sign in again"),
            Transient(m) => write!(f, "temporary failure, credentials kept: {m}"),
            Store(m) => write!(f, "{m}"),
        }
    }
}

impl std::error::Error for OAuthError {}

impl From<ProviderError> for OAuthError {
    fn from(e: ProviderError) -> Self {
        OAuthError::Store(e.to_string())
    }
}

fn is_code_shaped(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 64
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.'))
}

fn random_b64u(bytes: usize) -> Result<String, OAuthError> {
    use ring::rand::{SecureRandom, SystemRandom};
    let mut b = vec![0u8; bytes];
    SystemRandom::new()
        .fill(&mut b)
        .map_err(|_| OAuthError::Io("no secure random source".into()))?;
    Ok(URL_SAFE_NO_PAD.encode(b))
}

/// The PKCE code challenge for `verifier`: base64url(SHA-256), no padding.
pub fn pkce_challenge(verifier: &str) -> String {
    URL_SAFE_NO_PAD.encode(ring::digest::digest(
        &ring::digest::SHA256,
        verifier.as_bytes(),
    ))
}

/// Only OpenAI's auth host is ever sent a token or trusted for keys.
fn trusted_url(url: &str) -> bool {
    url::Url::parse(url)
        .ok()
        .is_some_and(|u| u.scheme() == "https" && u.host_str() == Some(HOST))
}

/// A sign-in attempt that has opened its listener and built its URL.
pub struct SignIn {
    listener: loopback::Listener,
    state: String,
    nonce: String,
    verifier: String,
    redirect_uri: String,
    /// The client the attempt runs as: the issued one, or the dynamic entry.
    client_id: String,
    registering: bool,
    host_id: String,
    /// The account a reauthorization must come back as.
    expected_subject: Option<String>,
    authorize_url: String,
    display_url: String,
    launch_url: String,
}

impl SignIn {
    /// The full URL to open in the user's browser. It can carry an ID-token
    /// hint, so it is for the browser only: never log it.
    pub fn authorize_url(&self) -> &str {
        &self.authorize_url
    }

    /// The URL to hand to a browser launcher: a one-shot loopback path that
    /// redirects to the real authorization URL. Unlike [`authorize_url`], it
    /// is safe on a command line or in a process list: it names no account,
    /// carries no hint, and works once.
    ///
    /// [`authorize_url`]: SignIn::authorize_url
    pub fn launch_url(&self) -> &str {
        &self.launch_url
    }

    /// The same URL with the account hints blanked, safe to show or log.
    pub fn display_url(&self) -> &str {
        &self.display_url
    }

    pub fn redirect_uri(&self) -> &str {
        &self.redirect_uri
    }

    pub fn is_registration(&self) -> bool {
        self.registering
    }
}

/// Start a sign-in: choose and persist the host id, start the loopback
/// listener, and build the authorization URL.
///
/// `force_consent` asks OpenAI to show the permission screen again. Use it
/// only when the user explicitly chose to enable ChatGPT plan usage after
/// declining it; never on an ordinary sign-in.
pub fn begin(store: &dyn SecretStore, force_consent: bool) -> Result<SignIn, OAuthError> {
    let host_id = store::host_id(store)?;
    let saved = store::load(store)?;
    let listener = loopback::Listener::bind().map_err(|e| OAuthError::Io(e.to_string()))?;
    let redirect_uri = listener.redirect_uri();
    let (state, nonce, verifier) = (random_b64u(32)?, random_b64u(32)?, random_b64u(32)?);
    let registering = saved.is_none();
    let client_id = saved
        .as_ref()
        .map(|r| r.registration.client_id.clone())
        .unwrap_or_else(|| DYNAMIC_CLIENT_ID.to_string());

    let mut params: Vec<(&str, String)> = vec![
        ("client_id", client_id.clone()),
        ("ext_agent_host_id", host_id.clone()),
        ("response_type", "code".into()),
        ("redirect_uri", redirect_uri.clone()),
        ("scope", format!("{IDENTITY_SCOPES} {PLAN_SCOPES}")),
        ("resource", RESOURCE.into()),
        ("state", state.clone()),
        ("nonce", nonce.clone()),
        ("code_challenge_method", "S256".into()),
        ("code_challenge", pkce_challenge(&verifier)),
    ];
    if registering {
        // only on the first registration; omitted on reauthorization
        params.push(("agent_name_hint", AGENT_NAME.into()));
    } else if let Some(rec) = &saved {
        if let Some(t) = rec.tokens.as_ref().map(|t| t.id_token.clone()) {
            params.push(("id_token_hint", t));
        }
        if let Some(email) = &rec.registration.email {
            params.push(("login_hint", email.clone()));
        }
    }
    if force_consent {
        params.push(("prompt", "consent".into()));
    }
    let build = |redact: bool| -> String {
        let mut url = url::Url::parse(AUTHORIZE_URL).expect("a constant URL");
        {
            let mut q = url.query_pairs_mut();
            for (k, v) in &params {
                let hidden = redact && matches!(*k, "id_token_hint" | "login_hint");
                q.append_pair(k, if hidden { "<redacted>" } else { v });
            }
        }
        url.to_string()
    };
    let authorize_url = build(false);
    let listener_launch = listener
        .set_launch(&authorize_url)
        .map_err(|e| OAuthError::Io(e.to_string()))?;
    Ok(SignIn {
        listener,
        state,
        nonce,
        verifier,
        redirect_uri,
        client_id,
        registering,
        host_id,
        expected_subject: saved.as_ref().map(|r| r.registration.subject.clone()),
        launch_url: listener_launch,
        authorize_url,
        display_url: build(true),
    })
}

/// What a finished sign-in tells the caller. No credential is in it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SignedIn {
    pub email: Option<String>,
    /// Whether the user granted ChatGPT plan usage. When `false` the sign-in
    /// is kept but nothing can be spent until the user enables it.
    pub plan_usage: bool,
}

struct TokenResponse {
    access_token: String,
    refresh_token: Option<String>,
    id_token: Option<String>,
    token_type: String,
    expires_in: u64,
    scope: Option<String>,
    earliest_refresh_at: Option<u64>,
}

fn parse_token_response(body: &str) -> Result<TokenResponse, OAuthError> {
    let v: Value = serde_json::from_str(body)
        .map_err(|_| OAuthError::Exchange("the token response was not JSON".into()))?;
    let text = |k: &str| v.get(k).and_then(Value::as_str).map(str::to_string);
    let numeric = |k: &str| {
        v.get(k).and_then(|x| {
            x.as_u64()
                .or_else(|| x.as_str().and_then(|s| s.parse::<u64>().ok()))
        })
    };
    Ok(TokenResponse {
        access_token: text("access_token")
            .filter(|t| !t.is_empty())
            .ok_or_else(|| OAuthError::Exchange("the token response had no access token".into()))?,
        refresh_token: text("refresh_token").filter(|t| !t.is_empty()),
        id_token: text("id_token").filter(|t| !t.is_empty()),
        token_type: text("token_type").unwrap_or_else(|| "Bearer".into()),
        expires_in: numeric("expires_in").unwrap_or(3600),
        scope: text("scope"),
        earliest_refresh_at: numeric("earliest_refresh_at"),
    })
}

/// POST a form to the token endpoint. A 4xx is OpenAI refusing (its error
/// code is kept, nothing else); a network failure, 429 or 5xx is temporary.
fn token_request<T: Transport + ?Sized>(
    transport: &T,
    form: &[(&str, &str)],
) -> Result<TokenResponse, OAuthError> {
    let body = url::form_urlencoded::Serializer::new(String::new())
        .extend_pairs(form)
        .finish();
    let headers = vec![
        (
            "content-type".to_string(),
            "application/x-www-form-urlencoded".to_string(),
        ),
        ("accept".to_string(), "application/json".to_string()),
    ];
    let (status, text) = transport
        .post(TOKEN_URL, &headers, &body)
        .map_err(|e| OAuthError::Transient(e.to_string()))?;
    match status {
        200 => parse_token_response(&text),
        429 | 500..=599 => Err(OAuthError::Transient(format!("HTTP {status}"))),
        _ => {
            let code = serde_json::from_str::<Value>(&text)
                .ok()
                .and_then(|v| v.get("error").and_then(Value::as_str).map(str::to_string))
                .filter(|c| is_code_shaped(c))
                .unwrap_or_else(|| format!("http_{status}"));
            Err(OAuthError::Provider(code))
        }
    }
}

struct Discovery {
    jwks_uri: String,
    revocation_endpoint: Option<String>,
}

/// OpenAI's published endpoints. Anything that is not on its auth host is
/// refused, so a tampered document cannot send a token elsewhere.
fn discover<T: Transport + ?Sized>(transport: &T) -> Result<Discovery, OAuthError> {
    let (status, body) = transport
        .get(DISCOVERY_URL, &[])
        .map_err(|e| OAuthError::Discovery(e.to_string()))?;
    if status != 200 {
        return Err(OAuthError::Discovery(format!("HTTP {status}")));
    }
    let v: Value =
        serde_json::from_str(&body).map_err(|_| OAuthError::Discovery("not JSON".into()))?;
    if v.get("issuer").and_then(Value::as_str) != Some(ISSUER) {
        return Err(OAuthError::Discovery("the issuer is not OpenAI's".into()));
    }
    let jwks_uri = v
        .get("jwks_uri")
        .and_then(Value::as_str)
        .filter(|u| trusted_url(u))
        .ok_or_else(|| OAuthError::Discovery("no trusted jwks_uri".into()))?
        .to_string();
    let revocation_endpoint = v
        .get("revocation_endpoint")
        .and_then(Value::as_str)
        .filter(|u| trusted_url(u))
        .map(str::to_string);
    Ok(Discovery {
        jwks_uri,
        revocation_endpoint,
    })
}

fn fetch_jwks<T: Transport + ?Sized>(transport: &T, uri: &str) -> Result<Value, OAuthError> {
    let (status, body) = transport
        .get(uri, &[])
        .map_err(|e| OAuthError::Discovery(e.to_string()))?;
    if status != 200 {
        return Err(OAuthError::Discovery(format!("key set HTTP {status}")));
    }
    serde_json::from_str(&body).map_err(|_| OAuthError::Discovery("key set is not JSON".into()))
}

/// Wait for the browser, then finish the sign-in: exchange the code,
/// verify the ID token, check the granted scopes, and store the result.
pub fn complete<T: Transport + ?Sized>(
    transport: &T,
    store: &dyn SecretStore,
    signin: &SignIn,
    timeout: Duration,
    cancel: &AtomicBool,
) -> Result<SignedIn, OAuthError> {
    let callback = signin
        .listener
        .wait(&signin.state, timeout, cancel)
        .map_err(|e| match e {
            loopback::WaitError::Timeout => OAuthError::TimedOut,
            loopback::WaitError::Cancelled => OAuthError::Cancelled,
            loopback::WaitError::Io(m) => OAuthError::Io(m),
        })?;
    if let Some(error) = &callback.error {
        // consent declined: stop, never exchange a code
        return Err(if error == "access_denied" {
            OAuthError::Denied
        } else if is_code_shaped(error) {
            OAuthError::Provider(error.clone())
        } else {
            OAuthError::Provider("unknown_error".into())
        });
    }
    let code = callback
        .code
        .as_deref()
        .filter(|c| !c.is_empty())
        .ok_or_else(|| OAuthError::Exchange("the redirect carried no code".into()))?;

    // Which client the exchange is for. A new registration must come back
    // with an issued id; a reauthorization keeps the one it began with and
    // rejects a callback that names another.
    let client_id = if signin.registering {
        match callback.client_id.as_deref() {
            Some(id) if !id.is_empty() && id != DYNAMIC_CLIENT_ID => id.to_string(),
            _ => return Err(OAuthError::RegistrationIncomplete),
        }
    } else {
        match callback.client_id.as_deref() {
            None | Some("") => signin.client_id.clone(),
            Some(id) if id == signin.client_id => id.to_string(),
            Some(_) => return Err(OAuthError::ClientMismatch),
        }
    };

    let tokens = token_request(
        transport,
        &[
            ("grant_type", "authorization_code"),
            ("client_id", &client_id),
            ("code", code),
            ("code_verifier", &signin.verifier),
            ("redirect_uri", &signin.redirect_uri),
            ("resource", RESOURCE),
        ],
    )?;

    let id_token = tokens
        .id_token
        .clone()
        .ok_or_else(|| OAuthError::IdToken("the token response had no ID token".into()))?;
    let discovery = discover(transport)?;
    let jwks = fetch_jwks(transport, &discovery.jwks_uri)?;
    let claims = jwt::verify(
        &id_token,
        &jwks,
        &jwt::Expect {
            issuer: ISSUER,
            audience: &client_id,
            nonce: &signin.nonce,
            now: store::now_secs(),
        },
    )
    .map_err(|e| OAuthError::IdToken(e.to_string()))?;
    let subject = claims["sub"].as_str().unwrap_or_default().to_string();
    // a reauthorization must be the same account before credentials change
    if signin
        .expected_subject
        .as_deref()
        .is_some_and(|s| s != subject)
    {
        return Err(OAuthError::AccountMismatch);
    }
    let email = claims
        .get("email")
        .and_then(Value::as_str)
        .map(str::to_string);
    let scopes: Vec<String> = tokens
        .scope
        .as_deref()
        .unwrap_or_default()
        .split_whitespace()
        .map(str::to_string)
        .collect();
    let now = store::now_secs();
    let record = Record {
        ext_agent_host_id: signin.host_id.clone(),
        registration: Registration {
            client_id,
            subject,
            email: email.clone(),
            issuer: ISSUER.to_string(),
        },
        tokens: Some(Tokens {
            id_token,
            access_token: tokens.access_token,
            refresh_token: tokens.refresh_token,
            token_type: tokens.token_type,
            expires_in: tokens.expires_in,
            scopes,
            saved_at: now,
            earliest_refresh_at: tokens.earliest_refresh_at,
        }),
    };
    let plan_usage = record.tokens.as_ref().is_some_and(Tokens::plan_usage);
    store::save(store, &record)?;
    Ok(SignedIn { email, plan_usage })
}

fn needs_refresh(t: &Tokens, now: u64) -> bool {
    if now.saturating_add(REFRESH_SKEW_SECS) < t.expires_at() {
        return false;
    }
    // the server may ask that a still-valid token not be refreshed yet
    if let Some(earliest) = t.earliest_refresh_at {
        if now < earliest && now < t.expires_at() {
            return false;
        }
    }
    true
}

/// Refresh codes after which the refresh token can never work again.
fn unusable_refresh_code(code: &str) -> bool {
    matches!(
        code,
        "invalid_grant"
            | "invalid_refresh_token"
            | "token_expired"
            | "refresh_token_expired"
            | "refresh_token_invalidated"
            | "refresh_token_reused"
    )
}

fn refresh_locked<T: Transport + ?Sized>(
    transport: &T,
    store: &dyn SecretStore,
    mut record: Record,
) -> Result<String, OAuthError> {
    let tokens = record.tokens.clone().ok_or(OAuthError::NotSignedIn)?;
    let refresh = tokens
        .refresh_token
        .clone()
        .ok_or_else(|| OAuthError::NeedsSignIn("no_refresh_token".into()))?;
    let result = token_request(
        transport,
        &[
            ("grant_type", "refresh_token"),
            ("client_id", &record.registration.client_id),
            ("refresh_token", &refresh),
            ("resource", RESOURCE),
        ],
    );
    match result {
        Ok(new) => {
            let scopes = new
                .scope
                .as_deref()
                .map(|s| s.split_whitespace().map(str::to_string).collect())
                .unwrap_or_else(|| tokens.scopes.clone());
            // replaced together: access token, expiry, scopes, refresh token
            let replaced = Tokens {
                id_token: new.id_token.unwrap_or(tokens.id_token),
                access_token: new.access_token,
                refresh_token: new.refresh_token.or(Some(refresh)),
                token_type: new.token_type,
                expires_in: new.expires_in,
                scopes,
                saved_at: store::now_secs(),
                earliest_refresh_at: new.earliest_refresh_at,
            };
            let access = replaced.access_token.clone();
            let plan = replaced.plan_usage();
            record.tokens = Some(replaced);
            store::save(store, &record)?;
            if plan {
                Ok(access)
            } else {
                Err(OAuthError::PlanUsageDisabled)
            }
        }
        Err(OAuthError::Provider(code)) if unusable_refresh_code(&code) => {
            // the grant is gone: clear what cannot work, keep the registration
            store::clear_tokens(store)?;
            Err(OAuthError::NeedsSignIn(code))
        }
        Err(e) => Err(e),
    }
}

/// A valid access token for the signed-in account, renewing it first when it
/// is near expiry. Credentials are only erased when OpenAI says the grant is
/// gone; a network or server failure leaves them as they were.
pub fn access_token<T: Transport + ?Sized>(
    transport: &T,
    store: &dyn SecretStore,
) -> Result<String, OAuthError> {
    let now = store::now_secs();
    let record = store::load(store)?.ok_or(OAuthError::NotSignedIn)?;
    let tokens = record.tokens.as_ref().ok_or(OAuthError::NotSignedIn)?;
    if !tokens.plan_usage() {
        return Err(OAuthError::PlanUsageDisabled);
    }
    if !needs_refresh(tokens, now) {
        return Ok(tokens.access_token.clone());
    }
    let _guard = store::lock_refresh(store);
    // another process or thread may have renewed it while this one waited
    let record = store::load(store)?.ok_or(OAuthError::NotSignedIn)?;
    let tokens = record.tokens.as_ref().ok_or(OAuthError::NotSignedIn)?;
    if !tokens.plan_usage() {
        return Err(OAuthError::PlanUsageDisabled);
    }
    if !needs_refresh(tokens, store::now_secs()) {
        return Ok(tokens.access_token.clone());
    }
    refresh_locked(transport, store, record)
}

/// What a sign-out managed to do.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SignOutReport {
    /// Whether OpenAI confirmed the renewable session was revoked.
    pub revoked: bool,
    /// Said to the user when it was not.
    pub note: Option<String>,
}

const REVOKE_ATTEMPTS: u32 = 3;

/// Sign out: revoke the renewable session at OpenAI, then clear the tokens
/// locally. The issued client and the host id are kept for a later sign-in.
/// If revocation cannot be confirmed the tokens are cleared anyway and the
/// user is told so (they can also disconnect REX in ChatGPT settings).
pub fn sign_out<T: Transport + ?Sized>(
    transport: &T,
    store: &dyn SecretStore,
) -> Result<SignOutReport, OAuthError> {
    let Some(record) = store::load(store)? else {
        return Ok(SignOutReport {
            revoked: true,
            note: None,
        });
    };
    let refresh = record.tokens.as_ref().and_then(|t| t.refresh_token.clone());
    let mut revoked = refresh.is_none(); // nothing renewable to revoke
    let mut note = None;
    if let Some(refresh) = refresh {
        if let Ok(Discovery {
            revocation_endpoint: Some(endpoint),
            ..
        }) = discover(transport)
        {
            let body = url::form_urlencoded::Serializer::new(String::new())
                .append_pair("token", &refresh)
                .append_pair("token_type_hint", "refresh_token")
                .append_pair("client_id", &record.registration.client_id)
                .finish();
            let headers = vec![(
                "content-type".to_string(),
                "application/x-www-form-urlencoded".to_string(),
            )];
            for attempt in 1..=REVOKE_ATTEMPTS {
                match transport.post(&endpoint, &headers, &body) {
                    Ok((200, _)) => {
                        revoked = true;
                        break;
                    }
                    // a network failure or 5xx is retried with backoff
                    Ok((500..=599, _)) | Err(_) if attempt < REVOKE_ATTEMPTS => {
                        std::thread::sleep(Duration::from_millis(200 * u64::from(attempt)));
                    }
                    _ => break,
                }
            }
        }
        if !revoked {
            note = Some(
                "REX signed you out here, but OpenAI did not confirm the session was revoked. You can disconnect REX in ChatGPT settings."
                    .to_string(),
            );
        }
    }
    store::clear_tokens(store)?;
    Ok(SignOutReport { revoked, note })
}

/// The sign-in as the UI may show it. No credential is in it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Status {
    pub signed_in: bool,
    pub email: Option<String>,
    pub plan_usage: bool,
    /// Unix seconds when the current access token expires.
    pub access_token_expires_at: Option<u64>,
}

pub fn status(store: &dyn SecretStore) -> Result<Status, OAuthError> {
    let record = store::load(store)?;
    let tokens = record.as_ref().and_then(|r| r.tokens.as_ref());
    Ok(Status {
        signed_in: tokens.is_some(),
        email: record.as_ref().and_then(|r| r.registration.email.clone()),
        plan_usage: tokens.is_some_and(Tokens::plan_usage),
        access_token_expires_at: tokens.map(Tokens::expires_at),
    })
}

/// A signed-in record for tests elsewhere in the crate that need a provider
/// the loop can run against: access token `AT-OLD`, refresh token `RT-OLD`,
/// the full plan scope, `saved_ago` seconds old with `expires_in` lifetime.
#[cfg(test)]
pub(crate) fn seed_signed_in(store: &dyn SecretStore, expires_in: u64, saved_ago: u64) {
    let now = store::now_secs();
    store::save(
        store,
        &Record {
            ext_agent_host_id: store::host_id(store).unwrap(),
            registration: Registration {
                client_id: "oaiapp_issued".into(),
                subject: "sub-1".into(),
                email: Some("alex@example.com".into()),
                issuer: ISSUER.into(),
            },
            tokens: Some(Tokens {
                id_token: "ID-OLD".into(),
                access_token: "AT-OLD".into(),
                refresh_token: Some("RT-OLD".into()),
                token_type: "Bearer".into(),
                expires_in,
                scopes: [
                    PLAN_SCOPE,
                    "email",
                    "offline_access",
                    "openid",
                    "profile",
                    "resource.invoke",
                ]
                .iter()
                .map(|s| s.to_string())
                .collect(),
                saved_at: now - saved_ago,
                earliest_refresh_at: None,
            }),
        },
    )
    .unwrap();
}

#[cfg(test)]
mod tests;
