use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiskMetric {
    pub drive: String,
    pub label: String,
    pub used_gb: f64,
    pub total_gb: f64,
    pub free_percent: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WindowsResourceMetrics {
    pub os_name: String,
    pub cpu_usage_percent: f64,
    pub ram_used_gb: f64,
    pub ram_total_gb: f64,
    pub ram_usage_percent: f64,
    pub uptime_hours: f64,
    pub net_in_kbps: f64,
    pub net_out_kbps: f64,
    pub disks: Vec<DiskMetric>,
}

#[cfg(windows)]
pub async fn collect_windows_resources(
    host: String,
    domain: String,
    username: String,
    password: String,
) -> Result<WindowsResourceMetrics, String> {
    tokio::task::spawn_blocking(move || {
        use std::os::windows::process::CommandExt;
        use std::process::Command;
        const CREATE_NO_WINDOW: u32 = 0x08000000;

        let is_local = matches!(
            host.trim().to_ascii_lowercase().as_str(),
            "localhost" | "127.0.0.1" | "::1" | "."
        );

        let full_user = if domain.trim().is_empty() || domain.trim() == "." {
            username
        } else {
            format!("{}\\{}", domain.trim(), username)
        };

        // Pasamos credenciales vía variables de entorno del subproceso para no exponerlas en la línea de comandos
        let script = r#"
$ErrorActionPreference = 'Stop'
if ($env:VERSAT_IS_LOCAL -eq '1') {
  $cim = New-CimSession
} else {
  $user = $env:VERSAT_WIN_USER
  $pass = $env:VERSAT_WIN_PASS
  $target = $env:VERSAT_WIN_HOST
  $sec = ConvertTo-SecureString $pass -AsPlainText -Force
  $cred = New-Object System.Management.Automation.PSCredential($user, $sec)
  $cim = $null
  $wsErr = $null
  try {
    $cim = New-CimSession -ComputerName $target -Credential $cred -Authentication Negotiate
  } catch {
    $wsErr = $_.Exception.Message
    try {
      $isAdmin = ([Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
      if ($isAdmin) {
        Set-Item WSMan:\localhost\Client\TrustedHosts -Value $target -Concatenate -Force -ErrorAction Stop
        $cim = New-CimSession -ComputerName $target -Credential $cred -Authentication Negotiate
      }
    } catch {}
    if (-not $cim) {
      try {
        $dcomOpt = New-CimSessionOption -Protocol Dcom
        $cim = New-CimSession -ComputerName $target -Credential $cred -SessionOption $dcomOpt
      } catch {
        throw "Error WinRM/CIM ($wsErr) y DCOM ($($_.Exception.Message)). Si estás fuera de dominio ejecuta como Administrador en esta PC: Set-Item WSMan:\localhost\Client\TrustedHosts -Value '$target' -Concatenate -Force"
      }
    }
  }
}
try {
  $os = Get-CimInstance -CimSession $cim -ClassName Win32_OperatingSystem
  $cpu = Get-CimInstance -CimSession $cim -ClassName Win32_Processor | Measure-Object -Property LoadPercentage -Average
  $disks = Get-CimInstance -CimSession $cim -ClassName Win32_LogicalDisk -Filter "DriveType=3" | ForEach-Object {
    $tot = [math]::Round($_.Size / 1GB, 1)
    $free = [math]::Round($_.FreeSpace / 1GB, 1)
    $used = [math]::Round($tot - $free, 1)
    $pct = if ($tot -gt 0) { [math]::Round(($free / $tot) * 100, 1) } else { 0 }
    @{ drive = $_.DeviceID; label = [string]$_.VolumeName; used_gb = $used; total_gb = $tot; free_percent = $pct }
  }
  $ramTotal = [math]::Round($os.TotalVisibleMemorySize / 1MB, 2)
  $ramFree = [math]::Round($os.FreePhysicalMemory / 1MB, 2)
  $ramUsed = [math]::Round($ramTotal - $ramFree, 2)
  $ramPct = if ($ramTotal -gt 0) { [math]::Round(($ramUsed / $ramTotal) * 100, 1) } else { 0 }
  $uptime = [math]::Round(((Get-Date) - $os.LastBootUpTime).TotalHours, 1)
  @{
    os_name = $os.Caption
    cpu_usage_percent = [math]::Round($cpu.Average, 1)
    ram_used_gb = $ramUsed
    ram_total_gb = $ramTotal
    ram_usage_percent = $ramPct
    uptime_hours = $uptime
    net_in_kbps = 0.0
    net_out_kbps = 0.0
    disks = @($disks)
  } | ConvertTo-Json -Depth 4 -Compress
} finally {
  Remove-CimSession $cim -ErrorAction SilentlyContinue
}
"#;

        let output = Command::new("powershell.exe")
            .args(["-NoProfile", "-NonInteractive", "-Command", script])
            .env("VERSAT_WIN_HOST", &host)
            .env("VERSAT_WIN_USER", &full_user)
            .env("VERSAT_WIN_PASS", &password)
            .env("VERSAT_IS_LOCAL", if is_local { "1" } else { "0" })
            .creation_flags(CREATE_NO_WINDOW)
            .output()
            .map_err(|e| format!("No se pudo lanzar PowerShell CIM: {}", e))?;

        if !output.status.success() {
            let err = String::from_utf8_lossy(&output.stderr);
            return Err(format!("WinRM/CIM error: {}", err.trim()));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        serde_json::from_str::<WindowsResourceMetrics>(stdout.trim())
            .map_err(|e| format!("Error parseando métricas de Windows: {}", e))
    })
    .await
    .map_err(|e| e.to_string())?
}

#[cfg(not(windows))]
pub async fn collect_windows_resources(
    _host: String,
    _domain: String,
    _username: String,
    _password: String,
) -> Result<WindowsResourceMetrics, String> {
    Err("Recolección WMI/WinRM nativa disponible únicamente en Windows".to_string())
}
