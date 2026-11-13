use serde::Serialize;
use std::time::{Duration, Instant};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpStream, UdpSocket};

#[derive(Debug, Clone, Serialize)]
pub struct SqlProbeResult {
    pub reachable: bool,
    pub latency_ms: Option<u64>,
    pub service_running: bool,
    pub sql_version: Option<String>,
    pub resolved_port: u16,
    pub detail: String,
}

const TDS_PRELOGIN_PACKET: [u8; 49] = [
    0x12, 0x01, 0x00, 0x31, 0x00, 0x00, 0x01, 0x00,
    0x00, 0x00, 0x1A, 0x00, 0x06,
    0x01, 0x00, 0x20, 0x00, 0x01,
    0x02, 0x00, 0x21, 0x00, 0x01,
    0x03, 0x00, 0x22, 0x00, 0x04,
    0x04, 0x00, 0x26, 0x00, 0x01,
    0xFF,
    0x09, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x02,
    0x00,
    0x00, 0x00, 0x00, 0x00,
    0x00,
    0x00, 0x00,
];

async fn resolve_instance_port(host: &str, instance: &str, timeout_ms: u64) -> Option<u16> {
    if instance.trim().is_empty() || instance.eq_ignore_ascii_case("MSSQLSERVER") {
        return None;
    }
    let socket = UdpSocket::bind("0.0.0.0:0").await.ok()?;
    let mut query = vec![0x04u8];
    query.extend_from_slice(instance.as_bytes());
    let target = format!("{}:1434", host);
    socket.send_to(&query, &target).await.ok()?;

    let mut buf = [0u8; 1024];
    let recv = tokio::time::timeout(
        Duration::from_millis(timeout_ms),
        socket.recv_from(&mut buf),
    )
    .await
    .ok()?
    .ok()?;

    let text = String::from_utf8_lossy(&buf[3..recv.0]);
    let parts: Vec<&str> = text.split(';').collect();
    for pair in parts.windows(2) {
        if pair[0].eq_ignore_ascii_case("tcp") {
            if let Ok(port) = pair[1].parse::<u16>() {
                return Some(port);
            }
        }
    }
    None
}

#[cfg(windows)]
fn check_windows_service_sc(host: &str, service_name: &str) -> Option<bool> {
    use std::os::windows::process::CommandExt;
    use std::process::Command;
    const CREATE_NO_WINDOW: u32 = 0x08000000;

    let server_arg = format!("\\\\{}", host);
    let output = Command::new("sc.exe")
        .args([&server_arg, "query", service_name])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .ok()?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    if stdout.contains("RUNNING") {
        Some(true)
    } else if stdout.contains("STOPPED") || stdout.contains("PAUSED") {
        Some(false)
    } else {
        None
    }
}

#[cfg(not(windows))]
fn check_windows_service_sc(_host: &str, _service_name: &str) -> Option<bool> {
    None
}

pub async fn probe_sql_server(
    host: &str,
    configured_port: u16,
    instance: &str,
    service_name: &str,
    check_win_service: bool,
    timeout_ms: u64,
) -> SqlProbeResult {
    let port = resolve_instance_port(host, instance, timeout_ms)
        .await
        .unwrap_or(configured_port);

    let addr = format!("{}:{}", host, port);
    let timeout = Duration::from_millis(timeout_ms);
    let started = Instant::now();

    let mut stream = match tokio::time::timeout(timeout, TcpStream::connect(&addr)).await {
        Ok(Ok(s)) => s,
        Ok(Err(e)) => {
            return SqlProbeResult {
                reachable: false,
                latency_ms: None,
                service_running: false,
                sql_version: None,
                resolved_port: port,
                detail: format!("Puerto TCP {} cerrado o rechazado ({})", port, e),
            };
        }
        Err(_) => {
            return SqlProbeResult {
                reachable: false,
                latency_ms: None,
                service_running: false,
                sql_version: None,
                resolved_port: port,
                detail: format!("Timeout al conectar a SQL Server en {}", addr),
            };
        }
    };

    if stream.write_all(&TDS_PRELOGIN_PACKET).await.is_err() {
        return SqlProbeResult {
            reachable: false,
            latency_ms: None,
            service_running: false,
            sql_version: None,
            resolved_port: port,
            detail: "Fallo al enviar handshake TDS Pre-Login".to_string(),
        };
    }

    let mut resp = [0u8; 256];
    let n = match tokio::time::timeout(timeout, stream.read(&mut resp)).await {
        Ok(Ok(bytes)) if bytes >= 8 => bytes,
        _ => {
            return SqlProbeResult {
                reachable: false,
                latency_ms: None,
                service_running: false,
                sql_version: None,
                resolved_port: port,
                detail: "Puerto abierto pero SQL Server no respondió al protocolo TDS".to_string(),
            };
        }
    };

    let rtt = started.elapsed().as_millis().max(1) as u64;
    let is_tds_response = resp[0] == 0x04;

    let mut service_running = is_tds_response;
    if check_win_service {
        if let Some(sc_state) = check_windows_service_sc(host, service_name) {
            service_running = sc_state && is_tds_response;
        }
    }

    let sql_version = if is_tds_response && n >= 35 {
        Some(format!("{}.{}.{}", resp[29], resp[30], u16::from_be_bytes([resp[31], resp[32]])))
    } else {
        None
    };

    SqlProbeResult {
        reachable: is_tds_response,
        latency_ms: Some(rtt),
        service_running,
        sql_version,
        resolved_port: port,
        detail: if is_tds_response {
            format!("SQL Server activo en puerto {} ({} ms)", port, rtt)
        } else {
            "Respuesta inesperada en puerto SQL".to_string()
        },
    }
}
