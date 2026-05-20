use arkon_adapters::AdapterRegistry;
use arkon_targets::Dispatcher;
use std::sync::{Arc, RwLock};

/// Shared application state managed by Tauri.
/// Accessed via `tauri::State<AppState>` in command handlers.
#[derive(Default)]
pub struct AppState {
    /// The currently open project root path.
    pub project_root: RwLock<Option<std::path::PathBuf>>,
    /// Adapter registry — shared across all IPC calls.
    pub adapter_registry: Arc<AdapterRegistry>,
    /// Deployment dispatcher — shared across all IPC calls.
    pub dispatcher: Arc<Dispatcher>,
}

impl AppState {
    pub fn project_root(&self) -> Option<std::path::PathBuf> {
        self.project_root.read().ok()?.clone()
    }

    pub fn set_project_root(&self, path: std::path::PathBuf) {
        if let Ok(mut root) = self.project_root.write() {
            *root = Some(path);
        }
    }
}
