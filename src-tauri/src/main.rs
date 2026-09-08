// Release builds are GUI programs: no console window attached at start.
// Debug builds keep the console so `tauri dev` can print to it.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    demodesk_lib::run();
}
