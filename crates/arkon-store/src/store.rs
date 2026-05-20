use crate::index::{IndexEntry, SnapshotIndex};
use arkon_core::{
    artifact::Artifact,
    deploy::DeployRecord,
    error::{ArkonError, Result},
    snapshot::Snapshot,
};
use chrono::Utc;
use std::path::{Path, PathBuf};
use tracing::{debug, info};
use uuid::Uuid;

/// Persistent snapshot store backed by ~/.arkon/snapshots/<project>/
pub struct SnapshotStore {
    project: String,
    dir: PathBuf,
    index_path: PathBuf,
}

impl SnapshotStore {
    /// Open the store for a given project. Creates directories on first use.
    pub fn open(project: &str) -> Result<Self> {
        let base = dirs::home_dir()
            .ok_or_else(|| ArkonError::Other(anyhow::anyhow!("cannot find home dir")))?;
        let dir = base
            .join(".arkon")
            .join("snapshots")
            .join(sanitize(project));
        std::fs::create_dir_all(&dir)?;
        let index_path = dir.join("index.json");
        Ok(Self {
            project: project.to_string(),
            dir,
            index_path,
        })
    }

    // ─── Write ───────────────────────────────────────────────────────────────

    /// Persist a snapshot after a successful deploy.
    /// Called by the dispatcher immediately after `deploy()` returns Ok.
    pub fn save(
        &self,
        artifact: &Artifact,
        record: &DeployRecord,
        adapter_name: &str,
    ) -> Result<Snapshot> {
        let id = Uuid::new_v4().to_string();
        let now = Utc::now();

        let snapshot = Snapshot {
            id: id.clone(),
            project: self.project.clone(),
            target: record.target.clone(),
            adapter: adapter_name.to_string(),
            artifact_fingerprint: artifact.fingerprint.clone(),
            file_hashes: artifact.file_hashes.clone(),
            size_bytes: artifact.size_bytes,
            deployed_at: now,
            deploy_record_id: record.id.clone(),
            notes: None,
        };

        // Persist full snapshot JSON
        let filename = format!(
            "{}_{}_{}",
            now.format("%Y-%m-%dT%H-%M-%SZ"),
            sanitize(&record.target),
            &id[..8],
        );
        let snap_path = self.dir.join(format!("{filename}.json"));
        let json = serde_json::to_string_pretty(&snapshot)
            .map_err(|e| ArkonError::Other(e.into()))?;
        std::fs::write(&snap_path, json)?;

        // Update index
        let mut index = SnapshotIndex::load(&self.index_path).unwrap_or_default();
        index.project = self.project.clone();
        index.add(IndexEntry {
            id: id.clone(),
            target: record.target.clone(),
            adapter: adapter_name.to_string(),
            deployed_at: now,
            artifact_fingerprint: artifact.fingerprint.clone(),
            size_bytes: artifact.size_bytes,
            filename: format!("{filename}.json"),
        });
        index.save(&self.index_path)?;

        info!(
            snapshot_id = %&id[..8],
            target = %record.target,
            "snapshot saved"
        );
        Ok(snapshot)
    }

    // ─── Read ────────────────────────────────────────────────────────────────

    /// Load a full snapshot by its index entry.
    pub fn load_by_filename(&self, filename: &str) -> Result<Snapshot> {
        let path = self.dir.join(filename);
        let raw = std::fs::read_to_string(&path)
            .map_err(|_| ArkonError::SnapshotNotFound { snapshot_id: filename.to_string() })?;
        serde_json::from_str(&raw).map_err(|e| ArkonError::Other(e.into()))
    }

    /// Find and load a snapshot by query string + optional target filter.
    /// See `SnapshotIndex::find` for query syntax.
    pub fn find(
        &self,
        query: &str,
        target_filter: Option<&str>,
    ) -> Result<Snapshot> {
        let index = SnapshotIndex::load(&self.index_path)?;
        let entry = index.find(query, target_filter).ok_or_else(|| {
            ArkonError::SnapshotNotFound {
                snapshot_id: format!("query='{query}'"),
            }
        })?;
        self.load_by_filename(&entry.filename)
    }

