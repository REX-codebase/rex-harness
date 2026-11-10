//! Interactive terminal backend: PTY sessions bound to the workspace.
//!
//! Each terminal spawns the user's shell in a pseudo-terminal via
//! `portable-pty`. The sandbox is the rex-tools discipline applied to
//! interactive use:
//!
//! - the PTY starts in a folder the shell layer has already vetted against the
//!   registry of folders the person opened (no arbitrary cwd from the webview);
//! - the environment is rebuilt from an allowlist (see [`terminal_env`]): what a
//!   shell needs to work (`HOME`, `PATH`, the Windows system variables, locale,
//!   the SSH agent socket) and nothing else, so an API key or token in the
//!   app's own environment never reaches the shell;
//! - the child runs as the invoking user with no privilege escalation.
//!
//! This is the user's own terminal, not a tool sandbox: network access is
//! allowed because `git pull` and `npm install` are the point. The boundary
//! is folder confinement + a clean environment, enforced in Rust.

use std::collections::HashMap;
use std::ffi::OsString;
use std::io::Read;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use portable_pty::{native_pty_system, CommandBuilder, MasterPty, PtySize};

/// Variables a shell needs in order to work, copied from the app's own
/// environment when set. Everything else is left behind on purpose.
const UNIX_ENV: &[&str] = &[
    "PATH",
    "HOME",
    "USER",
    "LOGNAME",
    "SHELL",
    "LANG",
    "LANGUAGE",
    "LC_ALL",
    "LC_CTYPE",
    "LC_MESSAGES",
    "LC_COLLATE",
    "LC_TIME",
    "TZ",
    "TMPDIR",
    // lets `git push` over ssh use the person's agent; a socket path, not a key
    "SSH_AUTH_SOCK",
];
const WINDOWS_ENV: &[&str] = &[
    "PATH",
    "PATHEXT",
    "SYSTEMROOT",
    "WINDIR",
    "COMSPEC",
    "USERPROFILE",
    "HOMEDRIVE",
    "HOMEPATH",
    "APPDATA",
    "LOCALAPPDATA",
    "PROGRAMDATA",
    "PROGRAMFILES",
    "PROGRAMFILES(X86)",
    "PROGRAMW6432",
    "COMMONPROGRAMFILES",
    "TEMP",
    "TMP",
    "USERNAME",
    "USERDOMAIN",
    "COMPUTERNAME",
    "OS",
    "PROCESSOR_ARCHITECTURE",
    "NUMBER_OF_PROCESSORS",
    "SSH_AUTH_SOCK",
];

/// The environment a new shell starts with, built from `vars` (the app's own
/// environment) by allowlist, plus the terminal's own settings.
pub fn terminal_env(
    vars: impl IntoIterator<Item = (OsString, OsString)>,
    windows: bool,
) -> Vec<(OsString, OsString)> {
    let allow = if windows { WINDOWS_ENV } else { UNIX_ENV };
    let mut out: Vec<(OsString, OsString)> = vars
        .into_iter()
        .filter(|(k, _)| {
            let key = k.to_string_lossy();
            // Windows names are case-insensitive; Unix names are not
            if windows {
                allow.contains(&key.to_uppercase().as_str())
            } else {
                allow.contains(&key.as_ref())
            }
        })
        .collect();
    let has = |out: &[(OsString, OsString)], name: &str| {
        out.iter().any(|(k, _)| k.to_string_lossy() == name)
    };
    if !has(&out, "PATH") && !windows {
        out.push(("PATH".into(), "/usr/local/bin:/usr/bin:/bin".into()));
    }
    // an app started from the Dock has no locale, and a shell without a UTF-8
    // locale prints mojibake for every accented character
    if !windows && !has(&out, "LANG") && !has(&out, "LC_ALL") {
        out.push(("LANG".into(), "en_US.UTF-8".into()));
    }
    out.push(("TERM".into(), "xterm-256color".into()));
    out.push(("COLORTERM".into(), "truecolor".into()));
    out
}

/// Turns the PTY's raw byte reads into text without splitting a multi-byte
/// character that straddles two reads (which a per-chunk lossy decode turns
/// into replacement characters, mid-word, in the middle of any accented text).
#[derive(Default)]
pub struct Utf8Stream {
    pending: Vec<u8>,
}

impl Utf8Stream {
    pub fn push(&mut self, bytes: &[u8]) -> String {
        self.pending.extend_from_slice(bytes);
        let mut out = String::new();
        loop {
            match std::str::from_utf8(&self.pending) {
                Ok(valid) => {
                    out.push_str(valid);
                    self.pending.clear();
                    return out;
                }
                Err(e) => {
                    let good = e.valid_up_to();
                    out.push_str(std::str::from_utf8(&self.pending[..good]).unwrap_or(""));
                    match e.error_len() {
                        // an incomplete character at the end: keep it for the next read
                        None => {
                            self.pending.drain(..good);
                            return out;
                        }
                        // genuinely invalid bytes: one replacement character, move on
                        Some(n) => {
                            out.push('\u{fffd}');
                            self.pending.drain(..good + n);
                        }
                    }
                }
            }
        }
    }

