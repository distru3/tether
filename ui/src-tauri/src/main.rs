// Prevents a second console window on release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    screentime_ui_lib::run();
}
