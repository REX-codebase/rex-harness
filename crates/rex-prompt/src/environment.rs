//! The environment a run actually executes in: the operating system, how
//! commands are run and limited, which programs are installed, and which
//! kind of project the workspace holds.
//!
//! A model that is not told these facts learns them by failing: it writes a
//! `&&` chain for a command that takes an argv, reaches for `python` where
//! interpreters are blocked, or assumes `make` exists on Windows. The
//! facts are observed from the host here and rendered into the per-turn
//! state (never the system prompt), so they cannot change the prompt
//! identity; the system prompt carries the rules that are the same on every
//! machine, how commands work and how to use this block (see
//! `tools::render_contract`).
//!
//! Nothing in this module is a guess: the OS and architecture come from the
//! build target, programs are looked up on `PATH` the way the command runner
//! looks them up (including `PATHEXT` on Windows), project markers are files
//! that exist at the workspace root, and the limits are passed in by the
//! caller from the constants the tool runtime enforces.

use serde_json::{json, Value};
use std::ffi::{OsStr, OsString};
use std::path::Path;

/// Programs a model might legitimately run through `run_command`. Interpreters
/// and shells are left out on purpose: the runtime blocks them, and listing
/// them as available would invite the call.
pub const PROBED_PROGRAMS: &[&str] = &[
    "cargo", "rustc", "npm", "pnpm", "yarn", "make", "git", "go", "pytest", "pip", "dotnet", "mvn",
    "gradle",
];

/// Files at the workspace root that say what kind of project it is.
pub const PROJECT_MARKERS: &[&str] = &[
    "Cargo.toml",
    "package.json",
    "pnpm-lock.yaml",
    "yarn.lock",
    "pyproject.toml",
    "requirements.txt",
    "go.mod",
    "pom.xml",
    "build.gradle",
    "Makefile",
    "CMakeLists.txt",
    "tsconfig.json",
];

/// Program categories the command runner refuses, with examples that the
/// runtime's own policy is tested against (see `rex-providers`: the test
/// `environment_examples_match_the_command_policy`).
pub const BLOCKED_EXAMPLES: &[(&str, &[&str])] = &[
    ("shells", &["sh", "bash", "cmd", "powershell"]),
    (
        "interpreters",
        &["python", "python3", "node", "ruby", "perl"],
    ),
    ("network clients", &["curl", "wget", "ssh"]),
    ("launchers", &["env", "xargs", "find", "timeout", "nohup"]),
    ("destructive or privileged", &["rm", "sudo", "kill"]),
];

/// What one command is held to. `None` means the platform does not enforce it
/// (the address-space and file-descriptor limits are Unix-only).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CommandLimits {
    pub timeout_secs: u64,
    pub output_kib: u64,
    pub memory_gib: Option<u64>,
    pub open_files: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Environment {
    pub os: &'static str,
    pub arch: &'static str,
    /// `unix` or `windows`.
    pub family: &'static str,
    pub programs_found: Vec<String>,
    pub programs_missing: Vec<String>,
    pub project_files: Vec<String>,
    pub limits: CommandLimits,
}

/// Observe the host for a run in `workspace`.
pub fn probe(workspace: &Path, limits: CommandLimits) -> Environment {
    probe_with(
        workspace,
        std::env::var_os("PATH"),
        std::env::var_os("PATHEXT"),
        limits,
    )
}

/// `probe` with the environment passed in, so tests need not touch the
/// process environment.
pub fn probe_with(
    workspace: &Path,
    path: Option<OsString>,
    pathext: Option<OsString>,
    limits: CommandLimits,
) -> Environment {
    let dirs: Vec<_> = path
        .as_deref()
        .map(|p| std::env::split_paths(p).collect())
        .unwrap_or_default();
    let exts = executable_extensions(pathext.as_deref());
    let (mut found, mut missing) = (Vec::new(), Vec::new());
    for program in PROBED_PROGRAMS {
        if on_path(program, &dirs, &exts) {
            found.push((*program).to_string());
        } else {
            missing.push((*program).to_string());
        }
    }
    let project_files = PROJECT_MARKERS
        .iter()
        .filter(|name| workspace.join(name).is_file())
        .map(|name| (*name).to_string())
        .collect();
    Environment {
        os: std::env::consts::OS,
        arch: std::env::consts::ARCH,
        family: std::env::consts::FAMILY,
        programs_found: found,
        programs_missing: missing,
        project_files,
        limits,
    }
}

/// Extensions tried after a bare program name. Only Windows has them; on
/// Unix the bare name is the file.
fn executable_extensions(pathext: Option<&OsStr>) -> Vec<String> {
    if !cfg!(windows) {
        return Vec::new();
    }
    let raw = pathext
        .and_then(OsStr::to_str)
        .filter(|s| !s.is_empty())
        .unwrap_or(".COM;.EXE;.BAT;.CMD");
    raw.split(';')
        .filter(|e| !e.is_empty())
        .map(|e| e.to_ascii_lowercase())
        .collect()
}

fn on_path(program: &str, dirs: &[std::path::PathBuf], exts: &[String]) -> bool {
    dirs.iter().any(|dir| {
        dir.join(program).is_file()
            || exts
                .iter()
                .any(|ext| dir.join(format!("{program}{ext}")).is_file())
    })
}