    /// List all snapshots, newest first.
    pub fn list(&self, target_filter: Option<&str>) -> Result<Vec<IndexEntry>> {
        let index = SnapshotIndex::load(&self.index_path).unwrap_or_default();
        Ok(index.list(target_filter).into_iter().cloned().collect())
    }

    /// Delete a snapshot by ID prefix or full ID. Removes JSON + index entry.
    pub fn delete(&self, id_prefix: &str) -> Result<()> {
        let mut index = SnapshotIndex::load(&self.index_path)?;
        let entry = index
            .entries
            .iter()
            .find(|e| e.id.starts_with(id_prefix))
            .ok_or_else(|| ArkonError::SnapshotNotFound {
                snapshot_id: id_prefix.to_string(),
            })?
            .clone();

        let snap_path = self.dir.join(&entry.filename);
        if snap_path.exists() {
            std::fs::remove_file(&snap_path)?;
        }
        index.entries.retain(|e| !e.id.starts_with(id_prefix));
        index.save(&self.index_path)?;
        info!(id = %id_prefix, "snapshot deleted");
        Ok(())
    }

    /// Prune snapshots keeping only the N most recent per target.
    pub fn prune(&self, keep: usize) -> Result<usize> {
        let index = SnapshotIndex::load(&self.index_path).unwrap_or_default();
        let mut targets: std::collections::HashMap<String, Vec<IndexEntry>> =
            std::collections::HashMap::new();

        for entry in index.entries {
            targets.entry(entry.target.clone()).or_default().push(entry);
        }

        let mut deleted = 0;
        let mut new_index = SnapshotIndex {
            project: self.project.clone(),
            entries: vec![],
        };

        for (_, mut entries) in targets {
            entries.sort_by(|a, b| b.deployed_at.cmp(&a.deployed_at));
            let (keep_entries, prune_entries) = entries.split_at(keep.min(entries.len()));
            for e in prune_entries {
                let path = self.dir.join(&e.filename);
                if path.exists() {
                    std::fs::remove_file(&path)?;
                    deleted += 1;
                }
            }
            for e in keep_entries {
                new_index.add(e.clone());
            }
        }

        new_index.save(&self.index_path)?;
        info!(deleted = %deleted, kept_per_target = %keep, "snapshots pruned");
        Ok(deleted)
    }

    // ─── Reconstruct artifact from snapshot ──────────────────────────────────

    /// Reconstruct a synthetic Artifact from a snapshot.
    /// The artifact points at the build cache directory if it still exists,
    /// otherwise at the project root (rsync --checksum handles the rest).
    pub fn reconstruct_artifact(
        snapshot: &Snapshot,
        project_root: &Path,
        adapter_name: &str,
    ) -> Artifact {
        use arkon_core::artifact::DeployableKind;

        // Look for cached output dir in common locations
        let output_candidates = [
            project_root.join("dist"),
            project_root.join("out"),
            project_root.join(".next"),
            project_root.join("build"),
            project_root.join("Builds"),
        ];

        let output_dir = output_candidates
            .into_iter()
            .find(|p| p.exists())
            .unwrap_or_else(|| project_root.to_path_buf());

        Artifact {
            name: format!("{}-rollback", snapshot.project),
            path: output_dir,
            kind: DeployableKind::Static, // targets use file_hashes, not kind, for rollback
            fingerprint: snapshot.artifact_fingerprint.clone(),
            file_hashes: snapshot.file_hashes.clone(),
            size_bytes: snapshot.size_bytes,
            meta: std::collections::HashMap::from([
                ("rollback".to_string(), "true".to_string()),
                ("snapshot_id".to_string(), snapshot.id.clone()),
            ]),
        }
    }
}

fn sanitize(s: &str) -> String {
    s.chars()
        .map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c } else { '_' })
        .collect()
}
