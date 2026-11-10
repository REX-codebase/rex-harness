//! Git for the Git tab and the status chip, through the person's own `git`.
//!
//! This is the person acting on their own repository, so it deliberately does
//! not run under the tool sandbox that confines the *model* (which replaces
//! `HOME` and would hide the identity and settings in their `~/.gitconfig`).
//! What it does keep from that discipline: no shell, a hard timeout, capped
//! output, no interactive prompts, and the folder is always one the registry
//! already vetted.

use serde::Serialize;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

const MAX_OUTPUT: usize = 4 * 1024 * 1024;
const MAX_DIFF: usize = 512 * 1024;
const MAX_FILES: usize = 1000;
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);
const MAX_MESSAGE: usize = 5000;
const PATH_CHUNK: usize = 200;

/// One changed path, as `git status --porcelain=v2` reports it.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct GitChange {
    pub path: String,
    /// The old path of a rename or copy.
    pub orig_path: Option<String>,
    /// Staged state: `.` unchanged, else `M` `A` `D` `R` `C` `T`.
    pub index: char,
    /// Working-tree state: `.` unchanged, else `M` `D` `T` (and `A` for intent-to-add).
    pub worktree: char,
    pub untracked: bool,
    pub conflicted: bool,
}

#[derive(Debug, Clone, Default, Serialize, PartialEq, Eq)]
pub struct GitOverview {
    /// `git` was found on this machine.
    pub git_available: bool,
    /// The folder is inside a git repository.
    pub is_repo: bool,
    /// The repository's top folder (may be above the opened folder).
    pub root: Option<String>,
    /// Current branch; `None` when detached or before the first commit has a name.
    pub branch: Option<String>,
    pub detached: bool,
    /// Short commit id of HEAD; `None` before the first commit.
    pub head: Option<String>,
    pub upstream: Option<String>,
    pub ahead: u32,
    pub behind: u32,
    pub staged: usize,
    pub unstaged: usize,
    pub untracked: usize,
    pub conflicted: usize,
    pub files: Vec<GitChange>,
    /// More than the listing limit changed; the counts are still exact.
    pub truncated: bool,
    /// Why there is no status, when git itself reported a problem.
    pub message: Option<String>,
}

impl GitOverview {
    /// Anything to commit or resolve.
    pub fn is_dirty(&self) -> bool {
        self.staged + self.unstaged + self.untracked + self.conflicted > 0
    }
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct CommitInfo {
    pub hash: String,
    pub subject: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct CommitSummary {
    pub hash: String,
    pub subject: String,
    pub author: String,
    pub when: String,
}

struct Output {
    code: Option<i32>,
    stdout: Vec<u8>,
    stderr: String,
}

impl Output {
    fn ok(&self) -> bool {
        self.code == Some(0)
    }
    fn text(&self) -> String {
        String::from_utf8_lossy(&self.stdout).to_string()
    }
}

/// A handle on the machine's `git`.
#[derive(Debug, Clone)]
pub struct Git {
    program: Option<PathBuf>,
    env: Vec<(String, String)>,
}

fn no_window(cmd: &mut Command) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // CREATE_NO_WINDOW: a console must not flash up on every status poll.
        cmd.creation_flags(0x0800_0000);
    }
    #[cfg(not(windows))]
    {
        let _ = cmd;
    }
}

fn find_git() -> Option<PathBuf> {
    static FOUND: OnceLock<Option<PathBuf>> = OnceLock::new();
    FOUND
        .get_or_init(|| {
            // A desktop app launched from the Dock or Start menu often has a
            // thinner PATH than a terminal, so look in the usual places too.
            let mut candidates: Vec<PathBuf> = vec![PathBuf::from("git")];
            for fixed in [
                "/usr/bin/git",
                "/usr/local/bin/git",
                "/opt/homebrew/bin/git",
                r"C:\Program Files\Git\cmd\git.exe",
                r"C:\Program Files (x86)\Git\cmd\git.exe",
            ] {
                candidates.push(PathBuf::from(fixed));
            }
            candidates.into_iter().find(|c| {
                let mut cmd = Command::new(c);
                cmd.arg("--version")
                    .stdin(Stdio::null())
                    .stdout(Stdio::null())
                    .stderr(Stdio::null());
                no_window(&mut cmd);
                cmd.status().map(|s| s.success()).unwrap_or(false)
            })
        })
        .clone()
}

impl Git {
    /// The machine's own git, with the person's own configuration.
    pub fn system() -> Self {
        Git {
            program: find_git(),
            env: Vec::new(),
        }
    }

