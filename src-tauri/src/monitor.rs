use crate::config::{MonitorConfig, ServerTarget};
use crate::ping::icmp_ping;
use crate::sql_metrics::{collect_sql_resources, SqlResourceMetrics};
use crate::sql_probe::probe_sql_server;
use crate::tray::{update_tray_visual, TrayVisualState};
use crate::win_metrics::{collect_windows_resources, WindowsResourceMetrics};
use chrono::Utc;
use serde::Serialize;
use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter};
use tauri_plugin_notification::NotificationExt;

#[derive(Debug, Clone, Serialize)]
pub struct ProbeSample {
    pub server_id: String,
    pub server_label: String,
    pub timestamp: String,
    pub ping_reachable: bool,
    pub ping_latency_ms: Option<u64>,
    pub sql_reachable: bool,
    pub sql_latency_ms: Option<u64>,
    pub sql_version: Option<String>,
    pub service_running: bool,
    pub windows_resources: Option<WindowsResourceMetrics>,
    pub sql_resources: Option<SqlResourceMetrics>,
    pub resource_note: Option<String>,
    pub status: String,
    pub message: String,
}

#[derive(Debug, Clone)]
pub struct SharedMonitorState {
    pub config: MonitorConfig,
    pub paused: bool,
    pub consecutive_failures_by_server: HashMap<String, u32>,
    pub last_alert_at_by_server: HashMap<String, Instant>,
    pub was_outage_by_server: HashMap<String, bool>,
    pub history: VecDeque<ProbeSample>,
    pub latest_by_server: HashMap<String, ProbeSample>,
}

impl SharedMonitorState {
    pub fn new(config: MonitorConfig) -> Self {
        Self {
            config,
            paused: false,
            consecutive_failures_by_server: HashMap::new(),
            last_alert_at_by_server: HashMap::new(),
            was_outage_by_server: HashMap::new(),
            history: VecDeque::with_capacity(300),
            latest_by_server: HashMap::new(),
        }
    }
}

struct RawServerProbe {
    server: ServerTarget,
    ping_reachable: bool,
    ping_latency_ms: Option<u64>,
    ping_detail: String,
    sql_res: Option<crate::sql_probe::SqlProbeResult>,
    win_res: Option<WindowsResourceMetrics>,
    sql_metrics: Option<SqlResourceMetrics>,
    current_note: Option<String>,
}

async fn probe_single_server(
    srv: ServerTarget,
    timeout_ms: u64,
    run_sql: bool,
    run_resources: bool,
) -> RawServerProbe {
    let ping_res = icmp_ping(srv.host.clone(), timeout_ms).await;
    let sql_res = if run_sql {
        Some(
            probe_sql_server(
                &srv.host,
                srv.sql_port,
                &srv.sql_instance,
                &srv.windows_service_name,
                srv.check_windows_service,
                timeout_ms,
            )
            .await,
        )
    } else {
        None
    };

    let mut notes: Vec<String> = Vec::new();
    let is_local = matches!(
        srv.host.trim().to_ascii_lowercase().as_str(),
        "localhost" | "127.0.0.1" | "::1" | "."
    );
    let win_res = if run_resources && srv.win_auth_enabled && (!srv.win_username.is_empty() || is_local) {
        match collect_windows_resources(
            srv.host.clone(),
            srv.win_domain.clone(),
            srv.win_username.clone(),
            srv.win_password.clone(),
        )
        .await
        {
            Ok(w) => Some(w),
            Err(e) => {
                notes.push(format!("Windows OS: {}", e));
                None
            }
        }
    } else {
        None
    };

    let sql_port_effective = sql_res.as_ref().map(|s| s.resolved_port).unwrap_or(srv.sql_port);
    let sql_metrics = if run_resources && srv.collect_sql_metrics && !srv.sql_username.is_empty() {
        match collect_sql_resources(&srv.host, sql_port_effective, &srv.sql_username, &srv.sql_password).await {
            Ok(m) => Some(m),
            Err(e) => {
                notes.push(format!("SQL DMVs: {}", e));
                None
            }
        }
    } else {
        None
    };
    let current_note = if notes.is_empty() { None } else { Some(notes.join(" · ")) };

    RawServerProbe {
        server: srv,
        ping_reachable: ping_res.reachable,
        ping_latency_ms: ping_res.latency_ms,
        ping_detail: ping_res.detail,
        sql_res,
        win_res,
        sql_metrics,
        current_note,
    }
}

