//! Vault — AES-256-GCM with dual-factor key derivation and BIP39 recovery.
use crate::{
    KEY_LEN, NONCE_LEN, SALT_LEN,
    aes_decrypt, aes_encrypt, derive_key, mnemonic, sanitize, vault_dir,
    keychain::{VaultCredential, clear_session, get_passphrase},
};
use arkon_core::error::{ArkonError, Result};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, path::PathBuf};
use zeroize::Zeroizing;

#[derive(Debug, Default, Serialize, Deserialize)]
struct VaultFile {
    version: u8,
    project: String,
    salt:    String,  // hex-encoded [u8; SALT_LEN]
    entries: HashMap<String, EncryptedEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct EncryptedEntry {
    nonce:      String,  // hex [u8; NONCE_LEN]
    ciphertext: String,  // base64
}

pub struct Vault {
    pub project:    String,
    pub vault_path: PathBuf,
    key:            Zeroizing<[u8; KEY_LEN]>,
    salt:           [u8; SALT_LEN],
}

impl Vault {
    pub fn open(project: &str) -> Result<Self> {
        let dir   = vault_dir()?;
        let path  = dir.join(format!("{}.vault", sanitize(project)));
        let cred  = get_passphrase(project)?;
        if path.exists() {
            Self::open_existing(project, &path, cred)
        } else {
            Self::create_new(project, &path, cred)
        }
    }

    pub fn open_with_key(project: &str, raw_key: Zeroizing<[u8; KEY_LEN]>) -> Result<Self> {
        let dir  = vault_dir()?;
        let path = dir.join(format!("{}.vault", sanitize(project)));
        let salt = if path.exists() {
            let vf = Self::load_file(&path)?;
            hex_to_salt(&vf.salt)?
        } else {
            random_salt()
        };
        Ok(Self { project: project.to_string(), vault_path: path, key: raw_key, salt })
    }

    fn open_existing(project: &str, path: &PathBuf, cred: VaultCredential) -> Result<Self> {
        let vf   = Self::load_file(path)?;
        let salt = hex_to_salt(&vf.salt)?;
        let key  = match cred {
            VaultCredential::RawKey(k)   => k,
            VaultCredential::Passphrase(p) => derive_key(&p, &salt)?,
        };
        // Verify key by attempting to decrypt any entry
        if let Some((_, entry)) = vf.entries.iter().next() {
            let n = hex_to_nonce(&entry.nonce)?;
            let c = base64_decode(&entry.ciphertext)?;
            aes_decrypt(&key, &n, &c).map_err(|_| {
                ArkonError::VaultError("wrong passphrase — decryption failed".into())
            })?;
        }
        Ok(Self { project: project.to_string(), vault_path: path.clone(), key, salt })
    }

    fn create_new(project: &str, path: &PathBuf, cred: VaultCredential) -> Result<Self> {
        let salt = random_salt();
        let key  = match cred {
            VaultCredential::RawKey(k)   => k,
            VaultCredential::Passphrase(p) => derive_key(&p, &salt)?,
        };
        // Display recovery mnemonic on first creation
        if let Ok(m) = mnemonic::key_to_mnemonic(&key) {
            mnemonic::display_recovery_mnemonic(&m);
        }
        let vault = Self { project: project.to_string(), vault_path: path.clone(), key, salt };
        vault.save_file(&VaultFile {
            version: 1,
            project: project.to_string(),
            salt: hex::encode(salt),
            entries: HashMap::new(),
        })?;
        tracing::info!(project = %project, "vault created with passphrase protection");
        Ok(vault)
    }

    // ── CRUD ──────────────────────────────────────────────────────────────

    pub fn set(&self, key: &str, value: &[u8]) -> Result<()> {
        let mut vf = self.load_or_default()?;
        let (nonce, ct) = aes_encrypt(&self.key, value)?;
        vf.entries.insert(key.to_string(), EncryptedEntry {
            nonce:      hex::encode(nonce),
            ciphertext: base64_encode(&ct),
        });
        self.save_file(&vf)
    }

    pub fn get(&self, key: &str) -> Result<Zeroizing<Vec<u8>>> {
        let vf    = self.load_or_default()?;
        let entry = vf.entries.get(key)
            .ok_or_else(|| ArkonError::VaultError(format!("secret '{key}' not found")))?;
        let n = hex_to_nonce(&entry.nonce)?;
        let c = base64_decode(&entry.ciphertext)?;
        aes_decrypt(&self.key, &n, &c)
    }

    pub fn delete(&self, key: &str) -> Result<()> {
        let mut vf = self.load_or_default()?;
        if vf.entries.remove(key).is_none() {
            return Err(ArkonError::VaultError(format!("secret '{key}' not found")));
        }
        self.save_file(&vf)
    }

    pub fn list_keys(&self) -> Result<Vec<String>> {
        let vf = self.load_or_default()?;
        let mut keys: Vec<String> = vf.entries.keys().cloned().collect();
        keys.sort();
        Ok(keys)
    }

