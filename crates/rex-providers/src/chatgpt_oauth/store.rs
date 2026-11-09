//! Persisting the sign-in: one protected record, and the host id.
//!
//! OpenAI's guidance (*Accounts and sessions*, *Sign in*) is followed as
//! written:
//!
//! * the credentials live in protected local storage, written atomically with
//!   owner-only permissions, and are never committed, logged or sent to a
//!   frontend. The record goes through REX's existing [`SecretStore`], which
//!   is a `0600` file written by rename; this module adds no second store;
//! * the access token, its expiry, the granted scopes and the rotating refresh
//!   token are **replaced together** after a refresh;
//! * the issued `client_id`, the verified account and the host id survive a
//!   sign-out, so a later sign-in reuses the same registration and host. Only
//!   the tokens are cleared;
//! * the host id is chosen and persisted **before** the first sign-in;
//! * refreshes are **serialised** so two processes do not race a rotating
//!   token: an in-process mutex, plus a lock file beside the store for other
//!   processes (the CLI and the desktop sidecar share one store).

use super::PLAN_SCOPE;
use crate::error::ProviderError;
use crate::secrets::SecretStore;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard};
use std::time::{Duration, SystemTime};

/// The store key of the sign-in record. The `#` suffix follows the existing
/// convention (`<provider>#base_url`): a provider id can never contain it.
pub const RECORD_KEY: &str = "openai-chatgpt#oauth";
pub const HOST_KEY: &str = "openai-chatgpt#host_id";

/// The account a registration belongs to. Kept across sign-outs.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Registration {
    /// The issued client id (`oaiapp_...`), never `dynamic_agent_client`.
    pub client_id: String,
    /// The validated ID-token `sub`: the account's identity.
    pub subject: String,
    pub email: Option<String>,
    pub issuer: String,
}

/// The credentials. `Debug` shows none of them.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Tokens {
    pub id_token: String,
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub token_type: String,
    pub expires_in: u64,
    pub scopes: Vec<String>,
    /// Unix seconds when this token response was received.
    pub saved_at: u64,
    /// Unix seconds before which the server does not want a refresh, when it
    /// said so.
    pub earliest_refresh_at: Option<u64>,
}

impl std::fmt::Debug for Tokens {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Tokens")
            .field("scopes", &self.scopes)
            .field("expires_at", &self.expires_at())
            .field("credentials", &"<redacted>")
            .finish()
    }
}

impl Tokens {
    pub fn expires_at(&self) -> u64 {
        self.saved_at.saturating_add(self.expires_in)
    }

    /// Whether the user granted permission to use their ChatGPT plan. A valid
    /// ID token without this scope signs the user in but pays for nothing.
    pub fn plan_usage(&self) -> bool {
        self.scopes.iter().any(|s| s == PLAN_SCOPE)
    }
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Record {
    pub ext_agent_host_id: String,
    pub registration: Registration,
    /// `None` after a sign-out: the registration stays, the credentials go.
    pub tokens: Option<Tokens>,
}

impl std::fmt::Debug for Record {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Record")
            .field("client_id", &self.registration.client_id)
            .field("tokens", &self.tokens)
            .finish()
    }
}

pub fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

pub fn load(store: &dyn SecretStore) -> Result<Option<Record>, ProviderError> {
    match store.get_key(RECORD_KEY)? {
        None => Ok(None),
        Some(text) => serde_json::from_str(&text)
            .map(Some)
            .map_err(|_| ProviderError::Store("the saved ChatGPT sign-in is unreadable".into())),
    }
}

pub fn save(store: &dyn SecretStore, record: &Record) -> Result<(), ProviderError> {
    let text = serde_json::to_string(record)
        .map_err(|e| ProviderError::Store(format!("cannot save the sign-in: {e}")))?;
    store.set_key(RECORD_KEY, &text)
}

/// Sign out locally: drop the credentials, keep the registration and the host
/// id for a later sign-in.
pub fn clear_tokens(store: &dyn SecretStore) -> Result<(), ProviderError> {
    if let Some(mut record) = load(store)? {
        record.tokens = None;
        save(store, &record)?;
    }
    Ok(())
}

/// Forget the registration too (the account is gone or the user starts
/// over). The host id stays: it names this machine, not the account.
#[cfg(test)]
pub fn forget(store: &dyn SecretStore) -> Result<(), ProviderError> {
    store.clear_key(RECORD_KEY)
}