    /// Whatever is left when the stream ends.
    pub fn finish(&mut self) -> String {
        let rest = String::from_utf8_lossy(&self.pending).to_string();
        self.pending.clear();
        rest
    }
}

/// A live PTY session.
struct TerminalSession {
    master: Box<dyn MasterPty + Send>,
    child: Box<dyn portable_pty::Child + Send + Sync>,
    writer: Box<dyn std::io::Write + Send>,
}

/// Manages PTY sessions by ID. Held in Tauri state.
pub struct TerminalManager {
    sessions: Mutex<HashMap<String, TerminalSession>>,
    next_id: Mutex<u64>,
}

impl TerminalManager {
    pub fn new() -> Self {
        TerminalManager {
            sessions: Mutex::new(HashMap::new()),
            next_id: Mutex::new(0),
        }
    }

    /// Spawn a shell in a PTY started in `cwd`, which the caller has already
    /// vetted against the folders the person opened. Returns the terminal ID.
    pub fn spawn(&self, cwd: PathBuf, cols: u16, rows: u16) -> Result<String, String> {
        let pty_system = native_pty_system();
        let pair = pty_system
            .openpty(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|e| format!("cannot open pty: {e}"))?;

        // An allowlisted environment: what a shell needs, no host secrets.
        let mut cmd = CommandBuilder::new_default_prog();
        cmd.cwd(&cwd);
        cmd.env_clear();
        for (k, v) in terminal_env(std::env::vars_os(), cfg!(windows)) {
            cmd.env(k, v);
        }

        let child = pair
            .slave
            .spawn_command(cmd)
            .map_err(|e| format!("cannot spawn shell: {e}"))?;
        drop(pair.slave);

        let writer = pair
            .master
            .take_writer()
            .map_err(|e| format!("cannot take pty writer: {e}"))?;

        let mut id_guard = self.next_id.lock().map_err(|e| e.to_string())?;
        *id_guard += 1;
        let id = format!("term-{}", *id_guard);
        drop(id_guard);

        self.sessions.lock().map_err(|e| e.to_string())?.insert(
            id.clone(),
            TerminalSession {
                master: pair.master,
                child,
                writer,
            },
        );
        Ok(id)
    }

    pub fn write(&self, id: &str, data: &[u8]) -> Result<(), String> {
        let mut sessions = self.sessions.lock().map_err(|e| e.to_string())?;
        let session = sessions.get_mut(id).ok_or("unknown terminal")?;
        use std::io::Write;
        session
            .writer
            .write_all(data)
            .map_err(|e| format!("pty write failed: {e}"))?;
        session
            .writer
            .flush()
            .map_err(|e| format!("pty flush failed: {e}"))?;
        Ok(())
    }

    pub fn resize(&self, id: &str, cols: u16, rows: u16) -> Result<(), String> {
        let sessions = self.sessions.lock().map_err(|e| e.to_string())?;
        let session = sessions.get(id).ok_or("unknown terminal")?;
        session
            .master
            .resize(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|e| format!("pty resize failed: {e}"))?;
        Ok(())
    }

    pub fn kill(&self, id: &str) -> Result<(), String> {
        let mut sessions = self.sessions.lock().map_err(|e| e.to_string())?;
        if let Some(mut session) = sessions.remove(id) {
            let _ = session.child.kill();
            let _ = session.child.wait();
        }
        Ok(())
    }

    /// Clone the PTY reader for background draining. The Tauri layer spawns
    /// a thread that forwards bytes as `terminal-output-{id}` events.
    pub fn clone_reader(&self, id: &str) -> Result<Box<dyn Read + Send>, String> {
        let sessions = self.sessions.lock().map_err(|e| e.to_string())?;
        let session = sessions.get(id).ok_or("unknown terminal")?;
        session
            .master
            .try_clone_reader()
            .map_err(|e| format!("cannot clone pty reader: {e}"))
    }
}

impl Default for TerminalManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Shared handle for Tauri state.
pub type SharedTerminals = Arc<TerminalManager>;

#[cfg(test)]
mod tests {
    use super::*;

    fn vars(pairs: &[(&str, &str)]) -> Vec<(OsString, OsString)> {
        pairs
            .iter()
            .map(|(k, v)| (OsString::from(k), OsString::from(v)))
            .collect()
    }
    fn get(env: &[(OsString, OsString)], name: &str) -> Option<String> {
        env.iter()
            .find(|(k, _)| k.to_string_lossy() == name)
            .map(|(_, v)| v.to_string_lossy().to_string())
    }

