//! The folders the person has opened, and the rule for what the shell may touch.
//!
//! The webview is not a trusted caller: it renders model-written text and
//! previews of generated pages. So a path it sends is never enough. Every
//! command that takes a folder asks [`WorkspaceRegistry::resolve`], which
//! accepts only a folder inside one the person opened through the native
//! dialog (or inside the runs root, where REX's own sandboxes live).
//! [`WorkspaceRegistry::register`] is the only way to add one, and the shell
//! calls it only with the path the native dialog returned.

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// How many opened folders are remembered.
pub const MAX_RECENT: usize = 12;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct Stored {
    #[serde(default)]
    active: Option<String>,
    #[serde(default)]
    recent: Vec<String>,
}

/// A folder as the UI shows it.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct WorkspaceInfo {
    pub path: String,
    pub name: String,
    /// False when the folder has been moved or deleted since it was opened.
    pub exists: bool,
}

/// A path for display: Windows' `\\?\C:\...` form is how the OS spells a
/// canonical path, and nobody wants to read it.
pub fn display_path(path: &Path) -> String {
    let text = path.to_string_lossy();
    match text.strip_prefix(r"\\?\") {
        Some(rest) if !rest.starts_with("UNC\\") => rest.to_string(),
        _ => text.to_string(),
    }
}

/// Everything the UI needs to draw the folder chip and its menu in one call.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct WorkspaceSnapshot {
    pub active: Option<WorkspaceInfo>,
    /// Folders the person opened, most recent first.
    pub recent: Vec<WorkspaceInfo>,
    /// REX's own run sandboxes, newest first.
    pub runs: Vec<WorkspaceInfo>,
}

fn info_for(path: &Path) -> WorkspaceInfo {
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .filter(|n| !n.is_empty())
        .unwrap_or_else(|| display_path(path));
    WorkspaceInfo {
        path: display_path(path),
        name,
        exists: path.is_dir(),
    }
}

pub struct WorkspaceRegistry {
    file: PathBuf,
    runs_root: PathBuf,
    state: Mutex<Stored>,
}

impl WorkspaceRegistry {
    /// Open the registry kept in `dir`. A missing or damaged file is an empty
    /// registry, not an error: losing the recent list must never stop the app.
    pub fn open(dir: &Path, runs_root: PathBuf) -> Self {
        let file = dir.join("workspaces.json");
        let state = fs::read(&file)
            .ok()
            .and_then(|bytes| serde_json::from_slice::<Stored>(&bytes).ok())
            .unwrap_or_default();
        WorkspaceRegistry {
            file,
            runs_root,
            state: Mutex::new(state),
        }
    }

    fn save(&self, state: &Stored) -> Result<(), String> {
        if let Some(parent) = self.file.parent() {
            fs::create_dir_all(parent).map_err(|e| format!("cannot save the folder list: {e}"))?;
        }
        let tmp = self.file.with_extension("json.tmp");
        let bytes = serde_json::to_vec_pretty(state).map_err(|e| e.to_string())?;
        fs::write(&tmp, bytes).map_err(|e| format!("cannot save the folder list: {e}"))?;
        fs::rename(&tmp, &self.file).map_err(|e| format!("cannot save the folder list: {e}"))
    }

    fn canonical_roots(state: &Stored) -> Vec<PathBuf> {
        state
            .recent
            .iter()
            .filter_map(|r| Path::new(r).canonicalize().ok())
            .collect()
    }

    /// The state the folder chip is drawn from.
    pub fn snapshot(&self) -> WorkspaceSnapshot {
        WorkspaceSnapshot {
            active: self.active().map(|p| info_for(&p)),
            recent: self.recent(),
            runs: self.run_sandboxes(12),
        }
    }

    /// The folder the tabs work in, if one was chosen and it still exists.
    pub fn active(&self) -> Option<PathBuf> {
        let state = self.state.lock().ok()?;
        let active = state.active.as_ref()?;
        let canon = Path::new(active).canonicalize().ok()?;
        canon.is_dir().then_some(canon)
    }

