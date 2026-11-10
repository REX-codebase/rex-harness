use crate::error::ProviderError;
use crate::protect::{platform_protector, Protector, KNOWN_PREFIXES};
use base64::Engine as _;
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;

/// Credential storage. Keys are write-only from the caller's perspective:
/// they can be stored, read back for an API call inside the backend, and
/// cleared. Nothing upstream ever serializes them.
pub trait SecretStore: Send + Sync {
    fn get_key(&self, provider: &str) -> Result<Option<String>, ProviderError>;
    fn set_key(&self, provider: &str, key: &str) -> Result<(), ProviderError>;
    fn clear_key(&self, provider: &str) -> Result<(), ProviderError>;
    fn has_key(&self, provider: &str) -> bool {
        matches!(self.get_key(provider), Ok(Some(_)))
    }
    /// A directory processes sharing this store can put a lock file in, when
    /// the store is on disk. Used to serialise rotating-token refreshes
    /// across processes (the CLI and the desktop sidecar share one store).
    fn lock_dir(&self) -> Option<std::path::PathBuf> {
        None
    }
}

/// A shared handle to a store is itself a store. The dev sidecar builds one
/// provider service per subsystem (UI, runs, agent); with an in-memory store
/// each used to get its own empty copy, so a key saved in the UI never
/// reached the agent. Cloning one `Arc` gives them all the same store.
impl<S: SecretStore + ?Sized> SecretStore for std::sync::Arc<S> {
    fn get_key(&self, provider: &str) -> Result<Option<String>, ProviderError> {
        (**self).get_key(provider)
    }
    fn set_key(&self, provider: &str, key: &str) -> Result<(), ProviderError> {
        (**self).set_key(provider, key)
    }
    fn clear_key(&self, provider: &str) -> Result<(), ProviderError> {
        (**self).clear_key(provider)
    }
    fn has_key(&self, provider: &str) -> bool {
        (**self).has_key(provider)
    }
    fn lock_dir(&self) -> Option<std::path::PathBuf> {
        (**self).lock_dir()
    }
}

/// In-memory store. Used by tests and by the dev sidecar when no config
/// directory is available. Keys vanish when the process exits.
#[derive(Default)]
pub struct MemorySecretStore {
    inner: Mutex<HashMap<String, String>>,
}

impl MemorySecretStore {
    pub fn new() -> Self {
        Self::default()
    }
}

impl SecretStore for MemorySecretStore {
    fn get_key(&self, provider: &str) -> Result<Option<String>, ProviderError> {
        let map = self
            .inner
            .lock()
            .map_err(|e| ProviderError::Store(e.to_string()))?;
        Ok(map.get(provider).cloned())
    }
    fn set_key(&self, provider: &str, key: &str) -> Result<(), ProviderError> {
        let mut map = self
            .inner
            .lock()
            .map_err(|e| ProviderError::Store(e.to_string()))?;
        map.insert(provider.to_string(), key.to_string());
        Ok(())
    }
    fn clear_key(&self, provider: &str) -> Result<(), ProviderError> {
        let mut map = self
            .inner
            .lock()
            .map_err(|e| ProviderError::Store(e.to_string()))?;
        map.remove(provider);
        Ok(())
    }
}

/// File-backed store: one JSON map at `<dir>/secrets.json`, permissions
/// 0600, directory 0700. Keys never enter logs, and the file lives outside
/// the frontend bundle and the repo. On Windows, where a file mode protects
/// nothing, each value is also encrypted with DPAPI for the signed-in account
/// (see [`crate::protect`]); on macOS and Linux the 0600 file is the
/// protection. The store is local to the machine: nothing here talks to a
/// server.
pub struct FileSecretStore {
    path: PathBuf,
    inner: Mutex<()>,
    /// Seals each value before it is written; `None` where the platform's
    /// file permissions are the protection.
    protector: Option<Box<dyn Protector>>,
}

