//! ARKON Secrets Vault — hardened dual-factor encryption
//!
//! Key derivation:
//!   Argon2id(user_passphrase, SHA-256(machine_fingerprint || vault_salt))
//!
//! The machine fingerprint is a salt, NOT the sole secret.
//! Passphrase is the blind factor — machine-id exposure does not compromise vault.
//!
//! File permissions: ~/.arkon/ 0700, *.vault 0600

pub mod keychain;
pub mod mnemonic;
pub mod vault;

pub use vault::Vault;
pub use keychain::{VaultCredential, get_passphrase, clear_session};

use aes_gcm::{
    aead::{Aead, KeyInit},
    aead::rand_core::RngCore,
    Aes256Gcm, Key, Nonce,
};
use aes_gcm::aead::OsRng;
use argon2::{Argon2, Algorithm, Params, Version};
use arkon_core::error::{ArkonError, Result};
use sha2::{Digest, Sha256};
use std::path::PathBuf;
use zeroize::Zeroizing;

pub(crate) const NONCE_LEN: usize = 12;
pub(crate) const KEY_LEN:   usize = 32;
pub(crate) const SALT_LEN:  usize = 32;

pub(crate) fn argon2_kdf() -> Argon2<'static> {
    Argon2::new(
        Algorithm::Argon2id,
        Version::V0x13,
        Params::new(65_536, 3, 4, Some(KEY_LEN)).expect("argon2 params"),
    )
}

/// Derive vault key: Argon2id(passphrase, SHA-256(machine_fingerprint || vault_salt))
pub(crate) fn derive_key(
    passphrase: &[u8],
    vault_salt: &[u8; SALT_LEN],
) -> Result<Zeroizing<[u8; KEY_LEN]>> {
    let fp = machine_fingerprint_bytes();
    let mut h = Sha256::new();
    h.update(&fp);
    h.update(vault_salt.as_ref());
    let combined = h.finalize();

    let mut key = Zeroizing::new([0u8; KEY_LEN]);
    argon2_kdf()
        .hash_password_into(passphrase, &combined, key.as_mut())
        .map_err(|e| ArkonError::VaultError(format!("Argon2id failed: {e}")))?;
    Ok(key)
}

pub(crate) fn aes_encrypt(
    key: &[u8; KEY_LEN],
    plaintext: &[u8],
) -> Result<([u8; NONCE_LEN], Vec<u8>)> {
    let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(key));
    let mut nonce_bytes = [0u8; NONCE_LEN];
    OsRng.fill_bytes(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);
    let ct = cipher.encrypt(nonce, plaintext)
        .map_err(|e| ArkonError::VaultError(format!("encrypt: {e}")))?;
    Ok((nonce_bytes, ct))
}

pub(crate) fn aes_decrypt(
    key: &[u8; KEY_LEN],
    nonce: &[u8; NONCE_LEN],
    ciphertext: &[u8],
) -> Result<Zeroizing<Vec<u8>>> {
    let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(key));
    let nonce  = Nonce::from_slice(nonce);
    cipher.decrypt(nonce, ciphertext)
        .map(Zeroizing::new)
        .map_err(|_| ArkonError::VaultError(
            "decryption failed — wrong passphrase or corrupted vault".into()
        ))
}

/// Machine fingerprint as 32 bytes (SHA-256 of raw string).
/// This is a SALT component — world-readable on most platforms.
pub(crate) fn machine_fingerprint_bytes() -> [u8; 32] {
    let raw = machine_fingerprint_raw();
    let mut h = Sha256::new();
    h.update(raw.as_bytes());
    h.finalize().into()
}

pub fn machine_fingerprint_raw() -> String {
    let user = std::env::var("USER")
        .or_else(|_| std::env::var("USERNAME"))
        .unwrap_or_else(|_| "arkon".into());

    if let Ok(id) = std::fs::read_to_string("/etc/machine-id") {
        let id = id.trim().to_string();
        if !id.is_empty() { return format!("linux:{id}:{user}"); }
    }

    #[cfg(target_os = "macos")]
    if let Ok(out) = std::process::Command::new("ioreg")
        .args(["-rd1", "-c", "IOPlatformExpertDevice"]).output()
    {
        let text = String::from_utf8_lossy(&out.stdout);
        if let Some(uuid) = text.lines()
            .find(|l| l.contains("IOPlatformUUID"))
            .and_then(|l| l.split('"').nth(3))
        {
            return format!("macos:{uuid}:{user}");
        }
    }

    #[cfg(target_os = "windows")]
    if let Ok(out) = std::process::Command::new("reg")
        .args(["query", "HKEY_LOCAL_MACHINE\\SOFTWARE\\Microsoft\\Cryptography", "/v", "MachineGuid"])
        .output()
    {
        let text = String::from_utf8_lossy(&out.stdout);
        if let Some(guid) = text.lines()
            .find(|l| l.contains("MachineGuid"))
            .and_then(|l| l.split_whitespace().last())
        {
            return format!("win:{guid}:{user}");
        }
    }

    let home = dirs::home_dir()
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_else(|| "unknown".into());
    tracing::warn!("using software machine fingerprint — vault is not hardware-assisted");
    format!("sw:{user}:{home}")
}

pub(crate) fn vault_dir() -> Result<PathBuf> {
    let base = dirs::home_dir()
        .ok_or_else(|| ArkonError::VaultError("cannot find home directory".into()))?;
    let arkon = base.join(".arkon");
    let dir   = arkon.join("vault");
    std::fs::create_dir_all(&dir)?;
    set_dir_0700(&arkon);
    set_dir_0700(&dir);
    Ok(dir)
}

pub(crate) fn sanitize(name: &str) -> String {
    name.chars()
        .map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c } else { '_' })
        .collect()
}

fn set_dir_0700(path: &PathBuf) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700));
    }
}