impl Environment {
    /// The per-turn state block: only what differs between machines and
    /// workspaces. How commands work is the same everywhere, so it is stated
    /// once in the system prompt (`tools::render_contract`), not re-sent here
    /// every turn. `lean` is the compact form for small context windows.
    pub fn to_value(&self, lean: bool) -> Value {
        if lean {
            return json!({
                "os": self.os,
                "arch": self.arch,
                "programs_on_path": self.programs_found,
                "workspace_files": self.project_files,
            });
        }
        let blocked: Vec<String> = BLOCKED_EXAMPLES
            .iter()
            .map(|(kind, names)| format!("{kind} ({})", names.join(", ")))
            .collect();
        let mut limits = serde_json::Map::new();
        limits.insert("timeout_secs".into(), json!(self.limits.timeout_secs));
        limits.insert("output_kib".into(), json!(self.limits.output_kib));
        if let Some(gib) = self.limits.memory_gib {
            limits.insert("memory_gib".into(), json!(gib));
        }
        if let Some(files) = self.limits.open_files {
            limits.insert("open_files".into(), json!(files));
        }
        json!({
            "os": self.os,
            "arch": self.arch,
            "family": self.family,
            "path_separator": std::path::MAIN_SEPARATOR.to_string(),
            "command_limits": Value::Object(limits),
            "blocked_commands": blocked,
            "programs_on_path": self.programs_found,
            "programs_not_found": self.programs_missing,
            "workspace_files": self.project_files,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    const LIMITS: CommandLimits = CommandLimits {
        timeout_secs: 120,
        output_kib: 512,
        memory_gib: Some(4),
        open_files: Some(256),
    };

    fn temp(tag: &str) -> std::path::PathBuf {
        let p = std::env::temp_dir().join(format!(
            "rex-env-{tag}-{}-{}",
            std::process::id(),
            &crate::sha256_hex(tag.as_bytes())[..8]
        ));
        let _ = fs::remove_dir_all(&p);
        fs::create_dir_all(&p).unwrap();
        p
    }

    #[test]
    fn programs_are_split_into_found_and_missing_by_what_is_on_path() {
        let bin = temp("bin");
        let name = if cfg!(windows) { "cargo.exe" } else { "cargo" };
        fs::write(bin.join(name), "").unwrap();
        let ws = temp("ws-programs");
        let env = probe_with(&ws, Some(bin.clone().into_os_string()), None, LIMITS);
        assert_eq!(env.programs_found, ["cargo"]);
        assert!(env.programs_missing.contains(&"npm".to_string()));
        assert_eq!(
            env.programs_found.len() + env.programs_missing.len(),
            PROBED_PROGRAMS.len()
        );
    }

    #[test]
    fn no_path_means_nothing_is_found() {
        let env = probe_with(&temp("ws-nopath"), None, None, LIMITS);
        assert!(env.programs_found.is_empty());
        assert_eq!(env.programs_missing.len(), PROBED_PROGRAMS.len());
    }

    #[test]
    fn workspace_markers_are_the_files_that_exist_at_its_root() {
        let ws = temp("ws-markers");
        fs::write(ws.join("Cargo.toml"), "").unwrap();
        fs::write(ws.join("package.json"), "").unwrap();
        // a marker inside a folder, or a folder with a marker's name, is not one
        fs::create_dir_all(ws.join("sub")).unwrap();
        fs::write(ws.join("sub").join("go.mod"), "").unwrap();
        fs::create_dir_all(ws.join("Makefile")).unwrap();
        let env = probe_with(&ws, None, None, LIMITS);
        assert_eq!(env.project_files, ["Cargo.toml", "package.json"]);
    }

    #[test]
    fn host_facts_come_from_the_build_target() {
        let env = probe_with(&temp("ws-host"), None, None, LIMITS);
        assert_eq!(env.os, std::env::consts::OS);
        assert_eq!(env.family, std::env::consts::FAMILY);
        assert!(!env.arch.is_empty());
    }

    #[test]
    fn the_full_block_states_the_command_rules_and_the_real_limits() {
        let env = probe_with(&temp("ws-full"), None, None, LIMITS);
        let v = env.to_value(false);
        assert_eq!(v["os"], std::env::consts::OS);
        assert_eq!(v["command_limits"]["timeout_secs"], 120);
        assert_eq!(v["command_limits"]["memory_gib"], 4);
        assert_eq!(v["path_separator"], std::path::MAIN_SEPARATOR.to_string());
        let blocked = v["blocked_commands"].to_string();
        for kind in ["shells", "interpreters", "network clients"] {
            assert!(blocked.contains(kind), "{kind} missing from {blocked}");
        }
    }

    #[test]
    fn a_limit_the_platform_does_not_enforce_is_not_claimed() {
        let limits = CommandLimits {
            memory_gib: None,
            open_files: None,
            ..LIMITS
        };
        let v = probe_with(&temp("ws-nolimit"), None, None, limits).to_value(false);
        let l = &v["command_limits"];
        assert!(l.get("memory_gib").is_none() && l.get("open_files").is_none());
        assert_eq!(l["timeout_secs"], 120);
    }

    #[test]
    fn the_lean_block_keeps_the_facts_and_drops_the_prose() {
        let env = probe_with(&temp("ws-lean"), None, None, LIMITS);
        let lean = env.to_value(true).to_string();
        let full = env.to_value(false).to_string();
        assert!(lean.len() < full.len(), "{} vs {}", lean.len(), full.len());
        assert!(lean.contains(std::env::consts::OS) && lean.contains("programs_on_path"));
        assert!(!lean.contains("blocked_commands") && !lean.contains("command_limits"));
    }

    #[test]
    fn the_probed_programs_are_never_ones_the_runtime_blocks() {
        for (_, names) in BLOCKED_EXAMPLES {
            for name in *names {
                assert!(
                    !PROBED_PROGRAMS.contains(name),
                    "{name} is blocked but listed as available"
                );
            }
        }
    }
}
