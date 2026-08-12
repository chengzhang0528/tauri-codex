mod commands;
#[cfg(debug_assertions)]
mod dev_bridge;
mod host;
mod job;
mod model;
mod paths;
mod runtime;
mod sessions;
mod updates;

use commands::AppState;
use serde::Serialize;
#[cfg(debug_assertions)]
use tauri::Manager;
use tauri::{AppHandle, Emitter, RunEvent};

#[cfg(debug_assertions)]
fn keep_browser_bridge_alive(code: Option<i32>) -> bool {
    code.is_none()
}

pub(crate) fn emit_to_main<T>(app: &AppHandle, event: &str, payload: T)
where
    T: Serialize + Clone,
{
    #[cfg(debug_assertions)]
    app.state::<AppState>().dev_events.publish(event, &payload);
    let _ = app.emit_to("main", event, payload);
}

pub(crate) fn terminal_event_name(label: &str, event: &str) -> String {
    format!("{event}:{label}")
}

pub(crate) fn emit_to_terminal<T>(app: &AppHandle, label: &str, event: &str, payload: T)
where
    T: Serialize + Clone,
{
    let event = terminal_event_name(label, event);
    #[cfg(debug_assertions)]
    app.state::<AppState>().dev_events.publish(&event, &payload);
    let _ = app.emit_to("main", &event, payload);
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    match std::env::args().nth(1).as_deref() {
        Some("--session-host") => {
            if let Err(error) = host::run_from_stdin() {
                eprintln!("Session Host failed: {error}");
                std::process::exit(1);
            }
            return;
        }
        Some("--runtime-check") => {
            std::process::exit(if runtime::check_system_node().is_ok() {
                0
            } else {
                1
            });
        }
        Some("--ensure-system-runtime") => {
            std::process::exit(if runtime::ensure_system_node_from_install_dir().is_ok() {
                0
            } else {
                1
            });
        }
        _ => {}
    }

    let state = AppState::default();
    let exit_sessions = state.sessions.clone();
    tauri::Builder::default()
        .manage(state)
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            runtime::check_system_node().map_err(std::io::Error::other)?;
            paths::codex_entry(app.handle()).map_err(std::io::Error::other)?;
            commands::sync_server_profiles(app.handle()).map_err(std::io::Error::other)?;
            #[cfg(debug_assertions)]
            dev_bridge::start(
                app.handle().clone(),
                app.state::<AppState>().inner().clone(),
            )
            .map_err(std::io::Error::other)?;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_snapshot,
            commands::get_server,
            commands::save_server,
            commands::delete_server,
            commands::save_config,
            commands::save_codex_settings,
            commands::start_terminal,
            commands::restart_terminal,
            commands::terminal_input,
            commands::terminal_ready,
            commands::terminal_rendered,
            commands::terminal_resize,
            commands::interrupt_terminal,
            commands::terminate_terminal,
            commands::force_terminate_terminal,
            commands::check_app_update,
            commands::check_codex_update,
            commands::download_app_update,
            commands::stage_app_update,
            commands::stage_codex_update,
            commands::install_codex_update,
            commands::activate_codex_update,
            commands::apply_app_update
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(move |_app, event| match event {
            #[cfg(debug_assertions)]
            RunEvent::ExitRequested { code, api, .. } if keep_browser_bridge_alive(code) => {
                api.prevent_exit();
            }
            RunEvent::Exit => {
                exit_sessions.force_terminate_all();
            }
            _ => {}
        });
}

#[cfg(all(test, debug_assertions))]
mod tests {
    use super::{keep_browser_bridge_alive, terminal_event_name};

    #[test]
    fn browser_bridge_survives_only_user_requested_debug_exit() {
        assert!(keep_browser_bridge_alive(None));
        assert!(!keep_browser_bridge_alive(Some(0)));
    }

    #[test]
    fn terminal_events_are_scoped_to_one_session() {
        assert_eq!(
            terminal_event_name("terminal-abc", "terminal-output"),
            "terminal-output:terminal-abc"
        );
    }

    #[test]
    fn session_runtime_cannot_create_per_session_tauri_windows() {
        let source = include_str!("sessions.rs");
        assert!(!source.contains("WebviewWindowBuilder"));
        assert!(!source.contains("WebviewUrl::App"));
        assert!(source.contains("--session-host"));
    }
}