    /// Like [`Git::system`] with extra environment variables (tests use this to
    /// give git an empty configuration).
    pub fn with_env(env: &[(&str, &str)]) -> Self {
        Git {
            program: find_git(),
            env: env
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
        }
    }

    /// A handle for a machine without git: every call answers with how to get it.
    pub fn unavailable() -> Self {
        Git {
            program: None,
            env: Vec::new(),
        }
    }

    pub fn available(&self) -> bool {
        self.program.is_some()
    }

    /// Paths git tracks plus those it does not ignore, relative to `ws`.
    /// `None` when git cannot answer (the caller falls back to walking).
    pub fn list_files_z(&self, ws: &Path) -> Option<Vec<String>> {
        let o = self
            .run(
                ws,
                &[
                    "ls-files",
                    "-z",
                    "--cached",
                    "--others",
                    "--exclude-standard",
                ],
                true,
            )
            .ok()?;
        if !o.ok() {
            return None;
        }
        Some(
            o.text()
                .split('\0')
                .filter(|p| !p.is_empty())
                .map(|p| p.to_string())
                .collect(),
        )
    }

    fn run(&self, ws: &Path, args: &[&str], read_only: bool) -> Result<Output, String> {
        self.run_with_timeout(ws, args, read_only, DEFAULT_TIMEOUT)
    }

    fn run_with_timeout(
        &self,
        ws: &Path,
        args: &[&str],
        read_only: bool,
        timeout: Duration,
    ) -> Result<Output, String> {
        let program = self.program.as_ref().ok_or_else(|| {
            "Git is not installed. Install it from https://git-scm.com and reopen REX.".to_string()
        })?;
        let mut cmd = Command::new(program);
        cmd.args(["-c", "core.quotepath=false"])
            .args(args)
            .current_dir(ws)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            // never wait on a password or editor prompt, never page output,
            // and keep messages in one language so they can be recognised
            .env("GIT_TERMINAL_PROMPT", "0")
            .env("GIT_PAGER", "cat")
            .env("GIT_EDITOR", "true")
            .env("LC_ALL", "C");
        if read_only {
            // a status poll must not take the index lock out from under the
            // person's own `git add` in the terminal
            cmd.env("GIT_OPTIONAL_LOCKS", "0");
        }
        for (k, v) in &self.env {
            cmd.env(k, v);
        }
        no_window(&mut cmd);
        let mut child = cmd.spawn().map_err(|e| format!("cannot run git: {e}"))?;

        fn drain(mut pipe: impl Read + Send + 'static) -> std::thread::JoinHandle<Vec<u8>> {
            std::thread::spawn(move || {
                let mut kept = Vec::new();
                let mut buf = [0u8; 16 * 1024];
                loop {
                    match pipe.read(&mut buf) {
                        Ok(0) | Err(_) => break,
                        Ok(n) => {
                            // keep draining past the cap so git never blocks on a full pipe
                            if kept.len() < MAX_OUTPUT {
                                let room = MAX_OUTPUT - kept.len();
                                kept.extend_from_slice(&buf[..n.min(room)]);
                            }
                        }
                    }
                }
                kept
            })
        }
        let out = child.stdout.take().map(drain);
        let err = child.stderr.take().map(drain);

        let start = Instant::now();
        let status = loop {
            match child.try_wait() {
                Ok(Some(status)) => break status,
                Ok(None) => {
                    if start.elapsed() > timeout {
                        let _ = child.kill();
                        let _ = child.wait();
                        return Err(format!(
                            "git took longer than {} seconds and was stopped",
                            timeout.as_secs()
                        ));
                    }
                    std::thread::sleep(Duration::from_millis(10));
                }
                Err(e) => return Err(format!("cannot wait for git: {e}")),
            }
        };
        let stdout = out.and_then(|h| h.join().ok()).unwrap_or_default();
        let stderr = err.and_then(|h| h.join().ok()).unwrap_or_default();
        Ok(Output {
            code: status.code(),
            stdout,
            stderr: String::from_utf8_lossy(&stderr).trim().to_string(),
        })
    }

    /// Status of the repository containing `ws`. Not being in a repository is a
    /// normal answer (`is_repo: false`), not an error.
    pub fn overview(&self, ws: &Path) -> GitOverview {
        if !self.available() {
            return GitOverview {
                message: Some(
                    "Git is not installed. Install it from https://git-scm.com and reopen REX."
                        .into(),
                ),
                ..GitOverview::default()
            };
        }
        let mut overview = GitOverview {
            git_available: true,
            ..GitOverview::default()
        };
        let top = match self.run(ws, &["rev-parse", "--show-toplevel"], true) {
            Ok(o) if o.ok() => o.text().trim().to_string(),
            Ok(o) => {
                if !o.stderr.contains("not a git repository") {
                    overview.message = Some(first_line(&o.stderr));
                }
                return overview;
            }
            Err(e) => {
                overview.message = Some(e);
                return overview;
            }
        };
        overview.is_repo = true;
        overview.root = Some(top);
        match self.run(
            ws,
            &[
                "status",
                "--porcelain=v2",
                "--branch",
                "-z",
                "--untracked-files=normal",
            ],
            true,
        ) {
            Ok(o) if o.ok() => parse_status(&o.text(), &mut overview),
            Ok(o) => overview.message = Some(first_line(&o.stderr)),
            Err(e) => overview.message = Some(e),
        }
        overview
    }

