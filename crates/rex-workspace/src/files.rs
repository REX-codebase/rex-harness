//! Listing, reading and writing text files inside an opened folder, for the
//! Editor tab.
//!
//! The folder arrives already vetted by the registry; these functions add the
//! per-file rules: no path may leave the folder (including through a symlink),
//! files that are too big, binary or not UTF-8 are refused with a plain reason
//! instead of being opened and then corrupted on save, and a save is atomic so
//! a crash cannot leave half a file.

use crate::git::Git;
use serde::Serialize;
use std::fs;
use std::io::Write;
use std::path::{Component, Path, PathBuf};

/// Largest file the editor will open or save.
pub const MAX_TEXT_BYTES: u64 = 2 * 1024 * 1024;
/// Longest file list returned; more sets `truncated`.
pub const MAX_LISTED: usize = 5000;
const MAX_DEPTH: usize = 16;

/// Folders that are almost never wanted in a picker and can hold millions of
/// files. Used only when git (which knows what is ignored) cannot be asked.
const SKIPPED_DIRS: &[&str] = &[
    ".git",
    ".hg",
    ".svn",
    "node_modules",
    "target",
    "dist",
    "build",
    ".next",
    ".nuxt",
    ".venv",
    "venv",
    "__pycache__",
    ".idea",
    ".gradle",
    ".cache",
    ".tox",
];

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct FileListing {
    /// Paths relative to the folder, always with `/`.
    pub files: Vec<String>,
    pub truncated: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct TextFile {
    pub content: String,
    pub size: u64,
}

/// Every file in the folder, sorted. Inside a git repository this is what git
/// tracks plus what it does not ignore, so `node_modules` and build output stay
/// out of the way exactly as the person already configured.
pub fn list_files(git: &Git, ws: &Path, limit: usize) -> Result<FileListing, String> {
    let limit = limit.clamp(1, MAX_LISTED);
    if let Some(listing) = list_with_git(git, ws, limit) {
        return Ok(listing);
    }
    list_by_walking(ws, limit)
}

fn list_with_git(git: &Git, ws: &Path, limit: usize) -> Option<FileListing> {
    if !git.available() {
        return None;
    }
    let overview = git.overview(ws);
    if !overview.is_repo {
        return None;
    }
    let names = git.list_files_z(ws)?;
    let mut files: Vec<String> = names
        .into_iter()
        // tracked files deleted from disk are still in the index
        .filter(|p| ws.join(p).is_file())
        .collect();
    files.sort();
    files.dedup();
    let truncated = files.len() > limit;
    files.truncate(limit);
    Some(FileListing { files, truncated })
}

fn list_by_walking(ws: &Path, limit: usize) -> Result<FileListing, String> {
    let mut files = Vec::new();
    let mut truncated = false;
    let mut stack: Vec<(PathBuf, usize)> = vec![(ws.to_path_buf(), 0)];
    'walk: while let Some((dir, depth)) = stack.pop() {
        let entries = match fs::read_dir(&dir) {
            Ok(e) => e,
            Err(e) if dir == ws => return Err(format!("cannot read the folder: {e}")),
            Err(_) => continue,
        };
        for entry in entries.flatten() {
            let Ok(kind) = entry.file_type() else {
                continue;
            };
            let name = entry.file_name().to_string_lossy().to_string();
            // symlinks are not followed: they are how a path would leave the folder
            if kind.is_symlink() {
                continue;
            }
            if kind.is_dir() {
                if depth < MAX_DEPTH && !SKIPPED_DIRS.contains(&name.as_str()) {
                    stack.push((entry.path(), depth + 1));
                }
            } else if kind.is_file() {
                if files.len() >= limit {
                    truncated = true;
                    break 'walk;
                }
                if let Ok(rel) = entry.path().strip_prefix(ws) {
                    files.push(rel.to_string_lossy().replace('\\', "/"));
                }
            }
        }
    }
    files.sort();
    Ok(FileListing { files, truncated })
}

