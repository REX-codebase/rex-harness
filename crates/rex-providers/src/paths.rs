//! Where REX keeps its local data on this machine: saved credentials, the
//! ChatGPT sign-in, run history, checkpoints, receipts and settings.
//!
//! Everything REX remembers stays on the machine; there is no server. This
//! module is the one place that decides which folder that is, so the desktop
//! app, the `rex` command and the helper tools agree.
//!
//! Resolution, first match wins:
//!
//! 1. `REX_STATE_DIR`, when set and not empty. Point the desktop app and the
//!    CLI at the same folder to share one sign-in and one run history.
//! 2. **Windows:** `%LOCALAPPDATA%\rex-harness` (falling back to
//!    `%USERPROFILE%\AppData\Local\rex-harness`, then `%APPDATA%\rex-harness`).
//!    The folder is per user, outside the temp folder, and the same for the app
//!    and the CLI, so they share by default.
//! 3. **macOS and Linux:** the desktop app uses
//!    `$XDG_CONFIG_HOME/rex-harness` or `~/.config/rex-harness`, and the CLI
//!    uses `~/.rex/harness`. These are the locations earlier versions used, so
//!    existing data is found where it was left.
//! 4. Only when no home folder can be found at all: a folder under the system
//!    temp directory. It is never the current directory, and a warning is the
//!    caller's to give.
//!
//! Before this module the desktop app fell back to the temp folder whenever
//! `HOME` was unset (which is the normal case on Windows) and the CLI to the
//! current directory, so on Windows a sign-in could end up in `%TEMP%`, or in a
//! different folder for every directory the CLI was run from.

use std::ffi::OsString;
use std::path::PathBuf;

/// Folder name used under the per-user data directory.
pub const APP_DIR: &str = "rex-harness";
/// The environment variable that overrides the location (see the module docs).
pub const STATE_DIR_ENV: &str = "REX_STATE_DIR";

/// Which program is asking. On macOS and Linux the two historically used
/// different folders, and keeping that means nobody's data moves.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Flavor {
    /// The desktop app (and its dev sidecar and helpers).
    Desktop,
    /// The `rex` command line.
    Cli,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Platform {
    Windows,
    Unix,
}

impl Platform {
    const fn current() -> Self {
        if cfg!(windows) {
            Platform::Windows
        } else {
            Platform::Unix
        }
    }
}

/// The folder REX keeps its local data in, created on first use by the
/// stores that write there.
pub fn data_dir(flavor: Flavor) -> PathBuf {
    resolve(flavor, Platform::current(), &|name| std::env::var_os(name))
}

