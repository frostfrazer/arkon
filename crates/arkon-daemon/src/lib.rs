//! ARKON Daemon
//!
//! Long-running background process managing:
//!   - Health monitoring   — HTTP/TCP/process checks on configurable intervals
//!   - Audit log           — HMAC-chained, append-only deploy history
//!   - Adapter hot-reload  — inotify/kqueue FS events on ~/.arkon/adapters/
//!   - Snapshot pruning    — daily cleanup, keeps N snapshots per target
//!   - Let's Encrypt ACME  — TLS provisioning + 30-day auto-renewal

pub mod acme;
pub mod audit;
pub mod health;
pub mod pruner;
pub mod watcher;

pub use acme::AcmeProvisioner;
pub use audit::AuditLog;
pub use health::HealthMonitor;
pub use pruner::SnapshotPruner;
pub use watcher::AdapterWatcher;
