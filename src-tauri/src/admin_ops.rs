use crate::ping::icmp_ping;
use serde::Serialize;
use std::time::{Duration, Instant};
use tiberius::{AuthMethod, Client, Config, EncryptionLevel};
use tokio::net::TcpStream;
use tokio_util::compat::TokioAsyncWriteCompatExt;

#[derive(Debug, Clone, Serialize)]
pub struct OpResult {
    pub ok: bool,
    pub summary: String,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct DiagnosticPortCheck {
    pub name: String,
    pub target: String,
    pub open: bool,
    pub latency_ms: Option<u64>,
    pub note: String,
}

fn is_local_host(host: &str) -> bool {
    matches!(
        host.trim().to_ascii_lowercase().as_str(),
        "localhost" | "127.0.0.1" | "::1" | "."
    )
}

async fn check_tcp_port(host: &str, port: u16, name: &str, ok_note: &str, timeout_ms: u64) -> DiagnosticPortCheck {
    let started = Instant::now();
    let addr = format!("{}:{}", host.trim(), port);
    match tokio::time::timeout(Duration::from_millis(timeout_ms), TcpStream::connect((host.trim(), port))).await {
        Ok(Ok(_stream)) => DiagnosticPortCheck {
            name: name.to_string(),
            target: addr,
            open: true,
            latency_ms: Some(started.elapsed().as_millis().max(1) as u64),
            note: ok_note.to_string(),
        },
        Ok(Err(e)) => DiagnosticPortCheck {
            name: name.to_string(),
            target: addr,
            open: false,
            latency_ms: None,
            note: format!("Cerrado o rechazado ({})", e),
        },
        Err(_) => DiagnosticPortCheck {
            name: name.to_string(),
            target: addr,
            open: false,
            latency_ms: None,
            note: format!("Timeout ({} ms) — posible bloqueo por Firewall", timeout_ms),
        },
    }
}

pub async fn diagnose_ports(host: String, sql_port: u16, timeout_ms: u64) -> Vec<DiagnosticPortCheck> {
    let clean_host = host.trim().to_string();
    let (ping_res, sql_chk, winrm_chk, rpc_chk) = tokio::join!(
        icmp_ping(clean_host.clone(), timeout_ms),
        check_tcp_port(&clean_host, sql_port, "SQL Server TCP", "Puerto SQL abierto", timeout_ms),
        check_tcp_port(&clean_host, 5985, "WinRM (PowerShell Remoting)", "WinRM HTTP abierto", timeout_ms),
        check_tcp_port(&clean_host, 135, "WMI / RPC Endpoint", "Puerto RPC abierto", timeout_ms),
    );

    vec![
        DiagnosticPortCheck {
            name: "Ping ICMPv4".to_string(),
            target: clean_host,
            open: ping_res.reachable,
            latency_ms: ping_res.latency_ms,
            note: ping_res.detail,
        },
        sql_chk,
        winrm_chk,
        rpc_chk,
    ]
}

#[cfg(windows)]
fn run_powershell_with_cred(
    host: &str,
    domain: &str,
    username: &str,
    password: &str,
    script: &str,
) -> Result<String, String> {
    use std::os::windows::process::CommandExt;
    use std::process::Command;
    const CREATE_NO_WINDOW: u32 = 0x08000000;

    let full_user = if domain.trim().is_empty() || domain.trim() == "." {
        username.to_string()
    } else {
        format!("{}\\{}", domain.trim(), username)
    };
    let local_flag = if is_local_host(host) { "1" } else { "0" };

    let output = Command::new("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-Command", script])
        .env("VERSAT_WIN_HOST", host)
        .env("VERSAT_WIN_USER", &full_user)
        .env("VERSAT_WIN_PASS", password)
        .env("VERSAT_IS_LOCAL", local_flag)
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .map_err(|e| format!("No se pudo ejecutar PowerShell: {}", e))?;