    /// `git init` in `ws`.
    pub fn init(&self, ws: &Path) -> Result<String, String> {
        let o = self.run(ws, &["init"], false)?;
        if o.ok() {
            Ok(first_line(&o.text()))
        } else {
            Err(failure("git init", &o))
        }
    }

    /// Stage the given paths (new, changed and deleted files alike).
    pub fn stage(&self, ws: &Path, paths: &[String]) -> Result<(), String> {
        self.pathspec_command(ws, &["add", "-A"], paths)
    }

    /// Stage everything.
    pub fn stage_all(&self, ws: &Path) -> Result<(), String> {
        let o = self.run(ws, &["add", "-A"], false)?;
        if o.ok() {
            Ok(())
        } else {
            Err(failure("git add", &o))
        }
    }

    /// Take the given paths out of the index, keeping the working-tree edits.
    pub fn unstage(&self, ws: &Path, paths: &[String]) -> Result<(), String> {
        self.pathspec_command(ws, &["reset", "-q"], paths)
    }

    fn pathspec_command(&self, ws: &Path, verb: &[&str], paths: &[String]) -> Result<(), String> {
        if paths.is_empty() {
            return Ok(());
        }
        for p in paths {
            check_relative(p)?;
        }
        // `--literal-pathspecs`: a file called `:(glob)*` is a file, not a pattern.
        for chunk in paths.chunks(PATH_CHUNK) {
            let mut args: Vec<&str> = vec!["--literal-pathspecs"];
            args.extend_from_slice(verb);
            args.push("--");
            args.extend(chunk.iter().map(|s| s.as_str()));
            let o = self.run(ws, &args, false)?;
            if !o.ok() {
                return Err(failure(&format!("git {}", verb[0]), &o));
            }
        }
        Ok(())
    }

    /// The diff for one path: staged changes when `staged`, else the
    /// working-tree changes. A file git does not track yet is shown as added.
    pub fn diff(&self, ws: &Path, path: &str, staged: bool) -> Result<String, String> {
        check_relative(path)?;
        if !staged && path.ends_with('/') {
            return Ok(untracked_folder_note(ws, path));
        }
        let mut args = vec!["--literal-pathspecs", "diff", "--no-color", "--no-ext-diff"];
        if staged {
            args.push("--cached");
        }
        args.push("--");
        args.push(path);
        let o = self.run(ws, &args, true)?;
        if !o.ok() {
            return Err(failure("git diff", &o));
        }
        let mut text = o.text();
        if text.is_empty() && !staged {
            let tracked = self.run(
                ws,
                &[
                    "--literal-pathspecs",
                    "ls-files",
                    "--error-unmatch",
                    "--",
                    path,
                ],
                true,
            )?;
            if !tracked.ok() {
                return Ok(untracked_file_diff(ws, path));
            }
        }
        if text.len() > MAX_DIFF {
            let mut cut = MAX_DIFF;
            while !text.is_char_boundary(cut) {
                cut -= 1;
            }
            text.truncate(cut);
            text.push_str("\n… diff truncated\n");
        }
        Ok(text)
    }

    /// Commit what is staged. The message is the person's own.
    pub fn commit(&self, ws: &Path, message: &str) -> Result<CommitInfo, String> {
        let message = message.trim();
        if message.is_empty() {
            return Err("Write a commit message first.".into());
        }
        if message.len() > MAX_MESSAGE {
            return Err(format!(
                "The commit message is longer than {MAX_MESSAGE} characters."
            ));
        }
        // exit 1 means there is something staged; 0 means there is not
        let staged = self.run(ws, &["diff", "--cached", "--quiet"], true)?;
        if staged.code == Some(0) {
            return Err("Nothing is staged. Stage the files you want in this commit first.".into());
        }
        let o = self.run(ws, &["commit", "-m", message], false)?;
        if !o.ok() {
            let all = format!("{}\n{}", o.stderr, o.text());
            if all.contains("Please tell me who you are")
                || all.contains("Author identity unknown")
                || all.contains("unable to auto-detect email address")
                || all.contains("empty ident name")
            {
                return Err("Git does not know who you are yet. In a terminal run:\n  git config --global user.name \"Your Name\"\n  git config --global user.email \"you@example.com\"\nthen commit again.".into());
            }
            return Err(failure("git commit", &o));
        }
        let hash = self
            .run(ws, &["rev-parse", "--short", "HEAD"], true)?
            .text()
            .trim()
            .to_string();
        let subject = self
            .run(ws, &["log", "-1", "--format=%s"], true)?
            .text()
            .trim()
            .to_string();
        Ok(CommitInfo { hash, subject })
    }

