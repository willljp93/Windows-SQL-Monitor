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
    configure_remote_firewall, control_sql_service, diagnose_ports, list_sql_services,
    restart_windows_server, test_sql_credentials, test_windows_credentials, DiagnosticPortCheck,
    OpResult, SqlServicesSnapshot,
};
use config::{load_config, save_config, MonitorConfig};
use monitor::{execute_probe_cycle, spawn_monitor_loop, ProbeSample, SharedMonitorState};
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter, Manager, State, WindowEvent};
use tauri_plugin_autostart::{MacosLauncher, ManagerExt};

struct AppState(Arc<Mutex<SharedMonitorState>>);

#[derive(Debug, Clone, serde::Serialize)]
struct TrayServerSummary {
    id: String,
    label: String,
    host: String,
    port: u16,
    enabled: bool,
    status: String,
    status_text: String,
    ping_text: String,
}

#[derive(Debug, Clone, serde::Serialize)]
struct TraySnapshot {
    paused: bool,
    active_cpu_ram: String,
    servers: Vec<TrayServerSummary>,
}

#[tauri::command]
fn get_app_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

#[tauri::command]
fn get_config(state: State<AppState>) -> MonitorConfig {
    state.0.lock().unwrap().config.clone()
}

#[tauri::command]
fn get_paused(state: State<AppState>) -> bool {
    state.0.lock().unwrap().paused
}

#[tauri::command]
fn toggle_paused(app: AppHandle, state: State<AppState>) -> bool {
    let paused = {
        let mut guard = state.0.lock().unwrap();
        guard.paused = !guard.paused;
        guard.paused
    };
    let _ = app.emit("probe-updated", ());
    paused
}

#[tauri::command]
fn get_tray_snapshot(state: State<AppState>) -> TraySnapshot {
    let guard = state.0.lock().unwrap();
    let servers_list = guard.config.effective_servers();
    let items: Vec<TrayServerSummary> = servers_list
        .iter()
        .map(|s| {
            let latest = guard.latest_by_server.get(&s.id);
            let status = if !s.enabled || guard.paused {
                "muted".to_string()
            } else {
                latest.map(|l| l.status.clone()).unwrap_or_else(|| "muted".to_string())
            };
            let status_text = if !s.enabled {
                "Deshabilitado".to_string()
            } else if guard.paused {
                "En pausa".to_string()
            } else if let Some(l) = latest {
                if l.sql_reachable {
                    "SQL OK".to_string()
                } else {
                    "SQL Caído".to_string()
                }
            } else {
                "Sin sondeos".to_string()
            };
            let ping_text = if !s.enabled || guard.paused {
                String::new()
            } else if let Some(l) = latest {
                l.ping_latency_ms
                    .map(|m| format!("{} ms", m))
                    .unwrap_or_else(|| "Err".to_string())
            } else {
                "—".to_string()
            };
            TrayServerSummary {
                id: s.id.clone(),
                label: s.server_label.clone(),
                host: s.host.clone(),
                port: s.sql_port,
                enabled: s.enabled,
                status,
                status_text,
                ping_text,
            }
        })
        .collect();

    let active_cpu_ram = servers_list
        .iter()
        .find_map(|s| {
            guard.latest_by_server.get(&s.id).and_then(|smp| {
                smp.windows_resources.as_ref().map(|wr| {
                    format!(
                        "{}: CPU {:.0}% · RAM {:.0}%",
                        s.server_label, wr.cpu_usage_percent, wr.ram_usage_percent
                    )
                })
            })
        })
        .unwrap_or_default();

    TraySnapshot {
        paused: guard.paused,
        active_cpu_ram,
        servers: items,
    }
}

#[tauri::command]
fn cmd_open_main_tab(app: AppHandle, tab: String) {
    if let Some(tray_win) = app.get_webview_window("tray") {
        let _ = tray_win.hide();
    }
    if let Some(win) = app.get_webview_window("main") {
        let _ = win.show();
        let _ = win.set_focus();
    }
    let _ = app.emit("navigate-tab", tab);
}

#[tauri::command]
fn cmd_quit_app(app: AppHandle) {
    app.exit(0);
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
async fn cmd_list_sql_services(
    state: State<'_, AppState>,
    server_id: Option<String>,
) -> Result<SqlServicesSnapshot, String> {
    let srv = state
        .0
        .lock()
        .unwrap()
        .config
        .resolve_server(server_id.as_deref());
    Ok(list_sql_services(
        srv.host,
        srv.win_domain,
        srv.win_username,
        srv.win_password,
        srv.windows_service_name,
        srv.sql_instance,
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
            get_app_version,
            get_config,
            get_paused,
            toggle_paused,
            get_tray_snapshot,
            cmd_open_main_tab,
            cmd_quit_app,
            save_app_config,
            get_history,
            run_probe_now,
            cmd_test_windows,
            cmd_test_sql,
            cmd_diagnose_ports,
            cmd_fix_firewall,
            cmd_list_sql_services,
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
            if window.label() == "tray" {
                if let WindowEvent::Focused(false) = event {
                    let _ = window.hide();
                }
                return;
            }
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
