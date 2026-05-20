//! Per-project peer identity.
//!
//! Each project gets a stable Ed25519 keypair stored at:
//!   ~/.arkon/identity/<project>.key  (private, 0600)
//!   ~/.arkon/identity/<project>.pub  (public, hex-encoded peer ID)
//!
//! The peer ID is derived as SHA-256(public_key_bytes)[..16] hex.
//! It's stable across restarts so shareable links remain valid as long as
//! the same machine is serving.

use arkon_core::error::{ArkonError, Result};
use ed25519_dalek::{SigningKey, VerifyingKey};
use rand::rngs::OsRng;
use sha2::{Digest, Sha256};
use std::path::PathBuf;

/// A project's peer identity backed by a real Ed25519 keypair.
#[derive(Debug, Clone)]
pub struct PeerIdentity {
    /// Stable 16-char hex peer ID derived from the public key.
    pub peer_id: String,
    /// Raw 32-byte Ed25519 private key seed.
    pub private_key: [u8; 32],
    /// Raw 32-byte Ed25519 public key.
    pub public_key: [u8; 32],
}

impl PeerIdentity {
    pub fn load_or_create(project: &str) -> Result<Self> {
        let key_path = identity_path(project);
        if key_path.exists() {
            Self::load(&key_path)
        } else {
            let identity = Self::generate();
            identity.save(&key_path)?;
            Ok(identity)
        }
    }

    pub fn generate() -> Self {
        let signing_key = SigningKey::generate(&mut OsRng);
        Self::from_signing_key(&signing_key)
    }

    fn from_signing_key(signing_key: &SigningKey) -> Self {
        let verifying_key: VerifyingKey = signing_key.verifying_key();
        let pub_bytes: [u8; 32] = verifying_key.to_bytes();
        let priv_bytes: [u8; 32] = signing_key.to_bytes();

        // peer_id = first 8 bytes of SHA-256(public_key), hex-encoded → 16 chars
        let mut h = Sha256::new();
        h.update(&pub_bytes);
        let hash = h.finalize();
        let peer_id = hex::encode(&hash[..8]);

        Self {
            peer_id,
            private_key: priv_bytes,
            public_key: pub_bytes,
        }
    }

    fn load(path: &PathBuf) -> Result<Self> {
        let raw = std::fs::read(path)?;
        if raw.len() != 32 {
            return Err(ArkonError::Other(anyhow::anyhow!(
                "corrupted identity file — delete {} and retry",
                path.display()
            )));
        }
        let mut seed = [0u8; 32];
        seed.copy_from_slice(&raw);
        let signing_key = SigningKey::from_bytes(&seed);
        Ok(Self::from_signing_key(&signing_key))
    }

    fn save(&self, path: &PathBuf) -> Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, &self.private_key)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
        }
        Ok(())
    }

    pub fn share_link(&self, project: &str, relay_url: &str) -> String {
        format!("{}/?p={}&n={}", relay_url.trim_end_matches('/'), self.peer_id, project)
    }
}

fn identity_path(project: &str) -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join(".arkon")
        .join("identity")
        .join(format!("{}.key", sanitize(project)))
}

fn sanitize(s: &str) -> String {
    s.chars()
        .map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c } else { '_' })
        .collect()
}