pub async fn execute_probe_cycle(
    app: &AppHandle,
    state: &Arc<Mutex<SharedMonitorState>>,
    run_sql: bool,
    run_resources: bool,
) -> Vec<ProbeSample> {
    let cfg = {
        let guard = state.lock().unwrap();
        guard.config.clone()
    };

    let enabled_servers: Vec<ServerTarget> = cfg
        .effective_servers()
        .into_iter()
        .filter(|s| s.enabled)
        .take(3)
        .collect();

    if enabled_servers.is_empty() {
        update_tray_visual(app, TrayVisualState::Paused, "Windows-SQL Monitor · Sin servidores activos");
        return Vec::new();
    }

    let mut handles = Vec::new();
    let timeout_ms = cfg.timeout_ms;
    for srv in enabled_servers {
        handles.push(tokio::spawn(probe_single_server(srv, timeout_ms, run_sql, run_resources)));
    }

    let mut raw_results = Vec::new();
    for h in handles {
        if let Ok(r) = h.await {
            raw_results.push(r);
        }
    }

    let (samples, worst_tray, alerts_to_send, tooltip_text) = {
        let mut guard = state.lock().unwrap();
        let mut collected_samples = Vec::new();
        let mut worst = TrayVisualState::Nominal;
        let mut alerts: Vec<(&'static str, String)> = Vec::new();

        for raw in raw_results {
            let srv = raw.server;
            let sid = srv.id.clone();

            let fails = {
                let entry = guard.consecutive_failures_by_server.entry(sid.clone()).or_insert(0);
                if !raw.ping_reachable {
                    *entry += 1;
                } else {
                    *entry = 0;
                }
                *entry
            };

            let prev_sample = guard.latest_by_server.get(&sid).cloned();
            let prev_sql_ok = prev_sample.as_ref().map(|s| s.sql_reachable).unwrap_or(true);
            let sql_ok = raw
                .sql_res
                .as_ref()
                .map(|s| s.reachable && s.service_running)
                .unwrap_or(prev_sql_ok);
            let sql_port_effective = raw.sql_res.as_ref().map(|s| s.resolved_port).unwrap_or(srv.sql_port);

            let cooldown_ok = guard
                .last_alert_at_by_server
                .get(&sid)
                .map(|t| t.elapsed() >= Duration::from_secs(cfg.alert_cooldown_mins * 60))
                .unwrap_or(true);

            let mut status_label = "nominal";
            let mut srv_tray = TrayVisualState::Nominal;
            let mut alert_msg: Option<(&'static str, String)> = None;

            if !raw.ping_reachable && fails >= cfg.consecutive_failures_for_alert {
                status_label = "critical";
                srv_tray = TrayVisualState::Critical;
                guard.was_outage_by_server.insert(sid.clone(), true);
                if cfg.enable_toast_alerts && cooldown_ok {
                    guard.last_alert_at_by_server.insert(sid.clone(), Instant::now());
                    alert_msg = Some((
                        "Interrupción de red detectada",
                        format!("{} ({}) no responde al ping tras {} intentos.", srv.server_label, srv.host, fails),
                    ));
                }
            } else if !sql_ok {
                status_label = "critical";
                srv_tray = TrayVisualState::Critical;
                guard.was_outage_by_server.insert(sid.clone(), true);
                if cfg.enable_toast_alerts && cooldown_ok {
                    guard.last_alert_at_by_server.insert(sid.clone(), Instant::now());
                    alert_msg = Some((
                        "Servicio SQL Server caído",
                        format!("{} ({}): {} no responde en el puerto {}.", srv.server_label, srv.host, srv.windows_service_name, sql_port_effective),
                    ));
                }
            } else if let Some(ms) = raw.ping_latency_ms {
                if ms >= cfg.critical_latency_ms {
                    status_label = "critical";
                    srv_tray = TrayVisualState::Critical;
                    if cfg.enable_toast_alerts && cooldown_ok {
                        guard.last_alert_at_by_server.insert(sid.clone(), Instant::now());
                        alert_msg = Some((
                            "Latencia crítica en servidor",
                            format!("{} registra {} ms (umbral crítico: {} ms).", srv.server_label, ms, cfg.critical_latency_ms),
                        ));
                    }
                } else if ms >= cfg.warning_latency_ms {
                    status_label = "warning";
                    srv_tray = TrayVisualState::Warning;
                    if cfg.enable_toast_alerts && cooldown_ok {
                        guard.last_alert_at_by_server.insert(sid.clone(), Instant::now());
                        alert_msg = Some((
                            "Latencia elevada detectada",
                            format!("{} presenta {} ms de ping (umbral: {} ms).", srv.server_label, ms, cfg.warning_latency_ms),
                        ));
                    }
                } else if guard.was_outage_by_server.get(&sid).copied().unwrap_or(false) && cfg.enable_toast_alerts {
                    guard.was_outage_by_server.insert(sid.clone(), false);
                    alert_msg = Some((
                        "Conexión restablecida",
                        format!("{} y SQL Server volvieron a operar con normalidad ({} ms).", srv.server_label, ms),
                    ));
                }
            }

            if let Some(ref wr) = raw.win_res {
                if wr.cpu_usage_percent >= cfg.warning_cpu_percent as f64 || wr.ram_usage_percent >= cfg.warning_ram_percent as f64 {
                    if status_label == "nominal" {
                        status_label = "warning";
                        srv_tray = TrayVisualState::Warning;
                    }
                    if alert_msg.is_none() && cfg.enable_toast_alerts && cooldown_ok {
                        guard.last_alert_at_by_server.insert(sid.clone(), Instant::now());
                        alert_msg = Some((
                            "Uso elevado de recursos en Windows Server",
                            format!("{} · CPU: {:.0}% · RAM: {:.0}%", srv.server_label, wr.cpu_usage_percent, wr.ram_usage_percent),
                        ));
                    }
                }
            }

            if let Some(ref sr) = raw.sql_metrics {
                if sr.blocked_processes >= cfg.max_blocked_sessions {
                    if status_label == "nominal" {
                        status_label = "warning";
                        srv_tray = TrayVisualState::Warning;
                    }
                    if alert_msg.is_none() && cfg.enable_toast_alerts && cooldown_ok {
                        guard.last_alert_at_by_server.insert(sid.clone(), Instant::now());
                        alert_msg = Some((
                            "Sesiones bloqueadas en SQL Server",
                            format!("{} tiene {} procesos bloqueados (umbral: {}).", srv.server_label, sr.blocked_processes, cfg.max_blocked_sessions),
                        ));
                    }
                }
            }

            match (worst, srv_tray) {
                (_, TrayVisualState::Critical) => worst = TrayVisualState::Critical,
                (TrayVisualState::Nominal, TrayVisualState::Warning) => worst = TrayVisualState::Warning,
                _ => {}
            }

            if let Some(a) = alert_msg {
                alerts.push(a);
            }

            let prev_win = prev_sample.as_ref().and_then(|s| s.windows_resources.clone());
            let prev_sql_m = prev_sample.as_ref().and_then(|s| s.sql_resources.clone());
            let prev_note = if !run_resources {
                prev_sample.as_ref().and_then(|s| s.resource_note.clone())
            } else {
                None
            };

            let sample = ProbeSample {
                server_id: sid.clone(),
                server_label: srv.server_label.clone(),
                timestamp: Utc::now().to_rfc3339(),
                ping_reachable: raw.ping_reachable,
                ping_latency_ms: raw.ping_latency_ms,
                sql_reachable: sql_ok,
                sql_latency_ms: raw.sql_res.as_ref().and_then(|s| s.latency_ms),
                sql_version: raw.sql_res.as_ref().and_then(|s| s.sql_version.clone()),
                service_running: sql_ok,
                windows_resources: raw.win_res.or(prev_win),
                sql_resources: raw.sql_metrics.or(prev_sql_m),
                resource_note: raw.current_note.or(prev_note),
                status: status_label.to_string(),
                message: raw
                    .sql_res
                    .as_ref()
                    .map(|s| s.detail.clone())
                    .unwrap_or(raw.ping_detail),
            };

            if guard.history.len() >= 300 {
                guard.history.pop_front();
            }
            guard.history.push_back(sample.clone());
            guard.latest_by_server.insert(sid, sample.clone());
            collected_samples.push(sample);
        }

        let summary_lines: Vec<String> = collected_samples
            .iter()
            .map(|s| {
                let p = s.ping_latency_ms.map(|m| format!("{}ms", m)).unwrap_or_else(|| "Err".into());
                let sq = if s.sql_reachable { "OK" } else { "Caido" };
                format!("{}: {} · SQL {}", s.server_label, p, sq)
            })
            .collect();
        let tooltip = format!("Windows-SQL Monitor v{}\n{}", env!("CARGO_PKG_VERSION"), summary_lines.join("\n"));

        (collected_samples, worst, alerts, tooltip)
    };

    update_tray_visual(app, worst_tray, &tooltip_text);
    let _ = app.emit("probe-updated", ());

    for (title, body) in alerts_to_send {
        let _ = app.notification().builder().title(title).body(body).show();
    }

    samples
}

pub fn spawn_monitor_loop(app: AppHandle, state: Arc<Mutex<SharedMonitorState>>) {
    tauri::async_runtime::spawn(async move {
        let mut last_sql_check = Instant::now() - Duration::from_secs(3600);
        let mut last_resource_check = Instant::now() - Duration::from_secs(3600);
        loop {
            let (paused, ping_secs, sql_secs, res_secs) = {
                let guard = state.lock().unwrap();
                (
                    guard.paused,
                    guard.config.ping_interval_secs.max(2),
                    guard.config.sql_interval_secs.max(5),
                    guard.config.resource_interval_secs.max(10),
                )
            };

            if !paused {
                let run_sql = last_sql_check.elapsed() >= Duration::from_secs(sql_secs);
                let run_res = last_resource_check.elapsed() >= Duration::from_secs(res_secs);
                if run_sql {
                    last_sql_check = Instant::now();
                }
                if run_res {
                    last_resource_check = Instant::now();
                }
                let _ = execute_probe_cycle(&app, &state, run_sql, run_res).await;
            } else {
                update_tray_visual(&app, TrayVisualState::Paused, "Windows-SQL Monitor · En pausa");
                let _ = app.emit("probe-updated", ());
            }

            tokio::time::sleep(Duration::from_secs(ping_secs)).await;
        }
    });
}