    pub fn export_all(&self) -> Result<HashMap<String, Zeroizing<String>>> {
        let vf  = self.load_or_default()?;
        let mut map = HashMap::new();
        for (k, entry) in &vf.entries {
            let n = hex_to_nonce(&entry.nonce)?;
            let c = base64_decode(&entry.ciphertext)?;
            let plain = aes_decrypt(&self.key, &n, &c)?;
            let value = String::from_utf8(plain.to_vec())
                .map_err(|_| ArkonError::VaultError("invalid UTF-8 in secret".into()))?;
            map.insert(k.clone(), Zeroizing::new(value));
        }
        Ok(map)
    }

    // ── Key rotation ──────────────────────────────────────────────────────

    pub fn rotate(&self) -> Result<usize> {
        let all = self.export_all()?;
        let count = all.len();
        // Backup
        let bak = self.vault_path.with_extension("vault.bak");
        if self.vault_path.exists() {
            std::fs::copy(&self.vault_path, &bak)?;
            set_0600(&bak);
            tracing::info!(backup = %bak.display(), "vault backed up");
        }
        // Clear keychain so new passphrase is prompted
        clear_session(&self.project);
        // Re-open with fresh credentials
        let new_vault = Vault::open(&self.project)?;
        for (k, v) in all.iter() {
            new_vault.set(k, v.as_bytes())?;
        }
        tracing::info!(count = %count, "vault key rotation complete");
        Ok(count)
    }

    pub fn recover(project: &str) -> Result<Self> {
        let recovered = mnemonic::interactive_recover()?;
        let vault = Self::open_with_key(project, recovered)?;
        let count = vault.rotate()?;
        tracing::info!(project = %project, secrets = %count, "vault recovered and re-encrypted");
        Ok(vault)
    }

    // ── Typed accessors ───────────────────────────────────────────────────

    pub fn set_acme_key(&self, domain: &str, json: &str) -> Result<()> {
        self.set(&format!("__acme__{}", sanitize(domain)), json.as_bytes())
    }

    pub fn get_acme_key(&self, domain: &str) -> Result<String> {
        let b = self.get(&format!("__acme__{}", sanitize(domain)))?;
        String::from_utf8(b.to_vec())
            .map_err(|_| ArkonError::VaultError("ACME key is not valid UTF-8".into()))
    }

    pub fn set_peer_identity(&self, seed: &[u8; 32]) -> Result<()> {
        self.set("__peer_identity__", seed)
    }

    pub fn get_peer_identity(&self) -> Result<[u8; 32]> {
        let b = self.get("__peer_identity__")?;
        if b.len() != 32 {
            return Err(ArkonError::VaultError("peer identity must be 32 bytes".into()));
        }
        let mut seed = [0u8; 32];
        seed.copy_from_slice(&b);
        Ok(seed)
    }

    // ── File I/O ──────────────────────────────────────────────────────────

    fn load_file(path: &PathBuf) -> Result<VaultFile> {
        let raw = std::fs::read_to_string(path)?;
        serde_json::from_str(&raw)
            .map_err(|e| ArkonError::VaultError(format!("vault corrupt: {e}")))
    }

    fn load_or_default(&self) -> Result<VaultFile> {
        if self.vault_path.exists() {
            Self::load_file(&self.vault_path)
        } else {
            Ok(VaultFile { version: 1, project: self.project.clone(),
                           salt: hex::encode(self.salt), entries: HashMap::new() })
        }
    }

    fn save_file(&self, vf: &VaultFile) -> Result<()> {
        if let Some(p) = self.vault_path.parent() { std::fs::create_dir_all(p)?; }
        let json = serde_json::to_string_pretty(vf)
            .map_err(|e| ArkonError::VaultError(e.to_string()))?;
        let tmp = self.vault_path.with_extension("vault.tmp");
        std::fs::write(&tmp, json)?;
        set_0600(&tmp);
        std::fs::rename(&tmp, &self.vault_path)?;
        set_0600(&self.vault_path);
        Ok(())
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn random_salt() -> [u8; SALT_LEN] {
    let mut s = [0u8; SALT_LEN];
    aes_gcm::aead::OsRng.fill_bytes(&mut s);
    s
}

fn hex_to_salt(h: &str) -> Result<[u8; SALT_LEN]> {
    let b = hex::decode(h).map_err(|e| ArkonError::VaultError(format!("salt hex: {e}")))?;
    b.try_into().map_err(|_| ArkonError::VaultError("salt must be 32 bytes".into()))
}

fn hex_to_nonce(h: &str) -> Result<[u8; NONCE_LEN]> {
    let b = hex::decode(h).map_err(|e| ArkonError::VaultError(format!("nonce hex: {e}")))?;
    b.try_into().map_err(|_| ArkonError::VaultError("nonce must be 12 bytes".into()))
}

fn base64_encode(b: &[u8]) -> String {
    use base64ct::{Base64, Encoding};
    Base64::encode_string(b)
}

fn base64_decode(s: &str) -> Result<Vec<u8>> {
    use base64ct::{Base64, Encoding};
    Base64::decode_vec(s).map_err(|e| ArkonError::VaultError(format!("base64: {e}")))
}

fn set_0600(path: &PathBuf) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600));
    }
}

use aes_gcm::aead::rand_core::RngCore;
