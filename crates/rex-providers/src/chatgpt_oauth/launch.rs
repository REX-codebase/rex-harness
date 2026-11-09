//! Opening the user's browser, and running a sign-in in the background for
//! callers that cannot block on it (the desktop app and its dev sidecar).
//!
//! Only the one-shot loopback launch URL (`SignIn::launch_url`) is ever
//! handed to the operating system's launcher, so no ID-token hint or account
//! name can appear in a process list.

use super::{SignIn, SignedIn};
use crate::error::ProviderError;
use serde::Serialize;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// Hand `url` to the desktop's default browser. `false` when no launcher
/// could be started (a headless machine): the caller shows the link instead.
pub fn open_in_browser(url: &str) -> bool {
    use std::process::{Command, Stdio};
    let mut cmd = match std::env::consts::OS {
        "macos" => Command::new("open"),
        "windows" => {
            let mut c = Command::new("cmd");
            c.args(["/C", "start", ""]);
            c
        }
        _ => Command::new("xdg-open"),
    };
    cmd.arg(url)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .is_ok()
}

/// Where a background sign-in stands. Carries no credential.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum SignInState {
    /// The browser has not come back yet.
    Waiting,
    Done {
        email: Option<String>,
        /// `false` when the user signed in but did not allow plan usage.
        plan_usage: bool,
    },
    Failed {
        error: ProviderError,
    },
    Cancelled,
}

/// What a caller needs to send the user to the browser and ask how it went.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SignInTicket {
    pub attempt: String,
    /// Open this (once). It names no account and carries no hint.
    pub launch_url: String,
    /// The first sign-in on this machine: OpenAI will ask the user to approve REX.
    pub registering: bool,
    pub timeout_secs: u64,
    /// Whether this side already opened the browser.
    pub opened: bool,
}

struct Attempt {
    id: String,
    cancel: Arc<AtomicBool>,
    state: Arc<Mutex<SignInState>>,
}

/// The one sign-in that may be in flight. Starting another cancels it.
#[derive(Default)]
pub struct SignInSessions {
    current: Mutex<Option<Attempt>>,
}

impl SignInSessions {
    pub fn new() -> Self {
        Self::default()
    }

    /// Begin a sign-in and wait for it on a background thread.
    ///
    /// `begin` starts the loopback listener and builds the URL; `complete`
    /// runs on the thread and finishes the exchange. Both are closures over
    /// the provider service, so this stays independent of its type.
    pub fn start<B, C>(
        &self,
        begin: B,
        complete: C,
        timeout: Duration,
        open: bool,
    ) -> Result<SignInTicket, ProviderError>
    where
        B: FnOnce() -> Result<SignIn, ProviderError>,
        C: FnOnce(&SignIn, &AtomicBool) -> Result<SignedIn, ProviderError> + Send + 'static,
    {
        let mut current = self
            .current
            .lock()
            .map_err(|_| ProviderError::Store("sign-in state is poisoned".into()))?;
        if let Some(old) = current.take() {
            old.cancel.store(true, Ordering::SeqCst);
        }
        let signin = begin()?;
        let id = super::random_b64u(12).map_err(|e| ProviderError::Credential(e.to_string()))?;
        let cancel = Arc::new(AtomicBool::new(false));
        let state = Arc::new(Mutex::new(SignInState::Waiting));
        let launch_url = signin.launch_url().to_string();
        let registering = signin.is_registration();
        let opened = open && open_in_browser(&launch_url);
        {
            let (cancel, state) = (cancel.clone(), state.clone());
            std::thread::spawn(move || {
                let outcome = complete(&signin, &cancel);
                let next = match outcome {
                    Ok(done) => SignInState::Done {
                        email: done.email,
                        plan_usage: done.plan_usage,
                    },
                    Err(_) if cancel.load(Ordering::SeqCst) => SignInState::Cancelled,
                    Err(error) => SignInState::Failed { error },
                };
                if let Ok(mut slot) = state.lock() {
                    *slot = next;
                }
            });
        }
        *current = Some(Attempt {
            id: id.clone(),
            cancel,
            state,
        });
        Ok(SignInTicket {
            attempt: id,
            launch_url,
            registering,
            timeout_secs: timeout.as_secs(),
            opened,
        })
    }

    /// The state of attempt `id`, or `None` if it is not the current one.
    pub fn poll(&self, id: &str) -> Option<SignInState> {
        let current = self.current.lock().ok()?;
        let attempt = current.as_ref().filter(|a| a.id == id)?;
        let state = attempt.state.lock().ok()?.clone();
        Some(state)
    }

    /// Stop waiting for attempt `id`. `false` if it is not the current one.
    pub fn cancel(&self, id: &str) -> bool {
        let Ok(current) = self.current.lock() else {
            return false;
        };
        match current.as_ref().filter(|a| a.id == id) {
            Some(a) => {
                a.cancel.store(true, Ordering::SeqCst);
                true
            }
            None => false,
        }
    }
}
