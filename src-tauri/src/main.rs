#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    windows_sql_monitor_lib::run();
}