    if !output.status.success() {
        let err = String::from_utf8_lossy(&output.stderr);
        return Err(err.trim().to_string());
    }

    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

#[cfg(not(windows))]
fn run_powershell_with_cred(
    _host: &str,
    _domain: &str,
    _username: &str,
    _password: &str,
    _script: &str,
) -> Result<String, String> {
    Err("Operación administrativa remota disponible únicamente en Windows".to_string())
}

pub async fn test_windows_credentials(
    host: String,
    domain: String,
    username: String,
    password: String,
) -> OpResult {
    if !is_local_host(&host) && (username.trim().is_empty() || password.is_empty()) {
        return OpResult {
            ok: false,
            summary: "Faltan credenciales de Windows".to_string(),
            detail: "Introduce el usuario y la contraseña de Windows Server antes de probar.".to_string(),
        };
    }

    let script = r#"
$ErrorActionPreference = 'Stop'
if ($env:VERSAT_IS_LOCAL -eq '1') {
  $cim = New-CimSession
} else {
  $sec = ConvertTo-SecureString $env:VERSAT_WIN_PASS -AsPlainText -Force
  $cred = New-Object System.Management.Automation.PSCredential($env:VERSAT_WIN_USER, $sec)
  $cim = New-CimSession -ComputerName $env:VERSAT_WIN_HOST -Credential $cred
}
try {
  $os = Get-CimInstance -CimSession $cim -ClassName Win32_OperatingSystem
  Write-Output "$($os.CSName) · $($os.Caption)"
} finally {
  Remove-CimSession $cim -ErrorAction SilentlyContinue
}
"#;

    let res = tokio::task::spawn_blocking(move || {
        run_powershell_with_cred(&host, &domain, &username, &password, script)
    })
    .await
    .map_err(|e| e.to_string())
    .and_then(|r| r);

    match res {
        Ok(info) => OpResult {
            ok: true,
            summary: "Credenciales de Windows válidas".to_string(),
            detail: format!("Autenticación CIM/WinRM correcta en {}", info),
        },
        Err(err) => OpResult {
            ok: false,
            summary: "Error de autenticación Windows".to_string(),
            detail: err,
        },
    }
}

pub async fn test_sql_credentials(
    host: String,
    port: u16,
    username: String,
    password: String,
) -> OpResult {
    if username.trim().is_empty() || password.is_empty() {
        return OpResult {
            ok: false,
            summary: "Faltan credenciales SQL".to_string(),
            detail: "Introduce usuario SQL (ej. sa) y contraseña antes de probar.".to_string(),
        };
    }

    let mut cfg = Config::new();
    cfg.host(&host);
    cfg.port(port);
    cfg.authentication(AuthMethod::sql_server(&username, &password));
    cfg.encryption(EncryptionLevel::NotSupported);
    cfg.trust_cert();

    let tcp = match TcpStream::connect((host.as_str(), port)).await {
        Ok(s) => s,
        Err(e) => {
            return OpResult {
                ok: false,
                summary: "Sin conexión TCP con SQL Server".to_string(),
                detail: format!("No se pudo conectar a {}:{} ({})", host, port, e),
            };
        }
    };
    tcp.set_nodelay(true).ok();

    let mut client = match Client::connect(cfg, tcp.compat_write()).await {
        Ok(c) => c,
        Err(e) => {
            return OpResult {
                ok: false,
                summary: "Credenciales SQL incorrectas".to_string(),
                detail: format!("Login rechazado para {}: {}", username, e),
            };
        }
    };

    let row = client
        .query(
            "SELECT CAST(@@SERVERNAME AS NVARCHAR(128)), CAST(SUSER_SNAME() AS NVARCHAR(128)), CAST(IS_SRVROLEMEMBER('sysadmin') AS INT)",
            &[],
        )
        .await;

    match row {
        Ok(stream) => {
            if let Ok(Some(r)) = stream.into_row().await {
                let srv = r.get::<&str, _>(0).unwrap_or(&host);
                let login = r.get::<&str, _>(1).unwrap_or(&username);
                let is_sa = r.get::<i32, _>(2).unwrap_or(0) == 1;
                return OpResult {
                    ok: true,
                    summary: "Credenciales SQL Server válidas".to_string(),
                    detail: format!(
                        "Conectado a {} como {} ({})",
                        srv,
                        login,
                        if is_sa { "rol sysadmin" } else { "permisos estándar" }
                    ),
                };
            }
            OpResult {
                ok: true,
                summary: "Login SQL correcto".to_string(),
                detail: format!("Autenticado en {}:{}", host, port),
            }
        }
        Err(e) => OpResult {
            ok: false,
            summary: "Error verificando sesión SQL".to_string(),
            detail: e.to_string(),
        },
    }
}

pub async fn configure_remote_firewall(
    host: String,
    domain: String,
    username: String,
    password: String,
    sql_port: u16,
) -> OpResult {
    let script = format!(
        r#"
$ErrorActionPreference = 'Stop'
$sb = {{
  param($port)
  if (-not (Get-NetFirewallRule -DisplayName 'Versat-Allow-ICMPv4-In' -ErrorAction SilentlyContinue)) {{
    New-NetFirewallRule -DisplayName 'Versat-Allow-ICMPv4-In' -Direction Inbound -Protocol ICMPv4 -IcmpType 8 -Action Allow | Out-Null
  }}
  if (-not (Get-NetFirewallRule -DisplayName "Versat-Allow-SQL-TCP-$port" -ErrorAction SilentlyContinue)) {{
    New-NetFirewallRule -DisplayName "Versat-Allow-SQL-TCP-$port" -Direction Inbound -Protocol TCP -LocalPort $port -Action Allow | Out-Null
  }}
  if (-not (Get-NetFirewallRule -DisplayName 'Versat-Allow-SQL-Browser-UDP-1434' -ErrorAction SilentlyContinue)) {{
    New-NetFirewallRule -DisplayName 'Versat-Allow-SQL-Browser-UDP-1434' -Direction Inbound -Protocol UDP -LocalPort 1434 -Action Allow | Out-Null
  }}
  Write-Output "Reglas aplicadas en $env:COMPUTERNAME (ICMPv4 + TCP $port + UDP 1434)"
}}
if ($env:VERSAT_IS_LOCAL -eq '1') {{
  & $sb {}
}} else {{
  $sec = ConvertTo-SecureString $env:VERSAT_WIN_PASS -AsPlainText -Force
  $cred = New-Object System.Management.Automation.PSCredential($env:VERSAT_WIN_USER, $sec)
  Invoke-Command -ComputerName $env:VERSAT_WIN_HOST -Credential $cred -ScriptBlock $sb -ArgumentList {}
}}
"#,
        sql_port, sql_port
    );

    let res = tokio::task::spawn_blocking(move || {
        run_powershell_with_cred(&host, &domain, &username, &password, &script)
    })
    .await
    .map_err(|e| e.to_string())
    .and_then(|r| r);

    match res {
        Ok(msg) => OpResult {
            ok: true,
            summary: "Excepciones de Firewall aplicadas".to_string(),
            detail: msg,
        },
        Err(err) => OpResult {
            ok: false,
            summary: "No se pudo configurar el Firewall remoto".to_string(),
            detail: err,
        },
    }
}

pub async fn control_sql_service(
    host: String,
    domain: String,
    username: String,
    password: String,
    service_name: String,
    action: String,
) -> OpResult {
    let ps_cmd = match action.as_str() {
        "start" => "Start-Service -Name $svc; (Get-Service -Name $svc).Status.ToString()",
        "stop" => "Stop-Service -Name $svc -Force; (Get-Service -Name $svc).Status.ToString()",
        "restart" => "Restart-Service -Name $svc -Force; (Get-Service -Name $svc).Status.ToString()",
        _ => {
            return OpResult {
                ok: false,
                summary: "Acción no válida".to_string(),
                detail: "Usa start, stop o restart".to_string(),
            }
        }
    };

    let safe_svc = service_name.replace("'", "");
    let script = format!(
        r#"
$ErrorActionPreference = 'Stop'
$sb = {{
  param($svc)
  {}
}}
if ($env:VERSAT_IS_LOCAL -eq '1') {{
  & $sb '{}'
}} else {{
  $sec = ConvertTo-SecureString $env:VERSAT_WIN_PASS -AsPlainText -Force
  $cred = New-Object System.Management.Automation.PSCredential($env:VERSAT_WIN_USER, $sec)
  Invoke-Command -ComputerName $env:VERSAT_WIN_HOST -Credential $cred -ScriptBlock $sb -ArgumentList '{}'
}}
"#,
        ps_cmd, safe_svc, safe_svc
    );

    let res = tokio::task::spawn_blocking(move || {
        run_powershell_with_cred(&host, &domain, &username, &password, &script)
    })
    .await
    .map_err(|e| e.to_string())
    .and_then(|r| r);

    match res {
        Ok(status) => OpResult {
            ok: true,
            summary: format!("Servicio {} actualizado", service_name),
            detail: format!("Acción '{}' ejecutada. Estado actual: {}", action, status),
        },
        Err(err) => OpResult {
            ok: false,
            summary: format!("Error controlando servicio {}", service_name),
            detail: err,
        },
    }
}

pub async fn restart_windows_server(
    host: String,
    domain: String,
    username: String,
    password: String,
) -> OpResult {
    let script = r#"
$ErrorActionPreference = 'Stop'
if ($env:VERSAT_IS_LOCAL -eq '1') {
  Restart-Computer -Force
  Write-Output "Orden de reinicio enviada al equipo local"
} else {
  $sec = ConvertTo-SecureString $env:VERSAT_WIN_PASS -AsPlainText -Force
  $cred = New-Object System.Management.Automation.PSCredential($env:VERSAT_WIN_USER, $sec)
  Restart-Computer -ComputerName $env:VERSAT_WIN_HOST -Credential $cred -Force
  Write-Output "Orden de reinicio enviada a $env:VERSAT_WIN_HOST"
}
"#;

    let res = tokio::task::spawn_blocking(move || {
        run_powershell_with_cred(&host, &domain, &username, &password, script)
    })
    .await
    .map_err(|e| e.to_string())
    .and_then(|r| r);

    match res {
        Ok(msg) => OpResult {
            ok: true,
            summary: "Reinicio de Windows Server iniciado".to_string(),
            detail: msg,
        },
        Err(err) => OpResult {
            ok: false,
            summary: "No se pudo reiniciar Windows Server".to_string(),
            detail: err,
        },
    }
}
