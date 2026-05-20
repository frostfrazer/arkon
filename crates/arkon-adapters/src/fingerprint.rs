use arkon_core::error::{ArkonError, Result};
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
};
use walkdir::WalkDir;

/// Fingerprint an entire directory tree.
/// Returns:
///   - `root_hash`: SHA-256 of all (path, file_hash) pairs sorted — stable across platforms.
///   - `file_hashes`: map of relative path string → file SHA-256 hex.
///   - `total_bytes`: sum of all file sizes.
pub fn fingerprint_dir(root: &Path) -> Result<(String, HashMap<String, String>, u64)> {
    let mut file_hashes: Vec<(String, String)> = Vec::new();
    let mut total_bytes: u64 = 0;

    for entry in WalkDir::new(root)
        .follow_links(false)
        .sort_by_file_name()
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
    {
        let abs = entry.path();
        let rel = abs
            .strip_prefix(root)
            .map_err(|e| ArkonError::Other(e.into()))?
            .to_string_lossy()
            .replace('\\', "/"); // normalize on Windows

        let bytes = fs::read(abs)?;
        total_bytes += bytes.len() as u64;

        let mut h = Sha256::new();
        h.update(&bytes);
        let hash = hex::encode(h.finalize());
        file_hashes.push((rel, hash));
    }

    // Root hash = hash of sorted (rel_path \0 file_hash \n) pairs
    let mut root_hasher = Sha256::new();
    file_hashes.sort_by(|a, b| a.0.cmp(&b.0));
    for (path, hash) in &file_hashes {
        root_hasher.update(path.as_bytes());
        root_hasher.update(b"\0");
        root_hasher.update(hash.as_bytes());
        root_hasher.update(b"\n");
    }
    let root_hash = hex::encode(root_hasher.finalize());

    let map: HashMap<String, String> = file_hashes.into_iter().collect();
    Ok((root_hash, map, total_bytes))
}

/// Compare two file-hash maps and return the files that changed, were added,
/// or were deleted. Used by targets to do incremental (diff) uploads.
#[derive(Debug, Default)]
pub struct FileDiff {
    pub added: Vec<String>,
    pub modified: Vec<String>,
    pub deleted: Vec<String>,
}

impl FileDiff {
    pub fn compute(
        previous: &HashMap<String, String>,
        current: &HashMap<String, String>,
    ) -> Self {
        let mut diff = FileDiff::default();

        for (path, hash) in current {
            match previous.get(path) {
                None => diff.added.push(path.clone()),
                Some(prev_hash) if prev_hash != hash => diff.modified.push(path.clone()),
                _ => {}
            }
        }
        for path in previous.keys() {
            if !current.contains_key(path) {
                diff.deleted.push(path.clone());
            }
        }
        diff
    }

    pub fn is_empty(&self) -> bool {
        self.added.is_empty() && self.modified.is_empty() && self.deleted.is_empty()
    }

    pub fn changed_count(&self) -> usize {
        self.added.len() + self.modified.len()
    }
}

/// Hash a single file. Utility for adapters building cache keys.
pub fn hash_file(path: &Path) -> Result<String> {
    let bytes = fs::read(path)?;
    let mut h = Sha256::new();
    h.update(&bytes);
    Ok(hex::encode(h.finalize()))
}

/// Build a cache key from a list of input paths (lockfiles, config files, etc.).
/// Stable: sorted before hashing, so order doesn't matter.
pub fn cache_key_from_files(root: &Path, paths: &[&str]) -> String {
    let mut parts: Vec<(String, String)> = paths
        .iter()
        .filter_map(|p| {
            let full = root.join(p);
            if full.exists() {
                hash_file(&full).ok().map(|h| (p.to_string(), h))
            } else {
                None
            }
        })
        .collect();
    parts.sort();

    let mut h = Sha256::new();
    for (path, hash) in &parts {
        h.update(path.as_bytes());
        h.update(b"=");
        h.update(hash.as_bytes());
        h.update(b";");
    }
    hex::encode(h.finalize())
}
