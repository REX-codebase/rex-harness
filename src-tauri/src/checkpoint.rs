//! Workspace checkpoints: snapshots for one-click restore.
//!
//! Checkpoints are full copies of the workspace stored under the REX config
//! dir (`checkpoints/{id}/`). The ledger (`checkpoints.json`) records each
//! checkpoint's workspace, label, and timestamp. Restore copies a checkpoint
//! back over the workspace — but first it auto-snapshots the current state,
//! so a restore is never destructive.
//!
//! A checkpoint covers the files a person would call "the project", not what a
//! build tool regenerates: `node_modules`, `target`, `dist` and similar are
//! neither copied nor, on restore, deleted. Symlinks are never followed (a
//! restore must not be able to delete files outside the folder through one),
//! and a folder too big to snapshot is refused instead of filling the disk.

use std::cmp::Reverse;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};

static ID_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Folders skipped when they are at the top of the workspace.
const EXCLUDED_TOP: &[&str] = &[
    ".git", "dist", "build", "target", ".next", ".nuxt", ".venv", "venv", ".cache", ".gradle",
    ".idea", ".tox",
];
/// Folders skipped wherever they are.
const EXCLUDED_ANYWHERE: &[&str] = &["node_modules", "__pycache__"];
/// The most files, and bytes, a checkpoint will take.
pub const MAX_FILES: u64 = 20_000;
pub const MAX_BYTES: u64 = 256 * 1024 * 1024;

