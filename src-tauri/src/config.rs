use crate::secrets::{protect_secret, unprotect_secret};
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

fn default_icon() -> String {
    "modern_glass".to_string()
}

fn default_true() -> bool {
    true
}

fn default_id() -> String {
    "srv-1".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerTarget {
    #[serde(default = "default_id")]
    pub id: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
    pub server_label: String,
    pub host: String,
    pub sql_port: u16,
    #[serde(default)]
    pub sql_instance: String,
    pub windows_service_name: String,
    #[serde(default = "default_true")]
    pub check_windows_service: bool,
    #[serde(default = "default_true")]
    pub win_auth_enabled: bool,
    #[serde(default)]
    pub win_domain: String,
    #[serde(default)]
    pub win_username: String,
    #[serde(default)]
    pub win_password: String,
    #[serde(default)]
    pub sql_auth_mode: String,
    #[serde(default)]
    pub sql_username: String,
    #[serde(default)]
    pub sql_password: String,
    #[serde(default = "default_true")]
    pub collect_sql_metrics: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonitorConfig {
    #[serde(default = "default_icon")]
    pub app_icon: String,
    pub server_label: String,
    pub host: String,
    pub sql_port: u16,
    pub sql_instance: String,
    pub windows_service_name: String,
    pub check_windows_service: bool,
    pub win_auth_enabled: bool,
    pub win_domain: String,
    pub win_username: String,
    pub win_password: String,
    pub sql_auth_mode: String,
    pub sql_username: String,
    pub sql_password: String,
    pub collect_sql_metrics: bool,
    #[serde(default)]
    pub servers: Vec<ServerTarget>,
    pub ping_interval_secs: u64,
    pub sql_interval_secs: u64,
    pub resource_interval_secs: u64,
    pub timeout_ms: u64,
    pub warning_latency_ms: u64,
    pub critical_latency_ms: u64,
    pub warning_cpu_percent: u8,
    pub warning_ram_percent: u8,
    pub min_disk_free_percent: u8,
    pub max_blocked_sessions: u32,
    pub consecutive_failures_for_alert: u32,
    pub alert_cooldown_mins: u64,
    pub enable_toast_alerts: bool,
    pub enable_sound_alert: bool,
    pub start_with_windows: bool,
    pub minimize_to_tray_on_close: bool,
}

impl MonitorConfig {
    pub fn effective_servers(&self) -> Vec<ServerTarget> {
        if !self.servers.is_empty() {
            return self.servers.iter().take(3).cloned().collect();
        }
        vec![ServerTarget {
            id: "srv-1".to_string(),
            enabled: true,
            server_label: self.server_label.clone(),
            host: self.host.clone(),
            sql_port: self.sql_port,
            sql_instance: self.sql_instance.clone(),
            windows_service_name: self.windows_service_name.clone(),
            check_windows_service: self.check_windows_service,
            win_auth_enabled: self.win_auth_enabled,
            win_domain: self.win_domain.clone(),
            win_username: self.win_username.clone(),
            win_password: self.win_password.clone(),
            sql_auth_mode: self.sql_auth_mode.clone(),
            sql_username: self.sql_username.clone(),
            sql_password: self.sql_password.clone(),
            collect_sql_metrics: self.collect_sql_metrics,
        }]
    }

    pub fn resolve_server(&self, server_id: Option<&str>) -> ServerTarget {
        let list = self.effective_servers();
        if let Some(id) = server_id {
            if let Some(found) = list.iter().find(|s| s.id == id) {
                return found.clone();
            }
        }
        list.into_iter().next().unwrap()
    }
}

impl Default for MonitorConfig {
    fn default() -> Self {
        serde_json::from_str(include_str!("../../default-config.json")).unwrap_or_else(|_| {
            let primary = ServerTarget {
                id: "srv-1".to_string(),
                enabled: true,
                server_label: "SERVERSAT".to_string(),
                host: "10.12.9.4".to_string(),
                sql_port: 1433,
                sql_instance: "".to_string(),
                windows_service_name: "MSSQLSERVER".to_string(),
                check_windows_service: true,
                win_auth_enabled: true,
                win_domain: ".".to_string(),
                win_username: "Administrador".to_string(),
                win_password: String::new(),
                sql_auth_mode: "sql".to_string(),
                sql_username: "sa".to_string(),
                sql_password: String::new(),
                collect_sql_metrics: true,
            };
            Self {
                app_icon: default_icon(),
                server_label: primary.server_label.clone(),
                host: primary.host.clone(),
                sql_port: primary.sql_port,
                sql_instance: primary.sql_instance.clone(),
                windows_service_name: primary.windows_service_name.clone(),
                check_windows_service: primary.check_windows_service,
                win_auth_enabled: primary.win_auth_enabled,
                win_domain: primary.win_domain.clone(),
                win_username: primary.win_username.clone(),
                win_password: String::new(),
                sql_auth_mode: primary.sql_auth_mode.clone(),
                sql_username: primary.sql_username.clone(),
                sql_password: String::new(),
                collect_sql_metrics: primary.collect_sql_metrics,
                servers: vec![primary],
                ping_interval_secs: 10,
                sql_interval_secs: 30,
                resource_interval_secs: 30,
                timeout_ms: 2500,
                warning_latency_ms: 100,
                critical_latency_ms: 250,
                warning_cpu_percent: 85,
                warning_ram_percent: 90,
                min_disk_free_percent: 15,
                max_blocked_sessions: 3,
                consecutive_failures_for_alert: 3,
                alert_cooldown_mins: 5,
                enable_toast_alerts: true,
                enable_sound_alert: true,
                start_with_windows: true,
                minimize_to_tray_on_close: true,
            }
        })
    }
}

fn config_file_path() -> PathBuf {
    if let Some(proj) = ProjectDirs::from("com", "windowssql", "WindowsSQLMonitor") {
        let dir = proj.config_dir();
        let _ = fs::create_dir_all(dir);
        return dir.join("config.json");
    }
    PathBuf::from("config.json")
}

pub fn load_config() -> MonitorConfig {
    let path = config_file_path();
    let mut cfg: MonitorConfig = match fs::read_to_string(&path) {
        Ok(raw) => serde_json::from_str(&raw).unwrap_or_default(),
        Err(_) => {
            let defaults = MonitorConfig::default();
            let _ = save_config(&defaults);
            defaults
        }
    };
    cfg.win_password = unprotect_secret(&cfg.win_password);
    cfg.sql_password = unprotect_secret(&cfg.sql_password);
    if cfg.servers.is_empty() {
        cfg.servers = cfg.effective_servers();
    } else {
        cfg.servers.truncate(3);
        for srv in &mut cfg.servers {
            srv.win_password = unprotect_secret(&srv.win_password);
            srv.sql_password = unprotect_secret(&srv.sql_password);
        }
    }
    cfg
}

pub fn save_config(cfg: &MonitorConfig) -> Result<(), String> {
    let path = config_file_path();
    let mut normalized = cfg.clone();
    if normalized.servers.is_empty() {
        normalized.servers = normalized.effective_servers();
    }
    normalized.servers.truncate(3);
    if let Some(first) = normalized.servers.first() {
        normalized.server_label = first.server_label.clone();
        normalized.host = first.host.clone();
        normalized.sql_port = first.sql_port;
        normalized.sql_instance = first.sql_instance.clone();
        normalized.windows_service_name = first.windows_service_name.clone();
        normalized.check_windows_service = first.check_windows_service;
        normalized.win_auth_enabled = first.win_auth_enabled;
        normalized.win_domain = first.win_domain.clone();
        normalized.win_username = first.win_username.clone();
        normalized.win_password = first.win_password.clone();
        normalized.sql_auth_mode = first.sql_auth_mode.clone();
        normalized.sql_username = first.sql_username.clone();
        normalized.sql_password = first.sql_password.clone();
        normalized.collect_sql_metrics = first.collect_sql_metrics;
    }

    let mut encrypted = normalized;
    encrypted.win_password = protect_secret(&encrypted.win_password);
    encrypted.sql_password = protect_secret(&encrypted.sql_password);
    for srv in &mut encrypted.servers {
        srv.win_password = protect_secret(&srv.win_password);
        srv.sql_password = protect_secret(&srv.sql_password);
    }
    let json = serde_json::to_string_pretty(&encrypted).map_err(|e| e.to_string())?;
    fs::write(path, json).map_err(|e| e.to_string())
}
