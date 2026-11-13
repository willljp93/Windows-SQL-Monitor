use std::net::{IpAddr, ToSocketAddrs};
use std::time::Instant;

#[derive(Debug, Clone)]
pub struct PingResult {
    pub reachable: bool,
    pub latency_ms: Option<u64>,
    pub detail: String,
}

fn resolve_ipv4(host: &str) -> Result<[u8; 4], String> {
    if let Ok(IpAddr::V4(v4)) = host.parse::<IpAddr>() {
        return Ok(v4.octets());
    }
    let addrs = (host, 0)
        .to_socket_addrs()
        .map_err(|e| format!("DNS error: {}", e))?;
    for addr in addrs {
        if let IpAddr::V4(v4) = addr.ip() {
            return Ok(v4.octets());
        }
    }
    Err("No se resolvió dirección IPv4 para el servidor".to_string())
}

#[cfg(windows)]
pub async fn icmp_ping(host: String, timeout_ms: u64) -> PingResult {
    tokio::task::spawn_blocking(move || {
        use windows_sys::Win32::Foundation::INVALID_HANDLE_VALUE;
        use windows_sys::Win32::NetworkManagement::IpHelper::{
            IcmpCloseHandle, IcmpCreateFile, IcmpSendEcho, ICMP_ECHO_REPLY,
        };

        let octets = match resolve_ipv4(&host) {
            Ok(o) => o,
            Err(err) => {
                return PingResult {
                    reachable: false,
                    latency_ms: None,
                    detail: err,
                }
            }
        };

        let dest_addr: u32 = u32::from_ne_bytes(octets);
        let payload = b"VersatICMP";
        let reply_size = std::mem::size_of::<ICMP_ECHO_REPLY>() + payload.len() + 16;
        let mut reply_buf = vec![0u8; reply_size];

        unsafe {
            let handle = IcmpCreateFile();
            if handle == INVALID_HANDLE_VALUE || handle.is_null() {
                return PingResult {
                    reachable: false,
                    latency_ms: None,
                    detail: "No se pudo abrir handle ICMP de Windows".to_string(),
                };
            }

            let started = Instant::now();
            let count = IcmpSendEcho(
                handle,
                dest_addr,
                payload.as_ptr() as *const _,
                payload.len() as u16,
                std::ptr::null(),
                reply_buf.as_mut_ptr() as *mut _,
                reply_size as u32,
                timeout_ms as u32,
            );
            let elapsed = started.elapsed().as_millis() as u64;
            let _ = IcmpCloseHandle(handle);

            if count > 0 {
                let reply = &*(reply_buf.as_ptr() as *const ICMP_ECHO_REPLY);
                if reply.Status == 0 {
                    let rtt = if reply.RoundTripTime > 0 {
                        reply.RoundTripTime as u64
                    } else {
                        elapsed.max(1)
                    };
                    return PingResult {
                        reachable: true,
                        latency_ms: Some(rtt),
                        detail: format!("ICMP OK ({} ms)", rtt),
                    };
                }
            }

            PingResult {
                reachable: false,
                latency_ms: None,
                detail: format!("Tiempo de espera agotado ({} ms)", timeout_ms),
            }
        }
    })
    .await
    .unwrap_or(PingResult {
        reachable: false,
        latency_ms: None,
        detail: "Error interno en hilo ICMP".to_string(),
    })
}

#[cfg(not(windows))]
pub async fn icmp_ping(host: String, timeout_ms: u64) -> PingResult {
    let started = Instant::now();
    let timeout = std::time::Duration::from_millis(timeout_ms);
    match tokio::time::timeout(timeout, tokio::net::TcpStream::connect((host.as_str(), 1433))).await {
        Ok(Ok(_)) => PingResult {
            reachable: true,
            latency_ms: Some(started.elapsed().as_millis().max(1) as u64),
            detail: "TCP Fallback OK".to_string(),
        },
        _ => PingResult {
            reachable: false,
            latency_ms: None,
            detail: "Host inalcanzable".to_string(),
        },
    }
}
