//! Adapter hot-reload watcher using real filesystem events via the `notify` crate.
//! On Linux: inotify. macOS: kqueue. Windows: ReadDirectoryChangesW.
//! Falls back to a 10-second poll loop if the watcher cannot be initialised.

use arkon_adapters::AdapterRegistry;
use notify::{RecommendedWatcher, RecursiveMode, Watcher, Event};
use std::{path::PathBuf, sync::Arc, time::Duration};
use tokio::sync::mpsc;
use tracing::{debug, info, warn};

pub struct AdapterWatcher {
    registry:     Arc<AdapterRegistry>,
    adapters_dir: PathBuf,
    debounce:     Duration,
}

impl AdapterWatcher {
    pub fn new(registry: Arc<AdapterRegistry>, adapters_dir: PathBuf, debounce_ms: u64) -> Self {
        Self { registry, adapters_dir, debounce: Duration::from_millis(debounce_ms) }
    }

    pub async fn run(self) {
        if !self.adapters_dir.exists() {
            debug!(dir = %self.adapters_dir.display(), "adapters dir absent — watcher idle");
            return;
        }
        info!(dir = %self.adapters_dir.display(), "adapter hot-reload watcher started");

        let (tx, mut rx) = mpsc::channel::<Event>(64);
        let mut watcher: RecommendedWatcher = match notify::recommended_watcher(
            move |res: notify::Result<Event>| { if let Ok(e) = res { let _ = tx.blocking_send(e); } }
        ) {
            Ok(w) => w,
            Err(e) => {
                warn!(error = %e, "notify init failed — polling every 10s");
                self.run_poll_fallback().await;
                return;
            }
        };

        if let Err(e) = watcher.watch(&self.adapters_dir, RecursiveMode::Recursive) {
            warn!(error = %e, "watch failed — polling every 10s");
            self.run_poll_fallback().await;
            return;
        }

        let mut deadline: Option<tokio::time::Instant> = None;
        let debounce = self.debounce;
        let registry = self.registry.clone();
        let dir      = self.adapters_dir.clone();

        loop {
            tokio::select! {
                Some(event) = rx.recv() => {
                    let relevant = event.paths.iter()
                        .any(|p| p.extension().and_then(|e| e.to_str()) == Some("json"));
                    if relevant {
                        deadline = Some(tokio::time::Instant::now() + debounce);
                    }
                }
                _ = async {
                    match deadline {
                        Some(d) => tokio::time::sleep_until(d).await,
                        None    => tokio::time::sleep(Duration::from_secs(3600)).await,
                    }
                } => {
                    deadline = None;
                    info!("adapter manifest changed — hot-reloading");
                    match registry.load_community_dir(&dir) {
                        Ok(n)  => info!(count = %n, "adapters reloaded"),
                        Err(e) => warn!(error = %e, "adapter reload failed"),
                    }
                }
            }
        }
    }

    async fn run_poll_fallback(self) {
        use std::collections::HashMap;
        let mut last: HashMap<PathBuf, std::time::SystemTime> = HashMap::new();
        let mut tick = tokio::time::interval(Duration::from_secs(10));
        loop {
            tick.tick().await;
            let changed = walkdir::WalkDir::new(&self.adapters_dir)
                .max_depth(3).into_iter().filter_map(|e| e.ok())
                .filter(|e| e.path().extension().and_then(|x| x.to_str()) == Some("json"))
                .any(|e| {
                    let p = e.path().to_path_buf();
                    let mt = e.metadata().ok().and_then(|m| m.modified().ok());
                    mt.map(|t| last.insert(p, t).map(|prev| prev != t).unwrap_or(true))
                        .unwrap_or(false)
                });
            if changed {
                let _ = self.registry.load_community_dir(&self.adapters_dir);
            }
        }
    }
}
