#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

#[cfg(all(not(debug_assertions), not(feature = "custom-protocol")))]
compile_error!("release Manager builds must enable the custom-protocol feature");

fn main() {
    tauri_codex_lib::run_manager();
}
