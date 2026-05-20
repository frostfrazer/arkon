//! ARKON Desktop GUI — Tauri v2 backend
//!
//! This crate exposes ARKON's core functionality as Tauri IPC commands,
//! allowing the React/TypeScript frontend to interact with the full ARKON
//! pipeline without spawning subprocesses.
//!
//! ## IPC Commands
//!
//! | Command | Description |
//! |---------|-------------|
//! | `detect_project` | Run the project detector and return adapter + confidence |
//! | `list_targets` | Parse arkon.toml and return configured targets |
//! | `get_deploy_history` | Return recent deploy records from audit log |
//! | `get_snapshots` | List snapshots for a project |
//! | `list_secrets` | Return secret key names (not values) |
//! | `set_secret` | Encrypt and store a secret |
//! | `delete_secret` | Remove a secret from the vault |
//! | `get_status` | Health-check all targets |
//! | `get_cost_estimate` | Estimate deploy cost without deploying |
//! | `doctor_check` | Run system dependency checks |

pub mod commands;
pub mod state;
pub mod tray;

use tauri::Manager;
use tracing_subscriber::EnvFilter;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Initialise logging
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_env("ARKON_LOG")
                .unwrap_or_else(|_| EnvFilter::new("arkon=info")),
        )
        .without_time()
        .with_target(false)
        .init();

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_dialog::init())
        .manage(state::AppState::default())
        .invoke_handler(tauri::generate_handler![
            commands::detect_project,
            commands::get_project_root,
            commands::set_project_root,
            commands::open_project_dialog,
            commands::list_targets,
            commands::get_deploy_history,
            commands::get_snapshots,
            commands::list_secrets,
            commands::set_secret,
            commands::delete_secret,
            commands::get_status,
            commands::get_cost_estimate,
            commands::doctor_check,
            commands::ship,
            commands::rollback,
            commands::promote,
        ])
        .setup(|app| {
            tray::setup_tray(app)?;
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error running ARKON GUI");
}