    #[test]
    fn a_unix_shell_gets_what_it_needs_and_none_of_the_apps_secrets() {
        let env = terminal_env(
            vars(&[
                ("PATH", "/usr/bin"),
                ("HOME", "/home/a"),
                ("USER", "a"),
                ("SHELL", "/bin/zsh"),
                ("LANG", "de_DE.UTF-8"),
                ("SSH_AUTH_SOCK", "/tmp/agent.sock"),
                ("REX_ANTHROPIC_API_KEY", "sk-secret"),
                ("GITHUB_TOKEN", "ghp_secret"),
                ("AWS_SECRET_ACCESS_KEY", "aws-secret"),
                ("OPENAI_API_KEY", "sk-other"),
            ]),
            false,
        );
        assert_eq!(
            get(&env, "HOME").as_deref(),
            Some("/home/a"),
            "git and npm need HOME"
        );
        assert_eq!(get(&env, "SHELL").as_deref(), Some("/bin/zsh"));
        assert_eq!(
            get(&env, "LANG").as_deref(),
            Some("de_DE.UTF-8"),
            "the person's locale is kept"
        );
        assert_eq!(
            get(&env, "SSH_AUTH_SOCK").as_deref(),
            Some("/tmp/agent.sock")
        );
        assert_eq!(get(&env, "TERM").as_deref(), Some("xterm-256color"));
        for secret in [
            "REX_ANTHROPIC_API_KEY",
            "GITHUB_TOKEN",
            "AWS_SECRET_ACCESS_KEY",
            "OPENAI_API_KEY",
        ] {
            assert_eq!(get(&env, secret), None, "{secret} must not reach the shell");
        }
    }

    #[test]
    fn a_windows_shell_keeps_the_system_variables_it_cannot_start_without() {
        let env = terminal_env(
            vars(&[
                ("Path", r"C:\Windows\System32"),
                ("SystemRoot", r"C:\Windows"),
                ("ComSpec", r"C:\Windows\System32\cmd.exe"),
                ("USERPROFILE", r"C:\Users\a"),
                ("LOCALAPPDATA", r"C:\Users\a\AppData\Local"),
                ("TEMP", r"C:\Temp"),
                ("REX_OPENAI_API_KEY", "sk-secret"),
            ]),
            true,
        );
        // names are matched without regard to case on Windows, and kept as given
        for kept in [
            "Path",
            "SystemRoot",
            "ComSpec",
            "USERPROFILE",
            "LOCALAPPDATA",
            "TEMP",
        ] {
            assert!(get(&env, kept).is_some(), "{kept}");
        }
        assert_eq!(get(&env, "REX_OPENAI_API_KEY"), None);
        // the Unix-only defaults are not invented on Windows
        assert_eq!(get(&env, "LANG"), None);
    }

    #[test]
    fn a_dock_launched_app_with_no_locale_or_path_still_gets_a_utf8_shell() {
        let env = terminal_env(vars(&[("HOME", "/home/a")]), false);
        assert_eq!(get(&env, "LANG").as_deref(), Some("en_US.UTF-8"));
        assert!(get(&env, "PATH").unwrap().contains("/usr/bin"));
        // but an explicit LC_ALL is respected, not overridden by a default LANG
        let env = terminal_env(vars(&[("LC_ALL", "C.UTF-8")]), false);
        assert_eq!(get(&env, "LANG"), None);
    }

    #[test]
    fn output_split_in_the_middle_of_a_character_is_not_corrupted() {
        let mut s = Utf8Stream::default();
        let text = "naïve café — 日本語 🙂";
        let bytes = text.as_bytes();
        // feed it one byte at a time: every multi-byte character is split
        let mut out = String::new();
        for b in bytes {
            out.push_str(&s.push(std::slice::from_ref(b)));
        }
        out.push_str(&s.finish());
        assert_eq!(out, text);
        // what the old per-chunk lossy decode did to a split character
        assert_eq!(String::from_utf8_lossy(&bytes[..3]), "na\u{fffd}");
        let mut fresh = Utf8Stream::default();
        assert_eq!(
            fresh.push(&bytes[..3]),
            "na",
            "the half character waits for its other half"
        );
        assert_eq!(fresh.push(&bytes[3..4]), "ï");
    }

    #[test]
    fn invalid_bytes_become_one_replacement_each_and_decoding_carries_on() {
        let mut s = Utf8Stream::default();
        let out = s.push(&[b'a', 0xff, b'b', 0xc3, 0x28, b'c']);
        assert_eq!(out, "a\u{fffd}b\u{fffd}(c");
        // a character left incomplete when the stream ends is reported, not lost silently
        let mut s = Utf8Stream::default();
        assert_eq!(s.push(&[b'x', 0xe2, 0x82]), "x");
        assert_eq!(s.finish(), "\u{fffd}");
    }
}
