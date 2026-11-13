// Entry point; everything lives in lib.rs. (Tauri template line, harmless on macOS.)
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    ssd_workspace_lib::run()
}