impl FileSecretStore {
    /// Open (creating if needed) the store in `dir`, protected the way this
    /// platform protects it. Values saved in plaintext by an earlier version
    /// are sealed the first time a protected store opens them.
    pub fn new(dir: PathBuf) -> Result<Self, ProviderError> {
        Self::with_protector(dir, platform_protector())
    }

    /// Like [`new`](Self::new) with an explicit protector (or none): a
    /// different protection scheme, or a test.
    pub fn with_protector(
        dir: PathBuf,
        protector: Option<Box<dyn Protector>>,
    ) -> Result<Self, ProviderError> {
        let store = Self::open_dir(dir, protector)?;
        store.seal_plaintext_values()?;
        Ok(store)
    }

    fn open_dir(
        dir: PathBuf,
        protector: Option<Box<dyn Protector>>,
    ) -> Result<Self, ProviderError> {
        fs::create_dir_all(&dir).map_err(|e| ProviderError::Store(e.to_string()))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&dir, fs::Permissions::from_mode(0o700))
                .map_err(|e| ProviderError::Store(e.to_string()))?;
        }
        Ok(Self {
            path: dir.join("secrets.json"),
            inner: Mutex::new(()),
            protector,
        })
    }

    /// The form a value takes in the file.
    fn seal(&self, plain: &str) -> Result<String, ProviderError> {
        match &self.protector {
            Some(p) => {
                let sealed = p.seal(plain.as_bytes())?;
                Ok(format!(
                    "{}{}",
                    p.prefix(),
                    base64::engine::general_purpose::STANDARD.encode(sealed)
                ))
            }
            None => Ok(plain.to_string()),
        }
    }

    /// The credential a stored value stands for.
    fn open(&self, stored: &str) -> Result<String, ProviderError> {
        let sealed_by = |prefix: &str| stored.strip_prefix(prefix);
        if let Some(p) = &self.protector {
            if let Some(body) = sealed_by(p.prefix()) {
                let bytes = base64::engine::general_purpose::STANDARD
                    .decode(body)
                    .map_err(|_| {
                        ProviderError::Store("a saved credential is damaged".to_string())
                    })?;
                let plain = p.open(&bytes)?;
                return String::from_utf8(plain)
                    .map_err(|_| ProviderError::Store("a saved credential is damaged".into()));
            }
        } else if KNOWN_PREFIXES.iter().any(|k| stored.starts_with(k)) {
            return Err(ProviderError::Store(
                "this credential is encrypted for a Windows account and cannot be read here; connect again".into(),
            ));
        }
        Ok(stored.to_string())
    }

    /// Seal any value an older version left in plaintext.
    fn seal_plaintext_values(&self) -> Result<(), ProviderError> {
        let Some(p) = &self.protector else {
            return Ok(());
        };
        let _guard = self
            .inner
            .lock()
            .map_err(|e| ProviderError::Store(e.to_string()))?;
        let mut map = self.read_map()?;
        let mut changed = false;
        for value in map.values_mut() {
            if !value.starts_with(p.prefix()) {
                *value = self.seal(value)?;
                changed = true;
            }
        }
        if changed {
            self.write_map(&map)?;
        }
        Ok(())
    }

    fn read_map(&self) -> Result<HashMap<String, String>, ProviderError> {
        match fs::read_to_string(&self.path) {
            Ok(text) => {
                serde_json::from_str(&text).map_err(|e| ProviderError::Store(e.to_string()))
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(HashMap::new()),
            Err(e) => Err(ProviderError::Store(e.to_string())),
        }
    }

    fn write_map(&self, map: &HashMap<String, String>) -> Result<(), ProviderError> {
        let text = serde_json::to_string(map).map_err(|e| ProviderError::Store(e.to_string()))?;
        // A temp file that is 0600 from creation, synced, then renamed over
        // the store: a crash mid-write keeps the old keys instead of leaving
        // a broken file, and the keys are never in a file others could
        // read. Hermes saves credentials the same way (`atomic_json_write`
        // with `mode=0o600`, `utils.py`).
        let tmp = self
            .path
            .with_file_name(format!(".secrets.{}.tmp", std::process::id()));
        let written = (|| {
            let mut options = fs::OpenOptions::new();
            options.write(true).create(true).truncate(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                options.mode(0o600);
            }
            let mut f = options.open(&tmp)?;
            #[cfg(unix)]
            {
                // a leftover temp file keeps its old mode; reset it
                use std::os::unix::fs::PermissionsExt;
                f.set_permissions(fs::Permissions::from_mode(0o600))?;
            }
            std::io::Write::write_all(&mut f, text.as_bytes())?;
            f.sync_all()?;
            fs::rename(&tmp, &self.path)
        })();
        if written.is_err() {
            let _ = fs::remove_file(&tmp);
        }
        written.map_err(|e| ProviderError::Store(e.to_string()))?;
        Ok(())
    }
}