    /// The latest commits, newest first. A repository with none gives an empty list.
    pub fn log(&self, ws: &Path, limit: usize) -> Result<Vec<CommitSummary>, String> {
        let n = limit.clamp(1, 100).to_string();
        let o = self.run(
            ws,
            &["log", "-n", &n, "--format=%h%x1f%s%x1f%an%x1f%ar%x1e"],
            true,
        )?;
        if !o.ok() {
            if o.stderr.contains("does not have any commits yet")
                || o.stderr.contains("unknown revision")
            {
                return Ok(Vec::new());
            }
            return Err(failure("git log", &o));
        }
        Ok(o.text()
            .split('\u{1e}')
            .filter_map(|rec| {
                let rec = rec.trim_matches(|c| c == '\n' || c == '\r');
                let mut parts = rec.split('\u{1f}');
                let hash = parts.next()?.trim();
                if hash.is_empty() {
                    return None;
                }
                Some(CommitSummary {
                    hash: hash.to_string(),
                    subject: parts.next().unwrap_or("").to_string(),
                    author: parts.next().unwrap_or("").to_string(),
                    when: parts.next().unwrap_or("").to_string(),
                })
            })
            .collect())
    }
}

fn first_line(text: &str) -> String {
    text.lines()
        .find(|l| !l.trim().is_empty())
        .unwrap_or("git reported an error")
        .trim()
        .to_string()
}

fn failure(what: &str, o: &Output) -> String {
    let detail = if o.stderr.is_empty() {
        o.text()
    } else {
        o.stderr.clone()
    };
    let detail: Vec<&str> = detail.lines().filter(|l| !l.trim().is_empty()).collect();
    let tail = detail[detail.len().saturating_sub(12)..].join("\n");
    if tail.is_empty() {
        format!("{what} failed")
    } else {
        format!("{what} failed:\n{tail}")
    }
}

/// A path relative to the opened folder, with no way out of it and nothing git
/// could read as an option.
pub fn check_relative(path: &str) -> Result<(), String> {
    if path.is_empty() || path.contains('\0') {
        return Err("that path is not valid".into());
    }
    if has_drive_prefix(path) {
        return Err("path must be relative to the opened folder".into());
    }
    let p = Path::new(path);
    if p.is_absolute() || path.starts_with('/') || path.starts_with('\\') || path.starts_with('-') {
        return Err("path must be relative to the opened folder".into());
    }
    if path.split(['/', '\\']).any(|c| c == "..") {
        return Err("path escapes the opened folder".into());
    }
    Ok(())
}

/// `C:` or `C:\...`: a drive on Windows, and nothing a path inside an opened
/// folder should ever start with, on any platform.
pub fn has_drive_prefix(path: &str) -> bool {
    let b = path.as_bytes();
    b.len() >= 2 && b[0].is_ascii_alphabetic() && b[1] == b':'
}

fn untracked_folder_note(ws: &Path, path: &str) -> String {
    let dir = ws.join(path.trim_end_matches('/'));
    let mut names: Vec<String> = Vec::new();
    let mut stack = vec![dir.clone()];
    while let Some(d) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&d) else {
            continue;
        };
        for e in entries.flatten() {
            let Ok(t) = e.file_type() else { continue };
            if t.is_dir() {
                if e.file_name() != ".git" {
                    stack.push(e.path());
                }
            } else if t.is_file() {
                if let Ok(rel) = e.path().strip_prefix(ws) {
                    names.push(rel.to_string_lossy().replace('\\', "/"));
                }
            }
            if names.len() >= 200 {
                break;
            }
        }
        if names.len() >= 200 {
            break;
        }
    }
    names.sort();
    let more = if names.len() >= 200 {
        "\n… and more"
    } else {
        ""
    };
    format!(
        "Untracked folder: {path}\n{} file(s) not yet tracked by git:\n{}{more}\n",
        names.len(),
        names.join("\n")
    )
}