fn is_excluded(rel: &Path) -> bool {
    let mut parts = rel.components().filter_map(|c| c.as_os_str().to_str());
    if let Some(first) = parts.next() {
        if EXCLUDED_TOP.contains(&first) || EXCLUDED_ANYWHERE.contains(&first) {
            return true;
        }
    }
    parts.any(|c| EXCLUDED_ANYWHERE.contains(&c))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Checkpoint {
    pub id: String,
    pub workspace: String,
    pub label: String,
    pub created_at_ms: u64,
    /// Number of files in the snapshot.
    pub file_count: u64,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct Ledger {
    checkpoints: Vec<Checkpoint>,
}

pub struct CheckpointStore {
    dir: PathBuf,
    ledger_path: PathBuf,
    inner: Mutex<Ledger>,
}

impl CheckpointStore {
    pub fn new(config_dir: &Path) -> Result<Self, String> {
        let dir = config_dir.join("checkpoints");
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let ledger_path = dir.join("checkpoints.json");
        let ledger = if ledger_path.exists() {
            let data = std::fs::read_to_string(&ledger_path).map_err(|e| e.to_string())?;
            serde_json::from_str(&data).map_err(|e| e.to_string())?
        } else {
            Ledger::default()
        };
        Ok(CheckpointStore {
            dir,
            ledger_path,
            inner: Mutex::new(ledger),
        })
    }

    fn save(&self) -> Result<(), String> {
        let ledger = self.inner.lock().map_err(|e| e.to_string())?;
        let data = serde_json::to_string_pretty(&*ledger).map_err(|e| e.to_string())?;
        // Atomic write.
        let tmp = self.ledger_path.with_extension("tmp");
        std::fs::write(&tmp, data).map_err(|e| e.to_string())?;
        std::fs::rename(&tmp, &self.ledger_path).map_err(|e| e.to_string())?;
        Ok(())
    }

    fn workspace_dir(&self, workspace: &Path) -> Result<PathBuf, String> {
        let ws = workspace
            .canonicalize()
            .map_err(|e| format!("workspace not found: {e}"))?;
        // A folder that holds REX's own data would snapshot its own snapshots.
        if self
            .dir
            .canonicalize()
            .map(|d| d.starts_with(&ws))
            .unwrap_or(false)
        {
            return Err(
                "this folder contains REX's own data, so it cannot be checkpointed".to_string(),
            );
        }
        Ok(ws)
    }

    /// Copy what `scan(src)` finds into `dst`. Returns the number of files.
    fn copy_workspace(&self, src: &Path, dst: &Path) -> Result<u64, String> {
        let found = scan(src)?;
        std::fs::create_dir_all(dst).map_err(|e| e.to_string())?;
        for dir in &found.dirs {
            let rel = dir.strip_prefix(src).map_err(|e| e.to_string())?;
            std::fs::create_dir_all(dst.join(rel)).map_err(|e| e.to_string())?;
        }
        for file in &found.files {
            let rel = file.strip_prefix(src).map_err(|e| e.to_string())?;
            let target = dst.join(rel);
            if let Some(parent) = target.parent() {
                std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
            std::fs::copy(file, &target)
                .map_err(|e| format!("cannot copy {}: {e}", rel.display()))?;
        }
        Ok(found.files.len() as u64)
    }

    /// Create a checkpoint of the workspace.
    pub fn create(&self, workspace: &Path, label: &str) -> Result<Checkpoint, String> {
        let ws = self.workspace_dir(workspace)?;
        let n = ID_COUNTER.fetch_add(1, Ordering::SeqCst);
        let id = format!(
            "ckpt-{}-{}-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_err(|e| e.to_string())?
                .as_millis(),
            std::process::id(),
            n
        );
        let dst = self.dir.join(&id);
        let file_count = self.copy_workspace(&ws, &dst)?;
        let cp = Checkpoint {
            id: id.clone(),
            workspace: ws.to_string_lossy().to_string(),
            label: label.to_string(),
            created_at_ms: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_err(|e| e.to_string())?
                .as_millis() as u64,
            file_count,
        };
        {
            let mut ledger = self.inner.lock().map_err(|e| e.to_string())?;
            ledger.checkpoints.push(cp.clone());
        }
        self.save()?;
        Ok(cp)
    }

    /// List checkpoints for a workspace, newest first.
    pub fn list(&self, workspace: &Path) -> Result<Vec<Checkpoint>, String> {
        let ws = self.workspace_dir(workspace)?;
        let ws_str = ws.to_string_lossy().to_string();
        let ledger = self.inner.lock().map_err(|e| e.to_string())?;
        let mut out: Vec<Checkpoint> = ledger
            .checkpoints
            .iter()
            .filter(|c| c.workspace == ws_str)
            .cloned()
            .collect();
        out.sort_by_key(|a| Reverse(a.created_at_ms));
        Ok(out)
    }

    /// Restore a checkpoint. Auto-snapshots the current workspace first, so
    /// the restore is reversible.
    pub fn restore(&self, workspace: &Path, id: &str) -> Result<Checkpoint, String> {
        let ws = self.workspace_dir(workspace)?;
        let src = {
            let ledger = self.inner.lock().map_err(|e| e.to_string())?;
            let cp = ledger
                .checkpoints
                .iter()
                .find(|c| c.id == id)
                .ok_or("checkpoint not found")?;
            if cp.workspace != ws.to_string_lossy() {
                return Err("checkpoint belongs to a different workspace".to_string());
            }
            self.dir.join(id)
        };
        if !src.exists() {
            return Err("checkpoint data missing".to_string());
        }
        // Auto-snapshot current state before overwriting.
        let backup = self.create(&ws, "auto-backup before restore")?;
        // Clear what a checkpoint covers (never excluded folders, never through
        // a symlink), then copy the checkpoint back.
        let current = scan(&ws)?;
        for file in &current.files {
            let _ = std::fs::remove_file(file);
        }
        // Remove now-empty folders, deepest first; a folder that still holds
        // something (an excluded folder's contents) is left alone.
        let mut dirs = current.dirs;
        dirs.sort_by_key(|a| Reverse(a.components().count()));
        for d in dirs {
            let _ = std::fs::remove_dir(&d);
        }
        let restored_count = self.copy_workspace(&src, &ws).map_err(|e| {
            format!(
                "The restore stopped part-way ({e}). Your files from before it are saved as checkpoint \"{}\"; restore that one to undo.",
                backup.id
            )
        })?;
        // Update the backup's file count to reflect what was there.
        {
            let mut ledger = self.inner.lock().map_err(|e| e.to_string())?;
            if let Some(b) = ledger.checkpoints.iter_mut().find(|c| c.id == backup.id) {
                b.file_count = restored_count;
            }
        }
        self.save()?;
        Ok(backup)
    }
}

/// What a checkpoint covers under `root`.
#[derive(Debug)]
struct Scan {
    files: Vec<PathBuf>,
    dirs: Vec<PathBuf>,
}

/// Walk `root`, skipping excluded folders and never following a symlink (a
/// symlink is neither copied nor, on restore, removed). Refuses a folder over
/// the limits rather than copying it.
fn scan(root: &Path) -> Result<Scan, String> {
    scan_within(root, MAX_FILES, MAX_BYTES)
}

fn scan_within(root: &Path, max_files: u64, max_bytes: u64) -> Result<Scan, String> {
    let mut files = Vec::new();
    let mut dirs = Vec::new();
    let mut bytes = 0u64;
    let mut stack = vec![root.to_path_buf()];
    while let Some(current) = stack.pop() {
        let entries = std::fs::read_dir(&current).map_err(|e| e.to_string())?;
        for entry in entries {
            let path = entry.map_err(|e| e.to_string())?.path();
            let rel = path.strip_prefix(root).map_err(|e| e.to_string())?;
            if is_excluded(rel) {
                continue;
            }
            let meta = std::fs::symlink_metadata(&path).map_err(|e| e.to_string())?;
            if meta.file_type().is_symlink() {
                continue;
            }
            if meta.is_dir() {
                dirs.push(path.clone());
                stack.push(path);
            } else if meta.is_file() {
                bytes += meta.len();
                files.push(path);
                if files.len() as u64 > max_files || bytes > max_bytes {
                    return Err(format!(
                        "This folder is too big to checkpoint (over {} files or {} MB). Open a smaller folder, or use git for history.",
                        max_files,
                        max_bytes / (1024 * 1024)
                    ));
                }
            }
        }
    }
    Ok(Scan { files, dirs })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    // The real limits are generous but finite; checked when this is compiled.
    const _: () = assert!(MAX_FILES >= 1000 && MAX_BYTES >= 64 * 1024 * 1024);

    static COUNTER: AtomicU64 = AtomicU64::new(0);

    fn temp() -> PathBuf {
        let n = COUNTER.fetch_add(1, Ordering::SeqCst);
        let dir = std::env::temp_dir().join(format!("rex-ckpt-test-{}-{}", std::process::id(), n));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn create_list_restore() {
        let config = temp();
        let ws = temp();
        std::fs::write(ws.join("a.txt"), "v1").unwrap();

        let store = CheckpointStore::new(&config).unwrap();
        let cp = store.create(&ws, "test").unwrap();
        assert_eq!(cp.file_count, 1);

        let list = store.list(&ws).unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].id, cp.id);

        // Modify and restore.
        std::fs::write(ws.join("a.txt"), "v2").unwrap();
        let backup = store.restore(&ws, &cp.id).unwrap();
        assert_eq!(backup.label, "auto-backup before restore");

        let content = std::fs::read_to_string(ws.join("a.txt")).unwrap();
        assert_eq!(content, "v1");

        // The backup has the v2 content.
        let backup_dir = config.join("checkpoints").join(&backup.id);
        let backup_content = std::fs::read_to_string(backup_dir.join("a.txt")).unwrap();
        assert_eq!(backup_content, "v2");
    }

    #[test]
    fn build_output_is_neither_copied_nor_deleted_by_a_restore() {
        let config = temp();
        let ws = temp();
        std::fs::write(ws.join("a.txt"), "v1").unwrap();
        std::fs::create_dir_all(ws.join("node_modules/pkg")).unwrap();
        std::fs::write(ws.join("node_modules/pkg/index.js"), "dep").unwrap();
        std::fs::create_dir_all(ws.join("src/node_modules")).unwrap();
        std::fs::write(ws.join("src/node_modules/x.js"), "nested dep").unwrap();
        std::fs::create_dir_all(ws.join("target/debug")).unwrap();
        std::fs::write(ws.join("target/debug/bin"), "built").unwrap();
        std::fs::create_dir_all(ws.join("src/build")).unwrap();
        std::fs::write(
            ws.join("src/build/real_source.rs"),
            "a source folder that happens to be called build",
        )
        .unwrap();

        let store = CheckpointStore::new(&config).unwrap();
        let cp = store.create(&ws, "t").unwrap();
        assert_eq!(cp.file_count, 2, "a.txt and src/build/real_source.rs only");
        assert!(!config
            .join("checkpoints")
            .join(&cp.id)
            .join("node_modules")
            .exists());

        std::fs::write(ws.join("a.txt"), "v2").unwrap();
        std::fs::write(ws.join("new.txt"), "made after").unwrap();
        store.restore(&ws, &cp.id).unwrap();

        assert_eq!(std::fs::read_to_string(ws.join("a.txt")).unwrap(), "v1");
        assert!(
            !ws.join("new.txt").exists(),
            "a file made after the checkpoint is removed"
        );
        // the regenerable folders are exactly as they were
        assert_eq!(
            std::fs::read_to_string(ws.join("node_modules/pkg/index.js")).unwrap(),
            "dep"
        );
        assert_eq!(
            std::fs::read_to_string(ws.join("src/node_modules/x.js")).unwrap(),
            "nested dep"
        );
        assert_eq!(
            std::fs::read_to_string(ws.join("target/debug/bin")).unwrap(),
            "built"
        );
        assert!(ws.join("src/build/real_source.rs").exists());
    }

    #[test]
    #[cfg(unix)]
    fn a_restore_never_deletes_through_a_symlink_out_of_the_folder() {
        let config = temp();
        let ws = temp();
        let outside = temp();
        std::fs::write(outside.join("precious.txt"), "keep").unwrap();
        std::os::unix::fs::symlink(&outside, ws.join("link")).unwrap();
        std::fs::write(ws.join("a.txt"), "v1").unwrap();

        let store = CheckpointStore::new(&config).unwrap();
        let cp = store.create(&ws, "t").unwrap();
        assert_eq!(
            cp.file_count, 1,
            "the symlink is not followed, so its target is not copied in"
        );
        store.restore(&ws, &cp.id).unwrap();

        assert_eq!(
            std::fs::read_to_string(outside.join("precious.txt")).unwrap(),
            "keep"
        );
        assert!(
            ws.join("link")
                .symlink_metadata()
                .unwrap()
                .file_type()
                .is_symlink(),
            "the link itself survives"
        );
    }

    #[test]
    fn a_folder_over_the_limit_is_refused_not_copied() {
        let ws = temp();
        for i in 0..5 {
            std::fs::write(ws.join(format!("f{i}.txt")), "xxxx").unwrap();
        }
        assert_eq!(
            scan_within(&ws, 5, 1024).unwrap().files.len(),
            5,
            "exactly at the limit is fine"
        );
        let by_count = scan_within(&ws, 4, 1024 * 1024).unwrap_err();
        assert!(by_count.contains("too big"), "{by_count}");
        let by_size = scan_within(&ws, 100, 10).unwrap_err();
        assert!(by_size.contains("too big"), "{by_size}");
    }

    #[test]
    fn a_folder_containing_the_checkpoint_store_is_refused() {
        let config = temp();
        std::fs::write(config.join("settings.json"), "{}").unwrap();
        let store = CheckpointStore::new(&config).unwrap();
        // the store lives in config/checkpoints, so config itself would snapshot its own snapshots
        let err = store.create(&config, "self").unwrap_err();
        assert!(err.contains("REX's own data"), "{err}");
    }

    #[test]
    fn excluded_paths_are_recognised() {
        for yes in [
            "node_modules/x",
            "src/node_modules/x",
            "target/debug",
            ".git/config",
            "dist/a.js",
            "pkg/__pycache__/m.pyc",
        ] {
            assert!(is_excluded(Path::new(yes)), "{yes}");
        }
        for no in [
            "src/build/a.rs",
            "src/target/a.rs",
            "docs/dist.md",
            "a.txt",
            "build.rs",
        ] {
            assert!(!is_excluded(Path::new(no)), "{no}");
        }
    }
}