/// A relative path with no empty, `.` or `..` parts and no way to be absolute.
fn relative_parts(rel: &str) -> Result<PathBuf, String> {
    if rel.is_empty() || rel.contains('\0') {
        return Err("that path is not valid".into());
    }
    if crate::git::has_drive_prefix(rel) {
        return Err("path must be relative to the opened folder".into());
    }
    let mut out = PathBuf::new();
    for part in rel.split(['/', '\\']) {
        match part {
            "" | "." => continue,
            ".." => return Err("path escapes the opened folder".into()),
            p => out.push(p),
        }
    }
    if out.as_os_str().is_empty() {
        return Err("that path is not valid".into());
    }
    let first = out.components().next();
    if matches!(first, Some(Component::Prefix(_)) | Some(Component::RootDir))
        || Path::new(rel).is_absolute()
    {
        return Err("path must be relative to the opened folder".into());
    }
    Ok(out)
}

/// The real location of `rel` inside `ws`, or why it is not allowed. A file
/// that exists is resolved through symlinks; one that does not yet exist is
/// checked through its nearest existing parent.
fn resolve_inside(ws: &Path, rel: &str) -> Result<PathBuf, String> {
    let root = ws
        .canonicalize()
        .map_err(|_| "the folder no longer exists".to_string())?;
    let target = root.join(relative_parts(rel)?);
    let check = |p: &Path| -> Result<(), String> {
        if p.starts_with(&root) {
            Ok(())
        } else {
            Err("path escapes the opened folder".to_string())
        }
    };
    if target.exists() {
        let real = target
            .canonicalize()
            .map_err(|e| format!("cannot resolve the path: {e}"))?;
        check(&real)?;
        return Ok(real);
    }
    let mut ancestor = target.parent();
    while let Some(a) = ancestor {
        if a.exists() {
            check(
                &a.canonicalize()
                    .map_err(|e| format!("cannot resolve the path: {e}"))?,
            )?;
            return Ok(target);
        }
        ancestor = a.parent();
    }
    Err("path escapes the opened folder".into())
}

/// Read a text file the editor can safely round-trip.
pub fn read_text(ws: &Path, rel: &str) -> Result<TextFile, String> {
    let path = resolve_inside(ws, rel)?;
    let meta = fs::metadata(&path).map_err(|e| format!("cannot open the file: {e}"))?;
    if !meta.is_file() {
        return Err("that is not a file".into());
    }
    if meta.len() > MAX_TEXT_BYTES {
        return Err(format!(
            "This file is {:.1} MB; the editor opens text files up to {} MB.",
            meta.len() as f64 / 1_048_576.0,
            MAX_TEXT_BYTES / 1_048_576
        ));
    }
    let bytes = fs::read(&path).map_err(|e| format!("cannot read the file: {e}"))?;
    if bytes.iter().take(8000).any(|b| *b == 0) {
        return Err("This looks like a binary file; the editor opens text files only.".into());
    }
    let content = String::from_utf8(bytes).map_err(|_| {
        "This file is not UTF-8 text, so saving it from the editor could corrupt it.".to_string()
    })?;
    Ok(TextFile {
        content,
        size: meta.len(),
    })
}

/// Save text to a file, atomically: the new content is written next to the file
/// and renamed over it, keeping the file's permissions.
pub fn write_text(ws: &Path, rel: &str, content: &str) -> Result<(), String> {
    if content.len() as u64 > MAX_TEXT_BYTES {
        return Err(format!(
            "The file is over {} MB, which is the most the editor saves.",
            MAX_TEXT_BYTES / 1_048_576
        ));
    }
    let path = resolve_inside(ws, rel)?;
    if path.is_dir() {
        return Err("that is a folder".into());
    }
    let parent = path.parent().ok_or("that path is not valid")?;
    if !parent.is_dir() {
        return Err("the folder for this file no longer exists".into());
    }
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();
    let tmp = parent.join(format!(".{name}.rex-{}.tmp", std::process::id()));
    let result = (|| -> std::io::Result<()> {
        let mut f = fs::File::create(&tmp)?;
        f.write_all(content.as_bytes())?;
        f.sync_all()?;
        if let Ok(meta) = fs::metadata(&path) {
            let _ = fs::set_permissions(&tmp, meta.permissions());
        }
        fs::rename(&tmp, &path)
    })();
    if let Err(e) = result {
        let _ = fs::remove_file(&tmp);
        return Err(format!("cannot save the file: {e}"));
    }
    Ok(())
}