/// The id of this host (`urn:uuid:<UUIDv4>`), created once and persisted. It
/// is an opaque identifier, not a credential and not derived from anything
/// about the user.
pub fn host_id(store: &dyn SecretStore) -> Result<String, ProviderError> {
    if let Some(id) = store.get_key(HOST_KEY)?.filter(|id| valid_host_id(id)) {
        return Ok(id);
    }
    let id = new_host_id()?;
    store.set_key(HOST_KEY, &id)?;
    Ok(id)
}

fn valid_host_id(id: &str) -> bool {
    id.strip_prefix("urn:uuid:").is_some_and(|u| {
        u.len() == 36
            && u.chars().enumerate().all(|(i, c)| match i {
                8 | 13 | 18 | 23 => c == '-',
                _ => c.is_ascii_hexdigit(),
            })
    })
}

fn new_host_id() -> Result<String, ProviderError> {
    use ring::rand::{SecureRandom, SystemRandom};
    let mut b = [0u8; 16];
    SystemRandom::new()
        .fill(&mut b)
        .map_err(|_| ProviderError::Store("no secure random source".into()))?;
    b[6] = (b[6] & 0x0f) | 0x40; // version 4
    b[8] = (b[8] & 0x3f) | 0x80; // RFC 4122 variant
    let h: String = b.iter().map(|x| format!("{x:02x}")).collect();
    Ok(format!(
        "urn:uuid:{}-{}-{}-{}-{}",
        &h[0..8],
        &h[8..12],
        &h[12..16],
        &h[16..20],
        &h[20..32]
    ))
}

static REFRESH: Mutex<()> = Mutex::new(());
/// A lock file older than this is a crashed holder's, and is taken over.
const STALE_LOCK: Duration = Duration::from_secs(60);
/// How long to wait for another process to finish refreshing.
const LOCK_WAIT: Duration = Duration::from_secs(15);

/// Held while a refresh is in progress.
pub struct RefreshGuard {
    _in_process: MutexGuard<'static, ()>,
    lock_file: Option<PathBuf>,
}

impl Drop for RefreshGuard {
    fn drop(&mut self) {
        if let Some(p) = &self.lock_file {
            let _ = std::fs::remove_file(p);
        }
    }
}