    /// Opened folders, most recent first.
    pub fn recent(&self) -> Vec<WorkspaceInfo> {
        let Ok(state) = self.state.lock() else {
            return Vec::new();
        };
        state
            .recent
            .iter()
            .map(|p| info_for(Path::new(p)))
            .collect()
    }

    /// REX's own run sandboxes (`task-ui-*` under the runs root), newest first,
    /// so the tabs can look at what an autonomous run left behind.
    pub fn run_sandboxes(&self, limit: usize) -> Vec<WorkspaceInfo> {
        let Ok(entries) = fs::read_dir(&self.runs_root) else {
            return Vec::new();
        };
        let mut found: Vec<(std::time::SystemTime, PathBuf)> = entries
            .flatten()
            .filter_map(|e| {
                let path = e.path();
                let name = e.file_name().to_string_lossy().to_string();
                if !path.is_dir() || !name.starts_with("task-") {
                    return None;
                }
                let modified = e.metadata().ok()?.modified().ok()?;
                Some((modified, path.canonicalize().ok()?))
            })
            .collect();
        found.sort_by_key(|(modified, _)| std::cmp::Reverse(*modified));
        found
            .into_iter()
            .take(limit)
            .map(|(_, p)| info_for(&p))
            .collect()
    }

    /// Add a folder the person just chose in the native dialog and make it the
    /// active one. Call this only with the dialog's own result.
    pub fn register(&self, chosen: &Path) -> Result<PathBuf, String> {
        let canon = chosen
            .canonicalize()
            .map_err(|e| format!("that folder cannot be opened: {e}"))?;
        if !canon.is_dir() {
            return Err("choose a folder, not a file".into());
        }
        if canon.parent().is_none() {
            return Err("choose a project folder, not a whole drive".into());
        }
        let text = canon.to_string_lossy().to_string();
        let mut state = self
            .state
            .lock()
            .map_err(|_| "folder list poisoned".to_string())?;
        state.recent.retain(|r| r != &text);
        state.recent.insert(0, text.clone());
        state.recent.truncate(MAX_RECENT);
        state.active = Some(text);
        self.save(&state)?;
        Ok(canon)
    }

    /// Switch to a folder that was opened before (or a run sandbox). A path
    /// that was never opened is refused: this is how the list is *used*, not
    /// how it grows.
    pub fn activate(&self, requested: &str) -> Result<PathBuf, String> {
        let canon = self.resolve(requested)?;
        let mut state = self
            .state
            .lock()
            .map_err(|_| "folder list poisoned".to_string())?;
        state.active = Some(canon.to_string_lossy().to_string());
        self.save(&state)?;
        Ok(canon)
    }

    /// Remove a folder from the list (the folder itself is not touched).
    pub fn forget(&self, requested: &str) -> Result<(), String> {
        let target = Path::new(requested).canonicalize().ok();
        let mut state = self
            .state
            .lock()
            .map_err(|_| "folder list poisoned".to_string())?;
        let before = state.recent.len();
        state.recent.retain(|r| {
            let same = match (&target, Path::new(r).canonicalize().ok()) {
                (Some(t), Some(c)) => *t == c,
                _ => r == requested,
            };
            !same
        });
        if state.recent.len() != before {
            let active_gone = state
                .active
                .as_ref()
                .is_some_and(|a| !state.recent.contains(a));
            if active_gone {
                state.active = None;
            }
            self.save(&state)?;
        }
        Ok(())
    }