/// Create an empty file (and any missing folders above it). Never overwrites.
pub fn create_file(ws: &Path, rel: &str) -> Result<(), String> {
    let path = resolve_inside(ws, rel)?;
    if path.exists() {
        return Err("a file with that name already exists".into());
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("cannot create the folder: {e}"))?;
        // re-check after creating, so a racing symlink cannot redirect the write
        resolve_inside(ws, rel)?;
    }
    fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
        .map(|_| ())
        .map_err(|e| format!("cannot create the file: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ws() -> (tempfile::TempDir, PathBuf) {
        let tmp = tempfile::tempdir().unwrap();
        let ws = tmp.path().join("proj");
        fs::create_dir_all(ws.join("src").join("deep")).unwrap();
        (tmp, ws)
    }

    #[test]
    fn listing_walks_nested_folders_and_skips_build_output_without_git() {
        let (_tmp, ws) = ws();
        for f in [
            "README.md",
            "src/main.rs",
            "src/deep/mod.rs",
            "node_modules/x/index.js",
            "target/debug/a",
            ".git/config",
        ] {
            let p = ws.join(f);
            fs::create_dir_all(p.parent().unwrap()).unwrap();
            fs::write(p, "x").unwrap();
        }
        let listing = list_files(&Git::unavailable(), &ws, 100).unwrap();
        assert_eq!(
            listing.files,
            ["README.md", "src/deep/mod.rs", "src/main.rs"]
        );
        assert!(!listing.truncated);
    }

    #[test]
    fn listing_is_capped_and_says_so() {
        let (_tmp, ws) = ws();
        for i in 0..30 {
            fs::write(ws.join(format!("f{i:02}.txt")), "x").unwrap();
        }
        let listing = list_by_walking(&ws, 10).unwrap();
        assert_eq!(listing.files.len(), 10);
        assert!(listing.truncated);
    }

    #[test]
    fn in_a_repository_listing_follows_gitignore() {
        let git = Git::system();
        if !git.available() {
            return;
        }
        let (_tmp, ws) = ws();
        git.init(&ws).unwrap();
        fs::write(ws.join(".gitignore"), "secret.txt\nout/\n").unwrap();
        fs::write(ws.join("a.txt"), "x").unwrap();
        fs::write(ws.join("secret.txt"), "x").unwrap();
        fs::create_dir_all(ws.join("out")).unwrap();
        fs::write(ws.join("out").join("b.txt"), "x").unwrap();
        let listing = list_files(&git, &ws, 100).unwrap();
        assert_eq!(listing.files, [".gitignore", "a.txt"]);
    }

    #[test]
    fn a_text_file_round_trips() {
        let (_tmp, ws) = ws();
        write_text(&ws, "src/a.txt", "héllo\nwörld\n").unwrap();
        let t = read_text(&ws, "src/a.txt").unwrap();
        assert_eq!(t.content, "héllo\nwörld\n");
        assert_eq!(t.size, "héllo\nwörld\n".len() as u64);
        // forward and back slashes name the same file
        assert_eq!(read_text(&ws, "src\\a.txt").unwrap().content, t.content);
    }

    #[test]
    fn a_save_replaces_the_file_whole_and_leaves_no_temporary_behind() {
        let (_tmp, ws) = ws();
        write_text(&ws, "a.txt", "long original content").unwrap();
        write_text(&ws, "a.txt", "short").unwrap();
        assert_eq!(fs::read_to_string(ws.join("a.txt")).unwrap(), "short");
        let leftovers: Vec<_> = fs::read_dir(&ws)
            .unwrap()
            .flatten()
            .filter(|e| e.file_name().to_string_lossy().ends_with(".tmp"))
            .collect();
        assert!(leftovers.is_empty());
    }

    #[test]
    #[cfg(unix)]
    fn a_save_keeps_the_files_permissions() {
        use std::os::unix::fs::PermissionsExt;
        let (_tmp, ws) = ws();
        fs::write(ws.join("run.sh"), "#!/bin/sh\n").unwrap();
        fs::set_permissions(ws.join("run.sh"), fs::Permissions::from_mode(0o755)).unwrap();
        write_text(&ws, "run.sh", "#!/bin/sh\necho hi\n").unwrap();
        assert_eq!(
            fs::metadata(ws.join("run.sh"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o755
        );
    }

    #[test]
    fn files_that_cannot_round_trip_are_refused_with_a_reason() {
        let (_tmp, ws) = ws();
        fs::write(ws.join("bin.dat"), [1u8, 0, 2, 3]).unwrap();
        assert!(read_text(&ws, "bin.dat").unwrap_err().contains("binary"));
        fs::write(ws.join("latin1.txt"), [0x63u8, 0x61, 0x66, 0xe9]).unwrap();
        assert!(read_text(&ws, "latin1.txt")
            .unwrap_err()
            .contains("not UTF-8"));
        fs::write(
            ws.join("big.txt"),
            vec![b'a'; (MAX_TEXT_BYTES + 1) as usize],
        )
        .unwrap();
        assert!(read_text(&ws, "big.txt")
            .unwrap_err()
            .contains("up to 2 MB"));
        assert!(
            write_text(&ws, "x.txt", &"a".repeat((MAX_TEXT_BYTES + 1) as usize))
                .unwrap_err()
                .contains("2 MB")
        );
        assert!(read_text(&ws, "src").unwrap_err().contains("not a file"));
    }

    #[test]
    fn no_path_can_leave_the_folder() {
        let (tmp, ws) = ws();
        fs::write(tmp.path().join("outside.txt"), "secret").unwrap();
        for bad in [
            "../outside.txt",
            "src/../../outside.txt",
            "..\\outside.txt",
            "",
            "/etc/passwd",
            "C:\\Windows\\x",
        ] {
            assert!(read_text(&ws, bad).is_err(), "read {bad:?}");
            assert!(write_text(&ws, bad, "x").is_err(), "write {bad:?}");
            assert!(create_file(&ws, bad).is_err(), "create {bad:?}");
        }
        assert_eq!(
            fs::read_to_string(tmp.path().join("outside.txt")).unwrap(),
            "secret"
        );
    }

    #[test]
    #[cfg(unix)]
    fn a_symlink_cannot_be_used_to_read_or_write_outside() {
        let (tmp, ws) = ws();
        fs::write(tmp.path().join("outside.txt"), "secret").unwrap();
        std::os::unix::fs::symlink(tmp.path().join("outside.txt"), ws.join("link.txt")).unwrap();
        assert!(read_text(&ws, "link.txt").unwrap_err().contains("escapes"));
        assert!(write_text(&ws, "link.txt", "pwned")
            .unwrap_err()
            .contains("escapes"));
        assert_eq!(
            fs::read_to_string(tmp.path().join("outside.txt")).unwrap(),
            "secret"
        );
        // and a symlinked folder is not a way out for a new file either
        std::os::unix::fs::symlink(tmp.path(), ws.join("up")).unwrap();
        assert!(create_file(&ws, "up/new.txt").is_err());
        assert!(!tmp.path().join("new.txt").exists());
    }

    #[test]
    fn creating_a_file_makes_folders_and_never_overwrites() {
        let (_tmp, ws) = ws();
        create_file(&ws, "docs/notes/todo.md").unwrap();
        assert_eq!(
            fs::read_to_string(ws.join("docs/notes/todo.md")).unwrap(),
            ""
        );
        let err = create_file(&ws, "docs/notes/todo.md").unwrap_err();
        assert!(err.contains("already exists"), "{err}");
        write_text(&ws, "docs/notes/todo.md", "keep me").unwrap();
        assert!(create_file(&ws, "docs/notes/todo.md").is_err());
        assert_eq!(
            fs::read_to_string(ws.join("docs/notes/todo.md")).unwrap(),
            "keep me"
        );
    }
}
