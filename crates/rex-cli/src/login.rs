//! `rex login chatgpt`, `rex logout chatgpt`, `rex auth status`.
//!
//! Sign in with ChatGPT lets the user spend their own ChatGPT plan through
//! REX instead of an API key. The flow itself lives in `rex_providers`; this
//! file is only the command line around it: open the browser, wait for the
//! redirect, say what happened.
//!
//! What the terminal is allowed to show: the account's email, whether plan
//! usage is on, and the one-shot launch URL. Never a token, never the real
//! authorization URL (it can carry an ID-token hint).
//!
//! The route depends on REX being released as open source (see
//! `rex_providers::OPEN_SOURCE_RELEASE`); if that switch is ever turned off,
//! every command that would start or use a sign-in says so instead of trying.

use crate::exec::{state_dir, ExecError};
use rex_providers::{
    ChatGptStatus, FileSecretStore, ProviderError, ProviderService, UreqTransport,
    CHATGPT_SIGN_IN_TIMEOUT, CHATGPT_USAGE_URL,
};
use serde_json::json;
use std::sync::atomic::AtomicBool;

type Service = ProviderService<FileSecretStore, UreqTransport>;

fn service() -> Result<Service, ExecError> {
    let state = state_dir();
    std::fs::create_dir_all(&state)
        .map_err(|e| ExecError::internal(format!("cannot create state dir: {e}")))?;
    // The plain file store, not the environment-aware wrapper: a sign-in is
    // only ever what a browser sign-in produced, never an environment variable.
    let store = FileSecretStore::new(state)
        .map_err(|e| ExecError::internal(format!("cannot open credential store: {e}")))?;
    Ok(ProviderService::new(store, UreqTransport::new()))
}

/// A refusal the user can act on (route closed, plan usage off) is a usage
/// error; a store or protocol failure is an internal one.
fn failure(e: ProviderError) -> ExecError {
    match e {
        ProviderError::Credential(m) | ProviderError::Unsupported(m) => ExecError::usage(m),
        other => ExecError::internal(other.to_string()),
    }
}

/// `chatgpt` (or the provider id) is the only sign-in there is today.
fn expect_chatgpt(args: &[String], command: &str) -> Result<Vec<String>, ExecError> {
    match args.first().map(String::as_str) {
        Some("chatgpt") | Some("openai-chatgpt") => Ok(args[1..].to_vec()),
        Some(other) if !other.starts_with("--") => Err(ExecError::usage(format!(
            "rex {command}: unknown service '{other}'; the only sign-in is `chatgpt`"
        ))),
        _ => Err(ExecError::usage(format!(
            "usage: rex {command} chatgpt{}",
            if command == "login" {
                " [--no-browser] [--force-consent] [--json]"
            } else {
                " [--json]"
            }
        ))),
    }
}

pub(crate) fn run_login(args: &[String]) -> Result<i32, ExecError> {
    let rest = expect_chatgpt(args, "login")?;
    let (mut no_browser, mut force_consent, mut json_out) = (false, false, false);
    for a in &rest {
        match a.as_str() {
            "--no-browser" => no_browser = true,
            "--force-consent" => force_consent = true,
            "--json" => json_out = true,
            other => {
                return Err(ExecError::usage(format!(
                    "rex login chatgpt: unknown option '{other}'"
                )))
            }
        }
    }
    let svc = service()?;
    let signin = svc.chatgpt_begin_sign_in(force_consent).map_err(failure)?;

    eprintln!("Sign in with ChatGPT to let REX use your ChatGPT plan.");
    eprintln!("REX will spend your plan's usage, not API credit. You can review or limit it at {CHATGPT_USAGE_URL}");
    if signin.is_registration() {
        eprintln!(
            "This is REX's first sign-in on this machine; OpenAI will ask you to approve it."
        );
    }
    let opened = !no_browser && rex_providers::open_in_browser(signin.launch_url());
    if opened {
        eprintln!("Opened your browser. If it did not appear, open this link on this machine (it works once):");
    } else {
        eprintln!("Open this link in a browser on this machine (it works once):");
    }
    eprintln!("  {}", signin.launch_url());
    eprintln!("Waiting for you to finish (up to 5 minutes)...");

    let cancel = AtomicBool::new(false);
    let done = svc
        .chatgpt_complete(&signin, CHATGPT_SIGN_IN_TIMEOUT, &cancel)
        .map_err(failure)?;

    if json_out {
        println!(
            "{}",
            json!({"signed_in": true, "email": done.email, "plan_usage": done.plan_usage})
        );
    }
    let who = done.email.as_deref().unwrap_or("your ChatGPT account");
    if done.plan_usage {
        eprintln!("Signed in as {who}. Using your ChatGPT plan: use `--provider openai-chatgpt`.");
        Ok(0)
    } else {
        eprintln!(
            "Signed in as {who}, but ChatGPT plan usage was not allowed, so REX cannot spend your plan. \
             Run `rex login chatgpt --force-consent` to be asked again, or use an API key."
        );
        Ok(3)
    }
}

