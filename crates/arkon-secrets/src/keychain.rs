//! OS keychain session caching for vault passphrase.
//! macOS Keychain · Linux Secret Service · Windows Credential Manager

use arkon_core::error::{ArkonError, Result};
use keyring::Entry;
use zeroize::Zeroizing;

const SERVICE: &str = "arkon-vault";

pub enum VaultCredential {
    /// Raw 32-byte key from ARKON_VAULT_KEY — skips Argon2id entirely
    RawKey(Zeroizing<[u8; 32]>),
    /// User passphrase — fed into Argon2id + machine fingerprint KDF
    Passphrase(Zeroizing<Vec<u8>>),
}

/// Get vault credential for `project`. Priority:
///   1. ARKON_VAULT_KEY env var (CI/headless)
///   2. OS keychain session cache
///   3. Interactive passphrase prompt
pub fn get_passphrase(project: &str) -> Result<VaultCredential> {
    // CI/headless: raw key bypass
    if let Ok(hex) = std::env::var("ARKON_VAULT_KEY") {
        let bytes = hex::decode(hex.trim())
            .map_err(|_| ArkonError::VaultError("ARKON_VAULT_KEY must be 64 hex chars".into()))?;
        if bytes.len() != 32 {
            return Err(ArkonError::VaultError("ARKON_VAULT_KEY must be 32 bytes".into()));
        }
        let mut key = Zeroizing::new([0u8; 32]);
        key.copy_from_slice(&bytes);
        return Ok(VaultCredential::RawKey(key));
    }

    // OS keychain session cache
    if let Ok(entry) = Entry::new(SERVICE, project) {
        if let Ok(cached) = entry.get_password() {
            if !cached.is_empty() {
                tracing::debug!(project = %project, "using cached passphrase from keychain");
                return Ok(VaultCredential::Passphrase(Zeroizing::new(cached.into_bytes())));
            }
        }
    }

    // Interactive prompt
    let passphrase = prompt(project)?;

    // Cache in OS keychain
    let pass_str = String::from_utf8_lossy(&passphrase).into_owned();
    if let Ok(entry) = Entry::new(SERVICE, project) {
        if entry.set_password(&pass_str).is_err() {
            tracing::warn!("could not cache passphrase in OS keychain — will re-prompt next time");
        }
    }

    Ok(VaultCredential::Passphrase(passphrase))
}

/// Clear the session passphrase from the OS keychain.
pub fn clear_session(project: &str) {
    if let Ok(entry) = Entry::new(SERVICE, project) {
        let _ = entry.delete_password();
        tracing::info!(project = %project, "vault session cleared from keychain");
    }
}

fn prompt(project: &str) -> Result<Zeroizing<Vec<u8>>> {
    let pass = dialoguer::Password::new()
        .with_prompt(format!("ARKON vault passphrase for '{project}'"))
        .interact()
        .map_err(|e| ArkonError::VaultError(format!("passphrase prompt: {e}")))?;
    if pass.is_empty() {
        return Err(ArkonError::VaultError(
            "passphrase cannot be empty — run `arkon secrets init` to set one".into()
        ));
    }
    Ok(Zeroizing::new(pass.into_bytes()))
}