fn untracked_file_diff(ws: &Path, path: &str) -> String {
    let file = ws.join(path);
    let Ok(meta) = std::fs::metadata(&file) else {
        return String::new();
    };
    if !meta.is_file() {
        return String::new();
    }
    if meta.len() as usize > MAX_DIFF {
        return format!(
            "New file {path} ({} bytes) is too large to preview.\n",
            meta.len()
        );
    }
    let Ok(bytes) = std::fs::read(&file) else {
        return String::new();
    };
    if bytes.iter().take(8000).any(|b| *b == 0) {
        return format!("New binary file {path} ({} bytes).\n", bytes.len());
    }
    let text = String::from_utf8_lossy(&bytes);
    let lines: Vec<&str> = text.lines().collect();
    let mut out = format!(
        "diff --git a/{path} b/{path}\nnew file\n--- /dev/null\n+++ b/{path}\n@@ -0,0 +1,{} @@\n",
        lines.len()
    );
    for l in &lines {
        out.push('+');
        out.push_str(l);
        out.push('\n');
    }
    out
}

/// Fill `overview` from `git status --porcelain=v2 --branch -z` output.
pub fn parse_status(raw: &str, overview: &mut GitOverview) {
    let mut tokens = raw.split('\0').peekable();
    let mut oid_known = false;
    while let Some(tok) = tokens.next() {
        if tok.is_empty() {
            continue;
        }
        if let Some(rest) = tok.strip_prefix("# ") {
            let mut it = rest.splitn(2, ' ');
            match (it.next(), it.next()) {
                (Some("branch.oid"), Some(v)) => {
                    if v != "(initial)" {
                        oid_known = true;
                        overview.head = Some(v.chars().take(7).collect());
                    }
                }
                (Some("branch.head"), Some(v)) => {
                    if v == "(detached)" {
                        overview.detached = true;
                    } else {
                        overview.branch = Some(v.to_string());
                    }
                }
                (Some("branch.upstream"), Some(v)) => overview.upstream = Some(v.to_string()),
                (Some("branch.ab"), Some(v)) => {
                    for part in v.split_whitespace() {
                        if let Some(n) = part.strip_prefix('+') {
                            overview.ahead = n.parse().unwrap_or(0);
                        } else if let Some(n) = part.strip_prefix('-') {
                            overview.behind = n.parse().unwrap_or(0);
                        }
                    }
                }
                _ => {}
            }
            continue;
        }
        let kind = tok.as_bytes()[0];
        let change = match kind {
            b'1' => {
                // 1 XY sub mH mI mW hH hI path
                let f: Vec<&str> = tok.splitn(9, ' ').collect();
                if f.len() < 9 {
                    continue;
                }
                let (i, w) = xy(f[1]);
                Some(GitChange {
                    path: f[8].to_string(),
                    orig_path: None,
                    index: i,
                    worktree: w,
                    untracked: false,
                    conflicted: false,
                })
            }
            b'2' => {
                // 2 XY sub mH mI mW hH hI Xscore path <NUL> origPath
                let f: Vec<&str> = tok.splitn(10, ' ').collect();
                if f.len() < 10 {
                    continue;
                }
                let orig = tokens.next().map(|s| s.to_string());
                let (i, w) = xy(f[1]);
                Some(GitChange {
                    path: f[9].to_string(),
                    orig_path: orig,
                    index: i,
                    worktree: w,
                    untracked: false,
                    conflicted: false,
                })
            }
            b'u' => {
                // u XY sub m1 m2 m3 mW h1 h2 h3 path
                let f: Vec<&str> = tok.splitn(11, ' ').collect();
                if f.len() < 11 {
                    continue;
                }
                let (i, w) = xy(f[1]);
                Some(GitChange {
                    path: f[10].to_string(),
                    orig_path: None,
                    index: i,
                    worktree: w,
                    untracked: false,
                    conflicted: true,
                })
            }
            b'?' => Some(GitChange {
                path: tok[2..].to_string(),
                orig_path: None,
                index: '.',
                worktree: '.',
                untracked: true,
                conflicted: false,
            }),
            _ => None, // ignored files ("!") and anything newer
        };
        if let Some(c) = change {
            if c.conflicted {
                overview.conflicted += 1;
            } else if c.untracked {
                overview.untracked += 1;
            } else {
                if c.index != '.' {
                    overview.staged += 1;
                }
                if c.worktree != '.' {
                    overview.unstaged += 1;
                }
            }
            if overview.files.len() < MAX_FILES {
                overview.files.push(c);
            } else {
                overview.truncated = true;
            }
        }
    }
    if !oid_known {
        overview.head = None;
    }
}

