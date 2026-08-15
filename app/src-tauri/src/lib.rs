mod commands;
mod delivery;
#[cfg(debug_assertions)]
mod dev_bridge;
mod host;
mod job;
mod model;
mod paths;
mod runtime;
mod sessions;

use commands::AppState;
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, RunEvent, WebviewUrl, WebviewWindowBuilder};

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

#[derive(Debug, PartialEq, Eq)]
enum ManagerVerificationAction {
    Authenticode(std::path::PathBuf),
    CodexComponent(std::path::PathBuf),
}

fn manager_verification_action(
    args: &[std::ffi::OsString],
) -> Result<Option<ManagerVerificationAction>, String> {
    let Some(flag @ ("--verify-authenticode" | "--verify-codex-component")) =
        args.first().and_then(|arg| arg.to_str())
    else {
        return Ok(None);
    };
    if args.len() != 2 {
        return Err(format!("{flag} 必须且只能接收一个绝对路径"));
    }
    let path = std::path::PathBuf::from(&args[1]);
    if !path.is_absolute() {
        return Err(format!("{flag} 只接受绝对路径"));
    }
    Ok(Some(match flag {
        "--verify-authenticode" => ManagerVerificationAction::Authenticode(path),
        "--verify-codex-component" => ManagerVerificationAction::CodexComponent(path),
        _ => unreachable!(),
    }))
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run_manager() {
    let args = std::env::args_os().skip(1).collect::<Vec<_>>();
    match manager_verification_action(&args) {
        Ok(Some(action)) => {
            let result = match action {
                ManagerVerificationAction::Authenticode(path) => {
                    delivery::verify_release_authenticode(&path)
                }
                ManagerVerificationAction::CodexComponent(root) => {
                    delivery::verify_release_codex_component(&root)
                }
            };
            if let Err(error) = result {
                eprintln!("Release verification failed: {error}");
                std::process::exit(1);
            }
            return;
        }
        Ok(None) => {}
        Err(error) => {
            eprintln!("Release verification failed: {error}");
            std::process::exit(1);
        }
    }
    match args.first().and_then(|arg| arg.to_str()) {
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
        Some("--thin-setup") => {
            std::process::exit(if delivery::validate_installer_bootstrap().is_ok() {
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
            WebviewWindowBuilder::new(app, "main", WebviewUrl::App("index.html".into()))
                .title("tauri-codex")
                .inner_size(1220.0, 780.0)
                .min_inner_size(980.0, 640.0)
                .build()?;
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
            commands::check_update,
            commands::prepare_update,
            commands::activate_update,
            commands::cancel_update
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

pub fn run_launcher() {
    match delivery::run_launcher_action() {
        Ok(true) => return,
        Ok(false) => {}
        Err(error) => {
            eprintln!("Launcher action failed: {error}");
            std::process::exit(1);
        }
    }
    let root = paths::delivery_root().unwrap_or_else(|error| {
        eprintln!("无法确定交付目录：{error}");
        std::process::exit(1);
    });
    let instance = delivery::acquire_launcher_instance().unwrap_or_else(|error| {
        eprintln!("无法取得 Launcher 单实例所有权：{error}");
        std::process::exit(1);
    });
    let Some(instance) = instance else {
        return;
    };
    if delivery::current_release_ready_for_launcher(&root) {
        if let Err(error) = delivery::run_manager_broker(root, instance) {
            eprintln!("Launcher Broker failed: {error}");
        }
        return;
    }

    tauri::Builder::default()
        .manage(delivery::LauncherState::default())
        .setup(|app| {
            WebviewWindowBuilder::new(app, "launcher", WebviewUrl::App("launcher.html".into()))
                .title("tauri-codex")
                .inner_size(600.0, 420.0)
                .min_inner_size(460.0, 340.0)
                .resizable(true)
                .build()?;
            delivery::start_launcher_setup(
                app.handle().clone(),
                app.state::<delivery::LauncherState>().inner(),
            )
            .map_err(std::io::Error::other)?;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            delivery::broker::get_launcher_status,
            delivery::broker::retry_launcher_setup
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri-codex launcher");
    if let Ok(root) = paths::delivery_root() {
        if delivery::current_release_ready_for_launcher(&root) {
            let _ = delivery::run_manager_broker(root, instance);
        }
    }
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

#[cfg(test)]
mod argument_tests {
    use super::{manager_verification_action, ManagerVerificationAction};
    use std::ffi::OsString;

    #[test]
    fn manager_verification_requires_one_absolute_path() {
        let path = std::env::current_dir().unwrap().join("signed.exe");
        assert_eq!(
            manager_verification_action(&[
                OsString::from("--verify-authenticode"),
                path.clone().into_os_string(),
            ]),
            Ok(Some(ManagerVerificationAction::Authenticode(path.clone())))
        );
        assert_eq!(
            manager_verification_action(&[
                OsString::from("--verify-codex-component"),
                path.clone().into_os_string(),
            ]),
            Ok(Some(ManagerVerificationAction::CodexComponent(path)))
        );
        assert!(
            manager_verification_action(&[OsString::from("--verify-authenticode")])
                .unwrap_err()
                .contains("一个绝对路径")
        );
        assert!(manager_verification_action(&[
            OsString::from("--verify-authenticode"),
            OsString::from("signed.exe"),
        ])
        .unwrap_err()
        .contains("绝对路径"));
        assert!(manager_verification_action(&[
            OsString::from("--verify-authenticode"),
            OsString::from("C:\\signed.exe"),
            OsString::from("unexpected"),
        ])
        .unwrap_err()
        .contains("一个绝对路径"));
    }
}
