// Prevents an additional console window on Windows in release.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    #[cfg(debug_assertions)]
    tauri_codex_lib::run_manager();
    #[cfg(not(debug_assertions))]
    tauri_codex_lib::run_launcher();
}