fn xy(s: &str) -> (char, char) {
    let mut c = s.chars();
    (c.next().unwrap_or('.'), c.next().unwrap_or('.'))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn parse(raw: &str) -> GitOverview {
        let mut o = GitOverview::default();
        parse_status(raw, &mut o);
        o
    }

    #[test]
    fn parses_branch_upstream_and_ahead_behind() {
        let o = parse("# branch.oid 0123456789abcdef\0# branch.head main\0# branch.upstream origin/main\0# branch.ab +3 -1\0");
        assert_eq!(o.branch.as_deref(), Some("main"));
        assert_eq!(o.head.as_deref(), Some("0123456"));
        assert_eq!(o.upstream.as_deref(), Some("origin/main"));
        assert_eq!((o.ahead, o.behind), (3, 1));
        assert!(!o.detached && !o.is_dirty());
    }

    #[test]
    fn a_repository_before_its_first_commit_has_a_branch_but_no_head() {
        let o = parse("# branch.oid (initial)\0# branch.head main\0");
        assert_eq!(o.branch.as_deref(), Some("main"));
        assert_eq!(o.head, None);
    }

    #[test]
    fn a_detached_head_has_no_branch() {
        let o = parse("# branch.oid abcdef0123\0# branch.head (detached)\0");
        assert!(o.detached);
        assert_eq!(o.branch, None);
        assert_eq!(o.head.as_deref(), Some("abcdef0"));
    }

    #[test]
    fn counts_staged_unstaged_untracked_and_conflicted_separately() {
        let raw = "# branch.head main\0\
            1 M. N... 100644 100644 100644 aaaa bbbb staged.txt\0\
            1 .M N... 100644 100644 100644 aaaa bbbb edited.txt\0\
            1 MM N... 100644 100644 100644 aaaa bbbb both.txt\0\
            1 .D N... 100644 100644 000000 aaaa bbbb gone.txt\0\
            u UU N... 100644 100644 100644 100644 aaaa bbbb cccc clash.txt\0\
            ? new file.txt\0\
            ? dir/\0\
            ! ignored.log\0";
        let o = parse(raw);
        // staged: staged.txt + both.txt; unstaged: edited + both + gone
        assert_eq!(
            (o.staged, o.unstaged, o.untracked, o.conflicted),
            (2, 3, 2, 1)
        );
        assert!(o.is_dirty());
        assert_eq!(o.files.len(), 7, "the ignored file is not listed");
        let new = o.files.iter().find(|c| c.path == "new file.txt").unwrap();
        assert!(
            new.untracked && new.index == '.',
            "paths with spaces survive"
        );
        let both = o.files.iter().find(|c| c.path == "both.txt").unwrap();
        assert_eq!((both.index, both.worktree), ('M', 'M'));
        assert!(
            o.files
                .iter()
                .find(|c| c.path == "clash.txt")
                .unwrap()
                .conflicted
        );
    }

    #[test]
    fn a_rename_carries_its_old_path() {
        let o = parse(
            "# branch.head main\0\
            2 R. N... 100644 100644 100644 aaaa bbbb R100 new name.txt\0old name.txt\0\
            1 .M N... 100644 100644 100644 aaaa bbbb after.txt\0",
        );
        assert_eq!(o.files.len(), 2);
        assert_eq!(o.files[0].path, "new name.txt");
        assert_eq!(o.files[0].orig_path.as_deref(), Some("old name.txt"));
        assert_eq!(o.files[0].index, 'R');
        assert_eq!(
            o.files[1].path, "after.txt",
            "the old path was consumed, not read as an entry"
        );
    }

    #[test]
    fn a_huge_change_list_is_capped_but_still_counted() {
        let mut raw = String::from("# branch.head main\0");
        for i in 0..(MAX_FILES + 50) {
            raw.push_str(&format!("? f{i}.txt\0"));
        }
        let o = parse(&raw);
        assert_eq!(o.untracked, MAX_FILES + 50);
        assert_eq!(o.files.len(), MAX_FILES);
        assert!(o.truncated);
    }

    #[test]
    fn paths_that_could_escape_or_act_as_options_are_refused() {
        for bad in [
            "",
            "/etc/passwd",
            "../x",
            "a/../../x",
            "-rf",
            "\\\\server\\share",
            "a\0b",
            "C:\\x\\..\\y",
            "C:\\Windows\\x",
            "c:rel",
        ] {
            assert!(check_relative(bad).is_err(), "{bad:?}");
        }
        for good in ["a.txt", "dir/a b.txt", "dir/", "a..b.txt", ".hidden"] {
            assert!(check_relative(good).is_ok(), "{good:?}");
        }
    }

    // ---- against a real git, in a throwaway repository -------------------

    fn have_git() -> bool {
        Git::system().available()
    }

    /// A git that ignores the machine's configuration and knows one identity.
    fn isolated(home: &Path) -> Git {
        Git::with_env(&[
            ("HOME", home.to_str().unwrap()),
            ("XDG_CONFIG_HOME", home.to_str().unwrap()),
            ("GIT_CONFIG_NOSYSTEM", "1"),
            (
                "GIT_CONFIG_GLOBAL",
                home.join(".gitconfig").to_str().unwrap(),
            ),
        ])
    }

    fn repo() -> (tempfile::TempDir, PathBuf, Git) {
        let tmp = tempfile::tempdir().unwrap();
        let ws = tmp.path().join("proj");
        fs::create_dir_all(&ws).unwrap();
        let home = tmp.path().join("home");
        fs::create_dir_all(&home).unwrap();
        let git = isolated(&home);
        assert!(git.init(&ws).is_ok());
        let cfg = |k: &str, v: &str| {
            let o = git.run(&ws, &["config", k, v], false).unwrap();
            assert!(o.ok(), "{}", o.stderr);
        };
        cfg("user.name", "Test User");
        cfg("user.email", "test@example.com");
        cfg("commit.gpgsign", "false");
        (tmp, ws, git)
    }

    #[test]
    fn not_a_repository_is_a_normal_answer() {
        if !have_git() {
            return;
        }
        let tmp = tempfile::tempdir().unwrap();
        // keep git from finding a repository in a parent of the temp folder
        let git = Git::with_env(&[(
            "GIT_CEILING_DIRECTORIES",
            tmp.path().parent().unwrap().to_str().unwrap(),
        )]);
        let o = git.overview(tmp.path());
        assert!(o.git_available && !o.is_repo);
        assert_eq!(o.message, None, "this is not an error");
    }

    #[test]
    fn a_new_repository_walks_through_stage_commit_and_clean() {
        if !have_git() {
            return;
        }
        let (_tmp, ws, git) = repo();
        let empty = git.overview(&ws);
        assert!(empty.is_repo && !empty.is_dirty());
        assert_eq!(empty.head, None);
        assert!(
            git.log(&ws, 5).unwrap().is_empty(),
            "no commits yet is an empty list, not an error"
        );

        fs::write(ws.join("a.txt"), "one\n").unwrap();
        fs::write(ws.join("b b.txt"), "two\n").unwrap();
        let o = git.overview(&ws);
        assert_eq!((o.untracked, o.staged, o.unstaged), (2, 0, 0));

        // nothing staged yet: the commit says so instead of failing obscurely
        let err = git.commit(&ws, "first").unwrap_err();
        assert!(err.contains("Nothing is staged"), "{err}");

        git.stage(&ws, &["a.txt".to_string()]).unwrap();
        let o = git.overview(&ws);
        assert_eq!((o.staged, o.untracked), (1, 1));
        assert_eq!(
            o.files.iter().find(|c| c.path == "a.txt").unwrap().index,
            'A'
        );

        let c = git.commit(&ws, "  add a  ").unwrap();
        assert_eq!(c.subject, "add a");
        assert!(!c.hash.is_empty());
        let o = git.overview(&ws);
        assert_eq!(
            (o.staged, o.untracked),
            (0, 1),
            "b b.txt is still untracked"
        );
        assert!(o.head.is_some());
        assert_eq!(git.log(&ws, 5).unwrap()[0].subject, "add a");

        git.stage_all(&ws).unwrap();
        git.commit(&ws, "add b").unwrap();
        assert!(!git.overview(&ws).is_dirty());
    }

    #[test]
    fn unstaging_keeps_the_edit_and_works_before_the_first_commit() {
        if !have_git() {
            return;
        }
        let (_tmp, ws, git) = repo();
        fs::write(ws.join("a.txt"), "one\n").unwrap();
        git.stage(&ws, &["a.txt".to_string()]).unwrap();
        git.unstage(&ws, &["a.txt".to_string()]).unwrap();
        let o = git.overview(&ws);
        assert_eq!((o.staged, o.untracked), (0, 1));
        assert!(ws.join("a.txt").exists(), "the file itself is untouched");

        git.stage_all(&ws).unwrap();
        git.commit(&ws, "base").unwrap();
        fs::write(ws.join("a.txt"), "two\n").unwrap();
        git.stage(&ws, &["a.txt".to_string()]).unwrap();
        git.unstage(&ws, &["a.txt".to_string()]).unwrap();
        let o = git.overview(&ws);
        assert_eq!((o.staged, o.unstaged), (0, 1));
        assert_eq!(fs::read_to_string(ws.join("a.txt")).unwrap(), "two\n");
    }

    #[test]
    fn diffs_cover_staged_unstaged_untracked_and_folders() {
        if !have_git() {
            return;
        }
        let (_tmp, ws, git) = repo();
        fs::write(ws.join("a.txt"), "one\n").unwrap();
        git.stage_all(&ws).unwrap();
        git.commit(&ws, "base").unwrap();

        fs::write(ws.join("a.txt"), "one\ntwo\n").unwrap();
        let unstaged = git.diff(&ws, "a.txt", false).unwrap();
        assert!(unstaged.contains("+two"), "{unstaged}");
        assert_eq!(git.diff(&ws, "a.txt", true).unwrap(), "");
        git.stage(&ws, &["a.txt".to_string()]).unwrap();
        assert!(git.diff(&ws, "a.txt", true).unwrap().contains("+two"));

        // a file git has not seen is shown as wholly added
        fs::write(ws.join("new.txt"), "x\ny\n").unwrap();
        let added = git.diff(&ws, "new.txt", false).unwrap();
        assert!(
            added.contains("--- /dev/null") && added.contains("+x") && added.contains("+y"),
            "{added}"
        );
        fs::write(ws.join("bin.dat"), [0u8, 1, 2, 3]).unwrap();
        assert!(git.diff(&ws, "bin.dat", false).unwrap().contains("binary"));

        // an untracked folder is listed rather than diffed
        fs::create_dir_all(ws.join("pkg")).unwrap();
        fs::write(ws.join("pkg").join("m.txt"), "m").unwrap();
        let folder = git.diff(&ws, "pkg/", false).unwrap();
        assert!(folder.contains("pkg/m.txt"), "{folder}");

        assert!(git.diff(&ws, "../x", false).is_err());
    }

    #[test]
    fn a_missing_identity_is_explained_with_the_fix() {
        if !have_git() {
            return;
        }
        let tmp = tempfile::tempdir().unwrap();
        let ws = tmp.path().join("proj");
        let home = tmp.path().join("home");
        fs::create_dir_all(&ws).unwrap();
        fs::create_dir_all(&home).unwrap();
        // no user.name / user.email anywhere, and no way to guess them
        let git = Git::with_env(&[
            ("HOME", home.to_str().unwrap()),
            ("XDG_CONFIG_HOME", home.to_str().unwrap()),
            ("GIT_CONFIG_NOSYSTEM", "1"),
            (
                "GIT_CONFIG_GLOBAL",
                home.join(".gitconfig").to_str().unwrap(),
            ),
            ("EMAIL", ""),
            ("GIT_AUTHOR_NAME", ""),
            ("GIT_COMMITTER_NAME", ""),
            ("GIT_AUTHOR_EMAIL", ""),
            ("GIT_COMMITTER_EMAIL", ""),
        ]);
        git.init(&ws).unwrap();
        fs::write(ws.join("a.txt"), "x").unwrap();
        git.stage_all(&ws).unwrap();
        let err = git.commit(&ws, "hello").unwrap_err();
        assert!(err.contains("git config --global user.name"), "{err}");
    }

    #[test]
    fn a_hung_command_is_stopped_and_reported() {
        if !have_git() {
            return;
        }
        let tmp = tempfile::tempdir().unwrap();
        let git = Git::system();
        // `git help -a` pages nothing here; instead prove the timeout path with a tiny budget
        let err = git
            .run_with_timeout(
                tmp.path(),
                &["-c", "alias.slow=!sleep 5", "slow"],
                true,
                Duration::from_millis(200),
            )
            .map(|o| o.code);
        // on Windows `sleep` may not exist, in which case git fails fast: both are fine, hanging is not
        match err {
            Err(e) => assert!(e.contains("was stopped"), "{e}"),
            Ok(code) => assert_ne!(code, Some(0)),
        }
    }

    #[test]
    fn long_commit_messages_and_empty_ones_are_refused_before_git_runs() {
        let git = Git::unavailable();
        let tmp = tempfile::tempdir().unwrap();
        assert!(git
            .commit(tmp.path(), "   ")
            .unwrap_err()
            .contains("Write a commit message"));
        assert!(git
            .commit(tmp.path(), &"x".repeat(MAX_MESSAGE + 1))
            .unwrap_err()
            .contains("longer than"));
    }

    #[test]
    fn without_git_every_call_says_how_to_get_it() {
        let git = Git::unavailable();
        let tmp = tempfile::tempdir().unwrap();
        let o = git.overview(tmp.path());
        assert!(!o.git_available && !o.is_repo);
        assert!(o.message.unwrap().contains("git-scm.com"));
        assert!(git
            .stage_all(tmp.path())
            .unwrap_err()
            .contains("not installed"));
    }
}
