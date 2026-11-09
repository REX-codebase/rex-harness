//! Keeps the documentation honest: links resolve, and the CLI reference covers
//! every command the binary says it has.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::process::Command;

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn markdown_files() -> Vec<PathBuf> {
    let root = repo();
    let mut files: Vec<PathBuf> = [
        "README.md",
        "INSTALL.md",
        "CONTRIBUTING.md",
        "SECURITY.md",
        "CHANGELOG.md",
    ]
    .iter()
    .map(|f| root.join(f))
    .collect();
    for entry in std::fs::read_dir(root.join("docs")).unwrap().flatten() {
        let p = entry.path();
        if p.extension().is_some_and(|e| e == "md") {
            files.push(p);
        }
    }
    files
}

/// GitHub's heading anchor: lower-case, drop punctuation, spaces to hyphens.
fn slug(heading: &str) -> String {
    heading
        .trim()
        .to_lowercase()
        .chars()
        .filter(|c| c.is_alphanumeric() || *c == ' ' || *c == '-' || *c == '_')
        .collect::<String>()
        .replace(' ', "-")
}

fn anchors(text: &str) -> HashSet<String> {
    let mut in_fence = false;
    let mut out = HashSet::new();
    for line in text.lines() {
        if line.trim_start().starts_with("```") {
            in_fence = !in_fence;
            continue;
        }
        if !in_fence {
            if let Some(h) = line.strip_prefix('#') {
                out.insert(slug(h.trim_start_matches('#')));
            }
        }
    }
    out
}

/// `](target)` links outside code fences and inline code.
fn links(text: &str) -> Vec<String> {
    let mut in_fence = false;
    let mut out = Vec::new();
    for line in text.lines() {
        if line.trim_start().starts_with("```") {
            in_fence = !in_fence;
            continue;
        }
        if in_fence {
            continue;
        }
        let mut rest = line;
        while let Some(at) = rest.find("](") {
            let after = &rest[at + 2..];
            if let Some(end) = after.find(')') {
                out.push(after[..end].trim().to_string());
                rest = &after[end..];
            } else {
                break;
            }
        }
    }
    out
}

#[test]
fn every_relative_link_and_anchor_in_the_docs_resolves() {
    let mut broken = Vec::new();
    for file in markdown_files() {
        let text = std::fs::read_to_string(&file).unwrap();
        let dir = file.parent().unwrap();
        for target in links(&text) {
            if target.starts_with("http") || target.starts_with("mailto:") || target.is_empty() {
                continue;
            }
            let (path, fragment) = match target.split_once('#') {
                Some((p, f)) => (p, Some(f)),
                None => (target.as_str(), None),
            };
            let resolved = if path.is_empty() {
                file.clone()
            } else {
                dir.join(path)
            };
            if !resolved.exists() {
                broken.push(format!("{}: {target} (no such file)", file.display()));
                continue;
            }
            if let Some(fragment) = fragment {
                if resolved.extension().is_some_and(|e| e == "md") {
                    let body = std::fs::read_to_string(&resolved).unwrap();
                    if !anchors(&body).contains(fragment) {
                        broken.push(format!("{}: {target} (no such heading)", file.display()));
                    }
                }
            }
        }
    }
    assert!(
        broken.is_empty(),
        "broken documentation links:\n{}",
        broken.join("\n")
    );
}

#[test]
fn the_cli_reference_covers_every_command_the_binary_lists() {
    let out = Command::new(env!("CARGO_BIN_EXE_rex"))
        .arg("--help")
        .output()
        .expect("run rex --help");
    let usage =
        String::from_utf8_lossy(&out.stderr).to_string() + &String::from_utf8_lossy(&out.stdout);
    let commands: HashSet<String> = usage
        .lines()
        .filter_map(|l| l.trim_start().strip_prefix("rex "))
        .filter_map(|rest| rest.split_whitespace().next())
        .filter(|w| w.chars().all(|c| c.is_ascii_lowercase()))
        .map(str::to_string)
        .collect();
    assert!(
        ["exec", "resume", "login", "logout", "auth", "verify", "serve"]
            .iter()
            .all(|c| commands.contains(*c)),
        "the help text parse found {commands:?}"
    );
    let doc = std::fs::read_to_string(repo().join("docs/cli.md")).unwrap();
    let missing: Vec<&String> = commands
        .iter()
        .filter(|c| !doc.contains(&format!("rex {c}")))
        .collect();
    assert!(
        missing.is_empty(),
        "docs/cli.md never mentions: {missing:?}"
    );
}

#[test]
fn help_prints_the_usage_on_stdout_and_exits_zero() {
    for args in [&["--help"][..], &["-h"], &["exec", "--help"]] {
        let out = Command::new(env!("CARGO_BIN_EXE_rex"))
            .args(args)
            .output()
            .expect("run rex --help");
        assert!(out.status.success(), "{args:?} exited {:?}", out.status);
        assert!(String::from_utf8_lossy(&out.stdout).contains("usage:"));
        assert!(out.stderr.is_empty(), "{args:?} wrote to stderr");
    }
    // no command at all is still a usage error
    let out = Command::new(env!("CARGO_BIN_EXE_rex")).output().unwrap();
    assert_eq!(out.status.code(), Some(2));
}
