//! Snapshot pruner daemon task.
//!
//! Runs daily and keeps only the N most recent snapshots per target per project.
//! Prevents ~/.arkon/snapshots/ from growing unbounded on long-lived machines.

use arkon_store::SnapshotStore;
use std::time::Duration;
use tokio::time;
use tracing::{info, warn};

pub struct SnapshotPruner {
    /// Project names to prune (from arkon.toml project.name across all projects on this machine).
    projects: Vec<String>,
    /// Number of snapshots to keep per target.
    keep: usize,
    /// How often to run (default: 24h).
    interval: Duration,
}

impl SnapshotPruner {
    pub fn new(projects: Vec<String>, keep: usize, interval_hours: u64) -> Self {
        Self {
            projects,
            keep,
            interval: Duration::from_secs(interval_hours * 3600),
        }
    }

    pub fn daily(projects: Vec<String>) -> Self {
        Self::new(projects, 10, 24)
    }

    /// Run forever. Spawn via `tokio::spawn`.
    pub async fn run(self) {
        info!(
            projects  = %self.projects.len(),
            keep      = %self.keep,
            interval_h = %(self.interval.as_secs() / 3600),
            "snapshot pruner started"
        );

        let mut ticker = time::interval(self.interval);
        ticker.tick().await; // skip immediate first tick

        loop {
            ticker.tick().await;
            for project in &self.projects {
                match SnapshotStore::open(project) {
                    Ok(store) => match store.prune(self.keep) {
                        Ok(deleted) if deleted > 0 => {
                            info!(project = %project, deleted = %deleted, "snapshots pruned");
                        }
                        Ok(_) => {}
                        Err(e) => warn!(project = %project, error = %e, "prune failed"),
                    },
                    Err(e) => warn!(project = %project, error = %e, "failed to open snapshot store"),
                }
            }
        }
    }
}