    /// The gate every folder-taking command goes through: the canonical folder,
    /// if it is inside one the person opened or inside the runs root.
    pub fn resolve(&self, requested: &str) -> Result<PathBuf, String> {
        if requested.trim().is_empty() {
            return Err("no folder is open".into());
        }
        let canon = Path::new(requested)
            .canonicalize()
            .map_err(|_| "that folder no longer exists".to_string())?;
        if !canon.is_dir() {
            return Err("that is not a folder".into());
        }
        let state = self
            .state
            .lock()
            .map_err(|_| "folder list poisoned".to_string())?;
        let in_opened = Self::canonical_roots(&state)
            .iter()
            .any(|root| canon.starts_with(root));
        let in_runs = self
            .runs_root
            .canonicalize()
            .map(|root| canon.starts_with(root))
            .unwrap_or(false);
        if in_opened || in_runs {
            Ok(canon)
        } else {
            Err("that folder has not been opened in REX. Use Open folder to choose it.".into())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Rig {
        _tmp: tempfile::TempDir,
        dir: PathBuf,
        runs: PathBuf,
        project: PathBuf,
        reg: WorkspaceRegistry,
    }

    fn rig() -> Rig {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("config");
        let runs = tmp.path().join("agent-runs");
        let project = tmp.path().join("my-project");
        fs::create_dir_all(&dir).unwrap();
        fs::create_dir_all(&runs).unwrap();
        fs::create_dir_all(project.join("src")).unwrap();
        let reg = WorkspaceRegistry::open(&dir, runs.clone());
        Rig {
            _tmp: tmp,
            dir,
            runs,
            project,
            reg,
        }
    }

    #[test]
    fn nothing_is_open_until_the_person_chooses_a_folder() {
        let r = rig();
        assert!(r.reg.active().is_none());
        assert!(r.reg.recent().is_empty());
        // and the webview naming a path of its own does not open it
        let err = r.reg.resolve(r.project.to_str().unwrap()).unwrap_err();
        assert!(err.contains("has not been opened"), "{err}");
        let err = r.reg.activate(r.project.to_str().unwrap()).unwrap_err();
        assert!(err.contains("has not been opened"), "{err}");
        assert!(r.reg.active().is_none());
    }

    #[test]
    fn a_chosen_folder_becomes_active_and_its_subfolders_resolve() {
        let r = rig();
        let chosen = r.reg.register(&r.project).unwrap();
        assert_eq!(r.reg.active(), Some(chosen.clone()));
        assert_eq!(r.reg.recent()[0].name, "my-project");
        assert_eq!(r.reg.resolve(r.project.to_str().unwrap()).unwrap(), chosen);
        let sub = r.project.join("src");
        assert_eq!(
            r.reg.resolve(sub.to_str().unwrap()).unwrap(),
            sub.canonicalize().unwrap()
        );
    }

    #[test]
    fn a_sibling_or_parent_of_an_opened_folder_is_not_covered() {
        let r = rig();
        r.reg.register(&r.project).unwrap();
        let parent = r.project.parent().unwrap();
        assert!(r.reg.resolve(parent.to_str().unwrap()).is_err());
        let sibling = parent.join("other");
        fs::create_dir_all(&sibling).unwrap();
        assert!(r.reg.resolve(sibling.to_str().unwrap()).is_err());
        // `..` cannot walk out either: the path is canonicalized first
        let sneaky = r.project.join("src").join("..").join("..");
        assert!(r.reg.resolve(sneaky.to_str().unwrap()).is_err());
    }

    #[test]
    fn a_symlink_out_of_an_opened_folder_is_not_covered() {
        #[cfg(unix)]
        {
            let r = rig();
            r.reg.register(&r.project).unwrap();
            let outside = r.dir.join("secret");
            fs::create_dir_all(&outside).unwrap();
            std::os::unix::fs::symlink(&outside, r.project.join("link")).unwrap();
            let via_link = r.project.join("link");
            assert!(r.reg.resolve(via_link.to_str().unwrap()).is_err());
        }
    }

    #[test]
    fn run_sandboxes_are_always_allowed_and_listed_newest_first() {
        let r = rig();
        let old = r.runs.join("task-ui-1");
        let new = r.runs.join("task-ui-2");
        fs::create_dir_all(&old).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(30));
        fs::create_dir_all(&new).unwrap();
        fs::create_dir_all(r.runs.join("not-a-task")).unwrap();
        let listed = r.reg.run_sandboxes(10);
        assert_eq!(
            listed.iter().map(|w| w.name.as_str()).collect::<Vec<_>>(),
            ["task-ui-2", "task-ui-1"]
        );
        assert_eq!(r.reg.run_sandboxes(1).len(), 1);
        // allowed without ever being "opened", and can be made active
        assert!(r.reg.resolve(new.to_str().unwrap()).is_ok());
        assert!(r.reg.activate(new.to_str().unwrap()).is_ok());
        assert_eq!(r.reg.active(), Some(new.canonicalize().unwrap()));
    }

    #[test]
    fn the_list_is_most_recent_first_without_duplicates_and_capped() {
        let r = rig();
        let base = r.dir.join("many");
        for i in 0..(MAX_RECENT + 3) {
            let p = base.join(format!("p{i}"));
            fs::create_dir_all(&p).unwrap();
            r.reg.register(&p).unwrap();
        }
        assert_eq!(r.reg.recent().len(), MAX_RECENT);
        assert_eq!(r.reg.recent()[0].name, format!("p{}", MAX_RECENT + 2));
        // re-opening an older one moves it to the front, once
        let again = base.join(format!("p{}", MAX_RECENT + 1));
        r.reg.register(&again).unwrap();
        let names: Vec<String> = r.reg.recent().into_iter().map(|w| w.name).collect();
        assert_eq!(names[0], format!("p{}", MAX_RECENT + 1));
        assert_eq!(names.iter().filter(|n| **n == names[0]).count(), 1);
    }

    #[test]
    fn it_survives_a_restart_and_a_damaged_file() {
        let r = rig();
        let chosen = r.reg.register(&r.project).unwrap();
        let reopened = WorkspaceRegistry::open(&r.dir, r.runs.clone());
        assert_eq!(reopened.active(), Some(chosen));
        fs::write(r.dir.join("workspaces.json"), b"{not json").unwrap();
        let damaged = WorkspaceRegistry::open(&r.dir, r.runs.clone());
        assert!(damaged.active().is_none());
        assert!(damaged.recent().is_empty());
    }

    #[test]
    fn a_folder_that_was_deleted_is_not_active_and_is_reported_missing() {
        let r = rig();
        r.reg.register(&r.project).unwrap();
        fs::remove_dir_all(&r.project).unwrap();
        assert!(r.reg.active().is_none());
        assert!(!r.reg.recent()[0].exists);
        assert!(r.reg.resolve(r.project.to_str().unwrap()).is_err());
    }

    #[test]
    fn forgetting_removes_it_from_the_list_and_clears_it_if_active() {
        let r = rig();
        r.reg.register(&r.project).unwrap();
        r.reg.forget(r.project.to_str().unwrap()).unwrap();
        assert!(r.reg.recent().is_empty());
        assert!(r.reg.active().is_none());
        assert!(r.project.is_dir(), "the folder itself is untouched");
        assert!(r.reg.resolve(r.project.to_str().unwrap()).is_err());
    }

    #[test]
    fn files_and_whole_drives_are_refused() {
        let r = rig();
        let file = r.project.join("a.txt");
        fs::write(&file, "x").unwrap();
        assert!(r.reg.register(&file).unwrap_err().contains("not a file"));
        let root = if cfg!(windows) {
            Path::new("C:\\")
        } else {
            Path::new("/")
        };
        assert!(r.reg.register(root).unwrap_err().contains("whole drive"));
    }

    #[test]
    fn windows_verbatim_prefixes_are_not_shown() {
        assert_eq!(
            display_path(Path::new(r"\\?\C:\Users\a\proj")),
            r"C:\Users\a\proj"
        );
        assert_eq!(
            display_path(Path::new(r"\\?\UNC\srv\share")),
            r"\\?\UNC\srv\share"
        );
        assert_eq!(display_path(Path::new("/home/a/proj")), "/home/a/proj");
    }
}
