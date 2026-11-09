//! Protecting a saved credential at rest.
//!
//! [`FileSecretStore`](crate::secrets::FileSecretStore) keeps its values in one
//! JSON file. On macOS and Linux that file is private to the user (mode 0600),
//! which is the platform's own protection. Windows has no equivalent of a 0600
//! file, so on Windows each value is encrypted with the **Windows Data
//! Protection API (DPAPI)** for the signed-in Windows account before it is
//! written. The encryption key never leaves Windows and is not stored by REX.
//!
//! What that gives you: another Windows user, a backup, a synced folder, or a
//! copied `secrets.json` cannot read the saved API keys or the ChatGPT
//! sign-in. What it does not: stop a program that is already running as you,
//! which can ask DPAPI to decrypt for itself, exactly as REX does. See
//! `docs/security.md`.
//!
//! A protected value is stored as `<prefix><base64 of the sealed bytes>`
//! (`dpapi1:` for DPAPI). A value without a known prefix is a plaintext value
//! from an older version and is still read; it is sealed the next time the
//! store is opened.

use crate::error::ProviderError;

/// Seals and opens one secret value. Implementations must never put the
/// plaintext in an error message.
pub trait Protector: Send + Sync {
    /// Marks the sealed values this protector produces. Must end with `:` and
    /// not occur at the start of any real credential.
    fn prefix(&self) -> &'static str;
    fn seal(&self, plain: &[u8]) -> Result<Vec<u8>, ProviderError>;
    fn open(&self, sealed: &[u8]) -> Result<Vec<u8>, ProviderError>;
}

/// Prefixes of every sealed format REX has written, so a file carried to a
/// system that cannot open it fails with an explanation instead of handing the
/// ciphertext to an API as if it were a key.
pub(crate) const KNOWN_PREFIXES: &[&str] = &["dpapi1:"];

/// The protector for this platform: DPAPI on Windows, none elsewhere (the
/// 0600 file is the protection there).
pub fn platform_protector() -> Option<Box<dyn Protector>> {
    #[cfg(windows)]
    {
        Some(Box::new(windows::Dpapi))
    }
    #[cfg(not(windows))]
    {
        None
    }
}

#[cfg(windows)]
mod windows {
    use super::Protector;
    use crate::error::ProviderError;
    use windows_sys::Win32::Foundation::{GetLastError, LocalFree};
    use windows_sys::Win32::Security::Cryptography::{
        CryptProtectData, CryptUnprotectData, CRYPTPROTECT_UI_FORBIDDEN, CRYPT_INTEGER_BLOB,
    };

    /// Extra data mixed into the key derivation, so a blob made for REX is not
    /// opened by a program that calls DPAPI with no context. A label, not a
    /// secret.
    const ENTROPY: &[u8] = b"rex-harness/credential/v1";

    /// DPAPI, scoped to the current Windows user.
    pub struct Dpapi;

    fn blob(data: &[u8]) -> CRYPT_INTEGER_BLOB {
        CRYPT_INTEGER_BLOB {
            cbData: data.len() as u32,
            pbData: data.as_ptr() as *mut u8,
        }
    }

    /// Copy the bytes DPAPI allocated and release them.
    ///
    /// # Safety
    /// `out` must have been filled in by a successful DPAPI call.
    unsafe fn take(out: CRYPT_INTEGER_BLOB) -> Vec<u8> {
        let bytes = std::slice::from_raw_parts(out.pbData, out.cbData as usize).to_vec();
        LocalFree(out.pbData as _);
        bytes
    }

    impl Protector for Dpapi {
        fn prefix(&self) -> &'static str {
            "dpapi1:"
        }

        fn seal(&self, plain: &[u8]) -> Result<Vec<u8>, ProviderError> {
            let (input, entropy) = (blob(plain), blob(ENTROPY));
            let mut out = CRYPT_INTEGER_BLOB {
                cbData: 0,
                pbData: std::ptr::null_mut(),
            };
            // SAFETY: the blobs point at live slices for the duration of the
            // call; `out` is written by DPAPI and released by `take`.
            let ok = unsafe {
                CryptProtectData(
                    &input,
                    std::ptr::null(),
                    &entropy,
                    std::ptr::null(),
                    std::ptr::null(),
                    CRYPTPROTECT_UI_FORBIDDEN,
                    &mut out,
                )
            };
            if ok == 0 {
                // SAFETY: reads the calling thread's last-error value.
                let code = unsafe { GetLastError() };
                return Err(ProviderError::Store(format!(
                    "Windows could not encrypt the credential (error {code})"
                )));
            }
            // SAFETY: the call succeeded.
            Ok(unsafe { take(out) })
        }

