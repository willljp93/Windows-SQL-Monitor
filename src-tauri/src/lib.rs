mod admin_ops;
mod config;
mod monitor;
mod ping;
mod secrets;
mod sql_metrics;
mod sql_probe;
mod tray;
mod win_metrics;

use admin_ops::{
    configure_remote_firewall, control_sql_service, diagnose_ports, restart_windows_server,
    test_sql_credentials, test_windows_credentials, DiagnosticPortCheck, OpResult,
};
use config::{load_config, save_config, MonitorConfig};
use monitor::{execute_probe_cycle, spawn_monitor_loop, ProbeSample, SharedMonitorState};
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Manager, State, WindowEvent};
use tauri_plugin_autostart::{MacosLauncher, ManagerExt};

struct AppState(Arc<Mutex<SharedMonitorState>>);

#[tauri::command]
fn get_config(state: State<AppState>) -> MonitorConfig {
    state.0.lock().unwrap().config.clone()
}

#[tauri::command]
fn get_paused(state: State<AppState>) -> bool {
    state.0.lock().unwrap().paused
}

#[tauri::command]
fn toggle_paused(state: State<AppState>) -> bool {
    let mut guard = state.0.lock().unwrap();
    guard.paused = !guard.paused;
    guard.paused
}

#[tauri::command]
fn save_app_config(
    app: AppHandle,
    state: State<AppState>,
    new_config: MonitorConfig,
) -> Result<MonitorConfig, String> {
    save_config(&new_config)?;
    let autostart = app.autolaunch();
    if new_config.start_with_windows {
        let _ = autostart.enable();
    } else {
        let _ = autostart.disable();
    }
    let mut guard = state.0.lock().unwrap();
    guard.config = new_config.clone();
    Ok(new_config)
}

#[tauri::command]
fn get_history(state: State<AppState>) -> Vec<ProbeSample> {
    state.0.lock().unwrap().history.iter().cloned().collect()
}

#[tauri::command]
async fn run_probe_now(app: AppHandle, state: State<'_, AppState>) -> Result<Vec<ProbeSample>, String> {
    Ok(execute_probe_cycle(&app, &state.0, true, true).await)
}

#[tauri::command]
async fn cmd_test_windows(
    host: String,
    domain: String,
    username: String,
    password: String,
) -> OpResult {
    test_windows_credentials(host, domain, username, password).await
}

#[tauri::command]
async fn cmd_test_sql(host: String, port: u16, username: String, password: String) -> OpResult {
    test_sql_credentials(host, port, username, password).await
}

#[tauri::command]
async fn cmd_diagnose_ports(
    host: String,
    port: u16,
    timeout_ms: u64,
) -> Vec<DiagnosticPortCheck> {
    diagnose_ports(host, port, timeout_ms).await
}

#[tauri::command]
async fn cmd_fix_firewall(
    state: State<'_, AppState>,
    server_id: Option<String>,
) -> Result<OpResult, String> {
    let srv = state
        .0
        .lock()
        .unwrap()
        .config
        .resolve_server(server_id.as_deref());
    Ok(configure_remote_firewall(
        srv.host,
        srv.win_domain,
        srv.win_username,
        srv.win_password,
        srv.sql_port,
    )
    .await)
}

#[tauri::command]
async fn cmd_control_sql_service(
    state: State<'_, AppState>,
    server_id: Option<String>,
    service_name: String,
    action: String,
) -> Result<OpResult, String> {
    let srv = state
        .0
        .lock()
        .unwrap()
        .config
        .resolve_server(server_id.as_deref());
    Ok(control_sql_service(
        srv.host,
        srv.win_domain,
        srv.win_username,
        srv.win_password,
        service_name,
        action,
    )
    .await)
}

#[tauri::command]
async fn cmd_restart_windows(
    state: State<'_, AppState>,
    server_id: Option<String>,
) -> Result<OpResult, String> {
    let srv = state
        .0
        .lock()
        .unwrap()
        .config
        .resolve_server(server_id.as_deref());
    Ok(restart_windows_server(srv.host, srv.win_domain, srv.win_username, srv.win_password).await)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let initial_config = load_config();
    let shared_state = Arc::new(Mutex::new(SharedMonitorState::new(initial_config.clone())));

    tauri::Builder::default()
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_autostart::init(
            MacosLauncher::LaunchAgent,
            Some(vec!["--minimized"]),
        ))
        .manage(AppState(shared_state.clone()))
        .invoke_handler(tauri::generate_handler![
            get_config,
            get_paused,
            toggle_paused,
            save_app_config,
            get_history,
            run_probe_now,
            cmd_test_windows,
            cmd_test_sql,
            cmd_diagnose_ports,
            cmd_fix_firewall,
            cmd_control_sql_service,
            cmd_restart_windows
        ])
        .setup(move |app| {
            tray::setup_tray(app.handle(), shared_state.clone())?;
            spawn_monitor_loop(app.handle().clone(), shared_state.clone());

            let args: Vec<String> = std::env::args().collect();
            if let Some(win) = app.get_webview_window("main") {
                if let Ok(win_icon) = tauri::image::Image::from_bytes(include_bytes!("../icons/128x128.png")) {
                    let _ = win.set_icon(win_icon);
                }
                if !args.iter().any(|a| a == "--minimized") {
                    let _ = win.show();
                }
            }
            Ok(())
        })
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                let state: State<AppState> = window.state();
                let minimize = state.0.lock().unwrap().config.minimize_to_tray_on_close;
                if minimize {
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
        })
        .run(tauri::generate_context!())
        .expect("Error ejecutando Windows-SQL Monitor");
}