/// Take the refresh lock. Callers re-read the record after this returns: a
/// process that waited may find the token already renewed.
pub fn lock_refresh(store: &dyn SecretStore) -> RefreshGuard {
    let in_process = REFRESH.lock().unwrap_or_else(|p| p.into_inner());
    let lock_file = store.lock_dir().map(|d| d.join("chatgpt-refresh.lock"));
    if let Some(path) = &lock_file {
        let start = std::time::Instant::now();
        loop {
            match std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(path)
            {
                Ok(_) => break,
                Err(_) => {
                    let stale = std::fs::metadata(path)
                        .and_then(|m| m.modified())
                        .ok()
                        .and_then(|t| t.elapsed().ok())
                        .is_some_and(|age| age > STALE_LOCK);
                    if stale || start.elapsed() > LOCK_WAIT {
                        // a crashed or stuck holder: take it over
                        let _ = std::fs::remove_file(path);
                        continue;
                    }
                    std::thread::sleep(Duration::from_millis(100));
                }
            }
        }
    }
    RefreshGuard {
        _in_process: in_process,
        lock_file,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::secrets::{FileSecretStore, MemorySecretStore};

    fn tokens() -> Tokens {
        Tokens {
            id_token: "ID.TOKEN.SECRET".into(),
            access_token: "ACCESS-SECRET".into(),
            refresh_token: Some("REFRESH-SECRET".into()),
            token_type: "Bearer".into(),
            expires_in: 3600,
            scopes: vec!["openid".into(), PLAN_SCOPE.into()],
            saved_at: 1_000,
            earliest_refresh_at: None,
        }
    }

    fn record() -> Record {
        Record {
            ext_agent_host_id: "urn:uuid:11111111-1111-4111-8111-111111111111".into(),
            registration: Registration {
                client_id: "oaiapp_x".into(),
                subject: "sub-1".into(),
                email: Some("a@example.com".into()),
                issuer: "https://auth.openai.com".into(),
            },
            tokens: Some(tokens()),
        }
    }

    #[test]
    fn debug_output_never_contains_a_credential() {
        let shown = format!("{:?} {:?}", record(), tokens());
        for secret in ["ID.TOKEN.SECRET", "ACCESS-SECRET", "REFRESH-SECRET"] {
            assert!(!shown.contains(secret), "{shown}");
        }
        assert!(shown.contains("redacted"));
    }

    #[test]
    fn a_record_round_trips_through_the_store() {
        let s = MemorySecretStore::new();
        assert!(load(&s).unwrap().is_none());
        save(&s, &record()).unwrap();
        assert_eq!(load(&s).unwrap().unwrap(), record());
    }

    #[test]
    fn signing_out_keeps_the_registration_and_drops_every_token() {
        let s = MemorySecretStore::new();
        save(&s, &record()).unwrap();
        clear_tokens(&s).unwrap();
        let r = load(&s).unwrap().unwrap();
        assert!(r.tokens.is_none());
        assert_eq!(
            r.registration.client_id, "oaiapp_x",
            "the client is reused next time"
        );
        let raw = s.get_key(RECORD_KEY).unwrap().unwrap();
        for secret in ["ID.TOKEN.SECRET", "ACCESS-SECRET", "REFRESH-SECRET"] {
            assert!(!raw.contains(secret), "a token survived sign-out");
        }
        // forgetting removes the registration as well
        forget(&s).unwrap();
        assert!(load(&s).unwrap().is_none());
    }

    #[test]
    fn a_corrupt_record_is_an_error_not_a_panic() {
        let s = MemorySecretStore::new();
        s.set_key(RECORD_KEY, "{not json").unwrap();
        assert!(load(&s).is_err());
    }

    #[test]
    fn token_expiry_and_the_plan_scope_are_read_from_the_record() {
        let t = tokens();
        assert_eq!(t.expires_at(), 4_600);
        assert!(t.plan_usage());
        let mut no_plan = tokens();
        no_plan.scopes = vec!["openid".into(), "email".into()];
        assert!(
            !no_plan.plan_usage(),
            "an ID token alone does not authorise plan usage"
        );
    }

    #[test]
    fn the_host_id_is_a_v4_uuid_urn_created_once_and_then_reused() {
        let s = MemorySecretStore::new();
        let a = host_id(&s).unwrap();
        assert!(valid_host_id(&a), "{a}");
        assert_eq!(&a[9 + 14..9 + 15], "4", "version 4: {a}");
        assert_eq!(host_id(&s).unwrap(), a, "persisted, not regenerated");
        assert_ne!(host_id(&MemorySecretStore::new()).unwrap(), a);
        // a damaged value is replaced by a fresh valid one
        s.set_key(HOST_KEY, "not-a-urn").unwrap();
        assert!(valid_host_id(&host_id(&s).unwrap()));
    }

    #[test]
    fn host_id_validation_is_strict() {
        assert!(valid_host_id(
            "urn:uuid:123e4567-e89b-42d3-a456-426614174000"
        ));
        for bad in [
            "",
            "123e4567-e89b-42d3-a456-426614174000",
            "urn:uuid:123e4567e89b42d3a456426614174000",
            "urn:uuid:123e4567-e89b-42d3-a456-42661417400g",
            "urn:uuid:123e4567-e89b-42d3-a456-4266141740000",
        ] {
            assert!(!valid_host_id(bad), "{bad}");
        }
    }

    #[test]
    fn the_refresh_lock_file_is_taken_released_and_excludes_a_second_holder() {
        let tmp = tempfile::tempdir().unwrap();
        let store = FileSecretStore::new(tmp.path().join("s")).unwrap();
        let lock = tmp.path().join("s").join("chatgpt-refresh.lock");
        {
            let _g = lock_refresh(&store);
            assert!(lock.exists());
        }
        assert!(!lock.exists(), "released on drop");
        // a stale lock from a crashed holder is taken over, not waited on forever
        std::fs::write(&lock, b"").unwrap();
        let old = SystemTime::now() - Duration::from_secs(600);
        let f = std::fs::File::options().write(true).open(&lock).unwrap();
        f.set_modified(old).unwrap();
        drop(f);
        let start = std::time::Instant::now();
        let _g = lock_refresh(&store);
        assert!(
            start.elapsed() < Duration::from_secs(3),
            "{:?}",
            start.elapsed()
        );
    }

    #[test]
    fn two_threads_never_refresh_at_once() {
        let store = std::sync::Arc::new(MemorySecretStore::new());
        let inside = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let max = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let handles: Vec<_> = (0..6)
            .map(|_| {
                let (store, inside, max) = (store.clone(), inside.clone(), max.clone());
                std::thread::spawn(move || {
                    let _g = lock_refresh(&*store);
                    let n = inside.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1;
                    max.fetch_max(n, std::sync::atomic::Ordering::SeqCst);
                    std::thread::sleep(Duration::from_millis(15));
                    inside.fetch_sub(1, std::sync::atomic::Ordering::SeqCst);
                })
            })
            .collect();
        for h in handles {
            h.join().unwrap();
        }
        assert_eq!(max.load(std::sync::atomic::Ordering::SeqCst), 1);
    }
}