fn resolve(flavor: Flavor, platform: Platform, env: &dyn Fn(&str) -> Option<OsString>) -> PathBuf {
    let get = |name: &str| env(name).filter(|v| !v.is_empty()).map(PathBuf::from);

    if let Some(dir) = get(STATE_DIR_ENV) {
        return dir;
    }
    match platform {
        Platform::Windows => {
            if let Some(local) = get("LOCALAPPDATA") {
                return local.join(APP_DIR);
            }
            if let Some(profile) = get("USERPROFILE") {
                return profile.join("AppData").join("Local").join(APP_DIR);
            }
            if let Some(roaming) = get("APPDATA") {
                return roaming.join(APP_DIR);
            }
        }
        Platform::Unix => match flavor {
            Flavor::Desktop => {
                if let Some(xdg) = get("XDG_CONFIG_HOME") {
                    return xdg.join(APP_DIR);
                }
                if let Some(home) = get("HOME") {
                    return home.join(".config").join(APP_DIR);
                }
            }
            Flavor::Cli => {
                if let Some(home) = get("HOME") {
                    return home.join(".rex").join("harness");
                }
            }
        },
    }
    std::env::temp_dir().join(APP_DIR)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn env(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<OsString> {
        let map: HashMap<String, OsString> = pairs
            .iter()
            .map(|(k, v)| (k.to_string(), OsString::from(v)))
            .collect();
        move |name| map.get(name).cloned()
    }

    fn at(flavor: Flavor, platform: Platform, pairs: &[(&str, &str)]) -> PathBuf {
        resolve(flavor, platform, &env(pairs))
    }

    #[test]
    fn an_explicit_state_dir_wins_everywhere_and_an_empty_one_does_not_count() {
        for platform in [Platform::Windows, Platform::Unix] {
            for flavor in [Flavor::Desktop, Flavor::Cli] {
                let all = [
                    ("REX_STATE_DIR", "/shared/rex"),
                    ("LOCALAPPDATA", "/l"),
                    ("HOME", "/h"),
                ];
                assert_eq!(at(flavor, platform, &all), PathBuf::from("/shared/rex"));
                let empty = [
                    ("REX_STATE_DIR", ""),
                    ("LOCALAPPDATA", "/l"),
                    ("HOME", "/h"),
                ];
                assert_ne!(at(flavor, platform, &empty), PathBuf::from(""));
            }
        }
    }

    #[test]
    fn on_windows_the_app_and_the_cli_share_the_per_user_local_folder() {
        let vars = [
            ("LOCALAPPDATA", "/Users/a/AppData/Local"),
            ("APPDATA", "/r"),
        ];
        let want = PathBuf::from("/Users/a/AppData/Local/rex-harness");
        assert_eq!(at(Flavor::Desktop, Platform::Windows, &vars), want);
        assert_eq!(at(Flavor::Cli, Platform::Windows, &vars), want);
    }

    #[test]
    fn windows_does_not_need_home_and_never_lands_in_temp_or_the_current_directory() {
        // HOME is normally unset on Windows: this is the case that used to
        // send a sign-in to %TEMP% (app) or ./.rex (CLI).
        let profile_only = [("USERPROFILE", "/Users/a")];
        for flavor in [Flavor::Desktop, Flavor::Cli] {
            let dir = at(flavor, Platform::Windows, &profile_only);
            assert_eq!(dir, PathBuf::from("/Users/a/AppData/Local/rex-harness"));
        }
        let roaming_only = [("APPDATA", "/Users/a/AppData/Roaming")];
        assert_eq!(
            at(Flavor::Cli, Platform::Windows, &roaming_only),
            PathBuf::from("/Users/a/AppData/Roaming/rex-harness")
        );
    }

    #[test]
    fn on_unix_each_program_keeps_the_folder_earlier_versions_used() {
        let vars = [("HOME", "/home/a")];
        assert_eq!(
            at(Flavor::Desktop, Platform::Unix, &vars),
            PathBuf::from("/home/a/.config/rex-harness")
        );
        assert_eq!(
            at(Flavor::Cli, Platform::Unix, &vars),
            PathBuf::from("/home/a/.rex/harness")
        );
        let xdg = [("HOME", "/home/a"), ("XDG_CONFIG_HOME", "/home/a/cfg")];
        assert_eq!(
            at(Flavor::Desktop, Platform::Unix, &xdg),
            PathBuf::from("/home/a/cfg/rex-harness")
        );
        // the CLI never honoured XDG; that stays
        assert_eq!(
            at(Flavor::Cli, Platform::Unix, &xdg),
            PathBuf::from("/home/a/.rex/harness")
        );
    }

    #[test]
    fn with_no_home_at_all_it_is_a_temp_folder_and_never_the_current_directory() {
        for platform in [Platform::Windows, Platform::Unix] {
            for flavor in [Flavor::Desktop, Flavor::Cli] {
                let dir = at(flavor, platform, &[]);
                assert!(dir.is_absolute(), "{dir:?}");
                assert!(dir.starts_with(std::env::temp_dir()), "{dir:?}");
                assert!(dir.ends_with("rex-harness"));
            }
        }
    }
}
