//! ARKON Snapshot Store
//!
//! Every successful deploy writes a Snapshot to:
//!   `~/.arkon/snapshots/<project>/<YYYY-MM-DD>T<HH-MM-SS>Z_<target>_<id8>.json`
//!
//! Snapshots record per-file SHA-256 hashes but NOT full file copies.
//! Rollback re-dispatches the stored artifact metadata to the target via rsync --checksum,
//! which reconstructs the exact file set from your local build cache.
//!
//! The snapshot index (~/.arkon/snapshots/<project>/index.json) tracks all snapshots
//! for fast listing and fuzzy timestamp lookup.

pub mod index;
pub mod store;

pub use store::SnapshotStore;
pub use arkon_core::snapshot::Snapshot;
