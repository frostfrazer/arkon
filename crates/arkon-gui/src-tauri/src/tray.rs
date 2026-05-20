//! System tray icon + context menu.
//!
//! The tray icon shows a coloured dot reflecting the last deploy status:
//!   ● green  — last deploy succeeded
//!   ● yellow — health check warning
//!   ● red    — last deploy failed / target unreachable
//!   ● grey   — no deploys yet / daemon not active
//!
//! Menu items:
//!   Open Dashboard       → show/focus main window
//!   ─────────────────
//!   Deploy (project)     → trigger arkon ship for default target
//!   Preview              → arkon preview
//!   ─────────────────
//!   Status               → submenu with per-target health
//!   Deploy History       → show log in dashboard
//!   ─────────────────
//!   Quit ARKON

use tauri::{
    App, AppHandle, Emitter, Manager, Runtime,
    menu::{Menu, MenuItem, PredefinedMenuItem, Submenu},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
};

pub fn setup_tray<R: Runtime>(app: &mut App<R>) -> tauri::Result<()> {
    let open_i    = MenuItem::with_id(app, "open",    "Open Dashboard",  true, None::<&str>)?;
    let sep1      = PredefinedMenuItem::separator(app)?;
    let ship_i    = MenuItem::with_id(app, "ship",    "Deploy (default target)", true, None::<&str>)?;
    let preview_i = MenuItem::with_id(app, "preview", "Start Preview",   true, None::<&str>)?;
    let sep2      = PredefinedMenuItem::separator(app)?;
    let log_i     = MenuItem::with_id(app, "log",     "Deploy History",  true, None::<&str>)?;
    let sep3      = PredefinedMenuItem::separator(app)?;
    let quit_i    = MenuItem::with_id(app, "quit",    "Quit ARKON",      true, None::<&str>)?;

    let menu = Menu::with_items(app, &[
        &open_i, &sep1, &ship_i, &preview_i, &sep2, &log_i, &sep3, &quit_i,
    ])?;

    let _tray = TrayIconBuilder::new()
        .icon(app.default_window_icon().cloned().unwrap())
        .menu(&menu)
        .tooltip("ARKON — Automated Runtime & Kernel Orchestration Node")
        .on_menu_event(move |app, event| {
            handle_menu_event(app, event.id().as_ref());
        })
        .on_tray_icon_event(|tray, event| {
            // Left-click → show window
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                let app = tray.app_handle();
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.show();
                    let _ = window.set_focus();
                }
            }
        })
        .build(app)?;

    Ok(())
}

fn handle_menu_event<R: Runtime>(app: &AppHandle<R>, id: &str) {
    match id {
        "open" => {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.show();
                let _ = window.set_focus();
            }
        }
        "ship" => {
            // Emit an event to the frontend which triggers the deploy flow
            let _ = app.emit("tray-ship", ());
        }
        "preview" => {
            let _ = app.emit("tray-preview", ());
        }
        "log" => {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.show();
                let _ = window.set_focus();
                let _ = window.emit("navigate", "/history");
            }
        }
        "quit" => {
            app.exit(0);
        }
        _ => {}
    }
}
