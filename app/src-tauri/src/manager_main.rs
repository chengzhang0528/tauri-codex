#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    tauri_codex_lib::run_manager();
}