pub(crate) fn run_logout(args: &[String]) -> Result<i32, ExecError> {
    let rest = expect_chatgpt(args, "logout")?;
    let mut json_out = false;
    for a in &rest {
        match a.as_str() {
            "--json" => json_out = true,
            other => {
                return Err(ExecError::usage(format!(
                    "rex logout chatgpt: unknown option '{other}'"
                )))
            }
        }
    }
    let report = service()?.chatgpt_sign_out().map_err(failure)?;
    if json_out {
        println!("{}", json!({"signed_out": true, "revoked": report.revoked}));
    }
    eprintln!("Signed out of ChatGPT.");
    if let Some(note) = report.note {
        eprintln!("{note}");
    }
    Ok(0)
}

pub(crate) fn run_auth(args: &[String]) -> Result<i32, ExecError> {
    match args.first().map(String::as_str) {
        Some("status") => {}
        _ => return Err(ExecError::usage("usage: rex auth status [--json]")),
    }
    let mut json_out = false;
    for a in &args[1..] {
        match a.as_str() {
            "--json" => json_out = true,
            other => {
                return Err(ExecError::usage(format!(
                    "rex auth status: unknown option '{other}'"
                )))
            }
        }
    }
    let svc = service()?;
    match svc.chatgpt_status() {
        Ok(status) => {
            if json_out {
                println!(
                    "{}",
                    json!({"chatgpt": {"available": true, "signed_in": status.signed_in,
                        "email": status.email, "plan_usage": status.plan_usage}})
                );
            }
            eprintln!("{}", describe(&status));
            Ok(0)
        }
        // not signed in is a state; "not available yet" is too, not a failure
        Err(ProviderError::Credential(why)) => {
            if json_out {
                println!(
                    "{}",
                    json!({"chatgpt": {"available": false, "reason": why}})
                );
            }
            eprintln!("{why}");
            Ok(0)
        }
        Err(other) => Err(failure(other)),
    }
}

fn describe(status: &ChatGptStatus) -> String {
    if !status.signed_in {
        return "ChatGPT plan: not signed in. Run `rex login chatgpt`.".to_string();
    }
    let who = status.email.as_deref().unwrap_or("your ChatGPT account");
    if status.plan_usage {
        format!(
            "ChatGPT plan: signed in as {who}; plan usage is on. Review it at {CHATGPT_USAGE_URL}"
        )
    } else {
        format!(
            "ChatGPT plan: signed in as {who}, but plan usage is off. Run `rex login chatgpt --force-consent`."
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn only_the_chatgpt_sign_in_exists() {
        assert!(expect_chatgpt(&args(&["chatgpt", "--json"]), "login").is_ok());
        assert!(expect_chatgpt(&args(&["openai-chatgpt"]), "login").is_ok());
        for bad in [args(&[]), args(&["gemini"]), args(&["--json"])] {
            let e = expect_chatgpt(&bad, "login").unwrap_err();
            assert_eq!(e.code, 2, "{}", e.message);
        }
    }

    #[test]
    fn a_closed_route_is_a_usage_message_not_an_internal_error() {
        let e = failure(ProviderError::Credential(
            "Using a ChatGPT plan is not available yet.".into(),
        ));
        assert_eq!(e.code, 2);
        assert!(e.message.contains("not available yet"));
        assert_eq!(failure(ProviderError::Store("disk".into())).code, 1);
    }

    #[test]
    fn the_status_line_names_the_account_and_what_to_do_next() {
        let off = ChatGptStatus {
            signed_in: false,
            email: None,
            plan_usage: false,
            access_token_expires_at: None,
        };
        assert!(describe(&off).contains("rex login chatgpt"));
        let on = ChatGptStatus {
            signed_in: true,
            email: Some("alex@example.com".into()),
            plan_usage: true,
            access_token_expires_at: Some(1),
        };
        let line = describe(&on);
        assert!(line.contains("alex@example.com") && line.contains(CHATGPT_USAGE_URL));
        let no_plan = ChatGptStatus {
            plan_usage: false,
            ..on
        };
        assert!(describe(&no_plan).contains("--force-consent"));
    }
}