        fn open(&self, sealed: &[u8]) -> Result<Vec<u8>, ProviderError> {
            let (input, entropy) = (blob(sealed), blob(ENTROPY));
            let mut out = CRYPT_INTEGER_BLOB {
                cbData: 0,
                pbData: std::ptr::null_mut(),
            };
            // SAFETY: as in `seal`.
            let ok = unsafe {
                CryptUnprotectData(
                    &input,
                    std::ptr::null_mut(),
                    &entropy,
                    std::ptr::null(),
                    std::ptr::null(),
                    CRYPTPROTECT_UI_FORBIDDEN,
                    &mut out,
                )
            };
            if ok == 0 {
                // SAFETY: reads the calling thread's last-error value.
                let code = unsafe { GetLastError() };
                return Err(ProviderError::Store(format!(
                    "Windows could not decrypt the saved credential (error {code}); it was saved by a different Windows account or machine. Disconnect and connect again"
                )));
            }
            // SAFETY: the call succeeded.
            Ok(unsafe { take(out) })
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn a_value_round_trips_and_the_sealed_bytes_do_not_contain_it() {
            let secret = b"sk-test-0123456789-not-a-real-key";
            let sealed = Dpapi.seal(secret).unwrap();
            assert_ne!(sealed, secret);
            assert!(
                !sealed.windows(secret.len()).any(|w| w == secret),
                "the ciphertext holds the plaintext"
            );
            assert_eq!(Dpapi.open(&sealed).unwrap(), secret);
            // sealing is salted: the same value seals differently each time
            assert_ne!(Dpapi.seal(secret).unwrap(), sealed);
        }

        #[test]
        fn empty_and_large_values_round_trip() {
            for value in [Vec::new(), vec![b'x'; 64 * 1024]] {
                let sealed = Dpapi.seal(&value).unwrap();
                assert_eq!(Dpapi.open(&sealed).unwrap(), value);
            }
        }

        #[test]
        fn a_damaged_blob_is_an_error_and_never_a_panic_or_a_wrong_value() {
            let mut sealed = Dpapi.seal(b"secret").unwrap();
            let last = sealed.len() - 1;
            sealed[last] ^= 0xff;
            assert!(Dpapi.open(&sealed).is_err());
            assert!(Dpapi.open(b"not a dpapi blob").is_err());
        }

        #[test]
        fn a_blob_sealed_without_rexs_context_is_refused() {
            // what another program calling DPAPI with no entropy would produce
            let input = blob(b"secret");
            let mut out = CRYPT_INTEGER_BLOB {
                cbData: 0,
                pbData: std::ptr::null_mut(),
            };
            let ok = unsafe {
                CryptProtectData(
                    &input,
                    std::ptr::null(),
                    std::ptr::null(),
                    std::ptr::null(),
                    std::ptr::null(),
                    CRYPTPROTECT_UI_FORBIDDEN,
                    &mut out,
                )
            };
            assert_ne!(ok, 0);
            let foreign = unsafe { take(out) };
            assert!(Dpapi.open(&foreign).is_err());
        }
    }
}

/// A reversible stand-in used by tests on every platform to exercise the
/// store's sealing, migration and failure paths without Windows.
#[cfg(test)]
pub(crate) struct TestProtector;

#[cfg(test)]
impl Protector for TestProtector {
    fn prefix(&self) -> &'static str {
        "dpapi1:"
    }
    fn seal(&self, plain: &[u8]) -> Result<Vec<u8>, ProviderError> {
        Ok(plain.iter().rev().map(|b| b ^ 0x5a).collect())
    }
    fn open(&self, sealed: &[u8]) -> Result<Vec<u8>, ProviderError> {
        if sealed.is_empty() {
            return Err(ProviderError::Store("empty sealed value".into()));
        }
        Ok(sealed.iter().rev().map(|b| b ^ 0x5a).collect())
    }
}