impl SecretStore for FileSecretStore {
    fn lock_dir(&self) -> Option<std::path::PathBuf> {
        self.path.parent().map(std::path::Path::to_path_buf)
    }
    fn get_key(&self, provider: &str) -> Result<Option<String>, ProviderError> {
        let _guard = self
            .inner
            .lock()
            .map_err(|e| ProviderError::Store(e.to_string()))?;
        match self.read_map()?.get(provider) {
            Some(stored) => self.open(stored).map(Some),
            None => Ok(None),
        }
    }
    fn set_key(&self, provider: &str, key: &str) -> Result<(), ProviderError> {
        let _guard = self
            .inner
            .lock()
            .map_err(|e| ProviderError::Store(e.to_string()))?;
        let mut map = self.read_map()?;
        map.insert(provider.to_string(), self.seal(key)?);
        self.write_map(&map)
    }
    fn clear_key(&self, provider: &str) -> Result<(), ProviderError> {
        let _guard = self
            .inner
            .lock()
            .map_err(|e| ProviderError::Store(e.to_string()))?;
        let mut map = self.read_map()?;
        map.remove(provider);
        self.write_map(&map)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    fn mode(p: &std::path::Path) -> u32 {
        use std::os::unix::fs::PermissionsExt;
        fs::metadata(p).unwrap().permissions().mode() & 0o777
    }

    #[test]
    fn keys_round_trip_and_the_store_stays_private() {
        let tmp = tempfile::tempdir().unwrap();
        let store = FileSecretStore::new(tmp.path().join("s")).unwrap();
        let path = tmp.path().join("s").join("secrets.json");
        // an old store left readable, and a leftover temp file from a crash
        fs::write(&path, "{\"old\":\"k0\"}").unwrap();
        let leftover = path.with_file_name(format!(".secrets.{}.tmp", std::process::id()));
        fs::write(&leftover, "stale").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();
            fs::set_permissions(&leftover, fs::Permissions::from_mode(0o644)).unwrap();
        }
        store.set_key("a", "k1").unwrap();
        // the first save reused the readable leftover; it is private now
        #[cfg(unix)]
        assert_eq!(mode(&path), 0o600);
        store.set_key("b", "k2").unwrap();
        store.clear_key("old").unwrap();
        assert_eq!(store.get_key("a").unwrap().as_deref(), Some("k1"));
        assert_eq!(store.get_key("b").unwrap().as_deref(), Some("k2"));
        assert_eq!(store.get_key("old").unwrap(), None);
        #[cfg(unix)]
        assert_eq!(mode(&path), 0o600);
        let names: Vec<String> = fs::read_dir(tmp.path().join("s"))
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names, ["secrets.json"]);
    }

    #[cfg(unix)]
    #[test]
    fn a_failed_rename_removes_the_temp_file() {
        // the write is stopped at the rename by a folder in the way
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("s");
        let store = FileSecretStore::new(dir.clone()).unwrap();
        let leftover = dir.join(format!(".secrets.{}.tmp", std::process::id()));
        fs::write(&leftover, "stale").unwrap();
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&leftover, fs::Permissions::from_mode(0o644)).unwrap();
        }
        // secrets.json as a non-empty folder makes the rename fail
        fs::create_dir(dir.join("secrets.json")).unwrap();
        fs::write(dir.join("secrets.json").join("x"), "").unwrap();
        let fresh = FileSecretStore {
            path: dir.join("secrets.json"),
            inner: Mutex::new(()),
            protector: None,
        };
        let mut map = HashMap::new();
        map.insert("a".to_string(), "k1".to_string());
        assert!(fresh.write_map(&map).is_err());
        // the failed write removed its temp file
        assert!(!leftover.exists());
        drop(store);
    }

    #[cfg(unix)]
    #[test]
    fn a_failed_save_keeps_the_old_keys() {
        use std::os::unix::fs::PermissionsExt;
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("s");
        let store = FileSecretStore::new(dir.clone()).unwrap();
        store.set_key("a", "k1").unwrap();
        // no new files can be made in the folder
        fs::set_permissions(&dir, fs::Permissions::from_mode(0o500)).unwrap();
        // Root (a container, some CI) ignores directory permissions, so the
        // write below would succeed and prove nothing. Say so and stop.
        if fs::File::create(dir.join("probe")).is_ok() {
            fs::set_permissions(&dir, fs::Permissions::from_mode(0o700)).unwrap();
            eprintln!("skipped: this user can write into a read-only folder (running as root?)");
            return;
        }
        let failed = store.set_key("b", "k2").is_err();
        fs::set_permissions(&dir, fs::Permissions::from_mode(0o700)).unwrap();
        assert!(failed);
        assert_eq!(store.get_key("a").unwrap().as_deref(), Some("k1"));
        assert_eq!(store.get_key("b").unwrap(), None);
    }

    // ---- protected values (DPAPI on Windows; a stand-in here) -----------------

    use crate::protect::TestProtector;

    fn protected(dir: &std::path::Path) -> FileSecretStore {
        FileSecretStore::with_protector(dir.to_path_buf(), Some(Box::new(TestProtector))).unwrap()
    }

    fn file_text(dir: &std::path::Path) -> String {
        fs::read_to_string(dir.join("secrets.json")).unwrap()
    }

    #[test]
    fn a_protected_store_never_writes_a_value_in_the_clear_and_reads_it_back() {
        let tmp = tempfile::tempdir().unwrap();
        let store = protected(tmp.path());
        store.set_key("gemini", "AIza-secret-value").unwrap();
        store
            .set_key("openai-chatgpt#oauth", "{\"refresh\":\"rt-secret\"}")
            .unwrap();
        let on_disk = file_text(tmp.path());
        for secret in ["AIza-secret-value", "rt-secret"] {
            assert!(!on_disk.contains(secret), "{secret} is in the file");
        }
        assert!(on_disk.contains("dpapi1:"));
        assert_eq!(
            store.get_key("gemini").unwrap().as_deref(),
            Some("AIza-secret-value")
        );
        assert_eq!(
            store.get_key("openai-chatgpt#oauth").unwrap().as_deref(),
            Some("{\"refresh\":\"rt-secret\"}")
        );
        assert!(store.has_key("gemini") && !store.has_key("xai"));
        store.clear_key("gemini").unwrap();
        assert_eq!(store.get_key("gemini").unwrap(), None);
        // values saved by one handle are read by a fresh one (another process)
        assert_eq!(
            protected(tmp.path())
                .get_key("openai-chatgpt#oauth")
                .unwrap()
                .as_deref(),
            Some("{\"refresh\":\"rt-secret\"}")
        );
    }

    #[test]
    fn plaintext_left_by_an_older_version_is_read_then_sealed_when_the_store_opens() {
        let tmp = tempfile::tempdir().unwrap();
        // what v0.1.0 wrote on Windows: plaintext values
        fs::write(
            tmp.path().join("secrets.json"),
            r#"{"gemini":"AIza-old-plain","xai#base_url":"https://example.test/v1"}"#,
        )
        .unwrap();
        // without a protector it is still plain and still works
        let plain = FileSecretStore::with_protector(tmp.path().to_path_buf(), None).unwrap();
        assert_eq!(
            plain.get_key("gemini").unwrap().as_deref(),
            Some("AIza-old-plain")
        );
        assert!(file_text(tmp.path()).contains("AIza-old-plain"));
        drop(plain);
        // opening it with a protector seals every value, losing none
        let store = protected(tmp.path());
        let on_disk = file_text(tmp.path());
        assert!(!on_disk.contains("AIza-old-plain") && !on_disk.contains("example.test"));
        assert_eq!(
            store.get_key("gemini").unwrap().as_deref(),
            Some("AIza-old-plain")
        );
        assert_eq!(
            store.get_key("xai#base_url").unwrap().as_deref(),
            Some("https://example.test/v1")
        );
        // and opening it again changes nothing
        let before = file_text(tmp.path());
        let _again = protected(tmp.path());
        assert_eq!(file_text(tmp.path()), before);
    }

    #[test]
    fn a_sealed_file_carried_to_a_system_that_cannot_open_it_explains_and_leaks_nothing() {
        let tmp = tempfile::tempdir().unwrap();
        protected(tmp.path())
            .set_key("gemini", "AIza-secret-value")
            .unwrap();
        let elsewhere = FileSecretStore::with_protector(tmp.path().to_path_buf(), None).unwrap();
        let err = elsewhere.get_key("gemini").unwrap_err().to_string();
        assert!(err.contains("encrypted"), "{err}");
        assert!(!err.contains("AIza-secret-value"));
        assert!(
            !elsewhere.has_key("gemini"),
            "an unreadable credential is not a usable one"
        );
        // and it is not handed out as if it were the key
        assert!(!matches!(elsewhere.get_key("gemini"), Ok(Some(_))));
    }

    #[test]
    fn a_damaged_sealed_value_is_an_error_not_a_panic_or_a_wrong_key() {
        let tmp = tempfile::tempdir().unwrap();
        let store = protected(tmp.path());
        store.set_key("gemini", "AIza-secret-value").unwrap();
        for broken in ["dpapi1:!!!not base64!!!", "dpapi1:", "dpapi1:AAAA"] {
            fs::write(
                tmp.path().join("secrets.json"),
                format!(r#"{{"gemini":"{broken}"}}"#),
            )
            .unwrap();
            let result = store.get_key("gemini");
            assert!(
                !matches!(&result, Ok(Some(v)) if v.contains("AIza")),
                "{broken}: {result:?}"
            );
            assert!(!store.has_key("gemini") || result.is_ok());
        }
    }

    #[cfg(unix)]
    #[test]
    fn a_protected_file_is_still_private_to_the_user() {
        let tmp = tempfile::tempdir().unwrap();
        let store = protected(tmp.path());
        store.set_key("a", "k1").unwrap();
        assert_eq!(mode(&tmp.path().join("secrets.json")), 0o600);
    }

    #[test]
    fn the_platform_protector_is_dpapi_on_windows_and_none_elsewhere() {
        let p = crate::protect::platform_protector();
        if cfg!(windows) {
            assert_eq!(p.expect("DPAPI on Windows").prefix(), "dpapi1:");
        } else {
            assert!(p.is_none(), "the 0600 file is the protection off Windows");
        }
    }

    /// On Windows this exercises real DPAPI through the store.
    #[test]
    fn the_default_store_round_trips_on_every_platform() {
        let tmp = tempfile::tempdir().unwrap();
        let store = FileSecretStore::new(tmp.path().to_path_buf()).unwrap();
        store.set_key("gemini", "AIza-secret-value").unwrap();
        assert_eq!(
            FileSecretStore::new(tmp.path().to_path_buf())
                .unwrap()
                .get_key("gemini")
                .unwrap()
                .as_deref(),
            Some("AIza-secret-value")
        );
        if cfg!(windows) {
            assert!(!file_text(tmp.path()).contains("AIza-secret-value"));
        }
    }
}
