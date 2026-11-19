# Guía de configuración de servidores Windows y SQL Server

## 1. Preparar Windows Server (para CPU, RAM, Discos, Firewall y Servicios)

Si monitorizas un servidor remoto en tu red, ejecuta una vez en PowerShell **como Administrador** dentro del propio Windows Server:

```powershell
# Habilitar PowerShell Remoting (WinRM en puerto 5985)
Enable-PSRemoting -Force

# Reglas de Firewall recomendadas
New-NetFirewallRule -DisplayName 'WindowsSQL-Allow-ICMPv4-In' -Direction Inbound -Protocol ICMPv4 -IcmpType 8 -Action Allow -ErrorAction SilentlyContinue
New-NetFirewallRule -DisplayName 'WindowsSQL-Allow-SQL-TCP-1433' -Direction Inbound -Protocol TCP -LocalPort 1433 -Action Allow -ErrorAction SilentlyContinue
New-NetFirewallRule -DisplayName 'WindowsSQL-Allow-SQL-Browser-UDP-1434' -Direction Inbound -Protocol UDP -LocalPort 1434 -Action Allow -ErrorAction SilentlyContinue
New-NetFirewallRule -DisplayName 'WindowsSQL-Allow-WinRM-5985' -Direction Inbound -Protocol TCP -LocalPort 5985 -Action Allow -ErrorAction SilentlyContinue
```

> Si ambos equipos están en un **Grupo de Trabajo** (sin Active Directory), recuerda agregar la IP del servidor remoto a `TrustedHosts` en la PC donde ejecutas el monitor:
> `Set-Item WSMan:\localhost\Client\TrustedHosts -Value "10.12.9.4" -Concatenate -Force`

---

## 2. Instancias con nombre (ej. `SQL2019`)

En SQL Server existen dos formatos habituales de servicios en Windows:

| Tipo de instancia | Motor SQL | Agente SQL | Browser |
| :--- | :--- | :--- | :--- |
| Predeterminada | `MSSQLSERVER` | `SQLSERVERAGENT` | `SQLBrowser` |
| Con nombre (ej. `SQL2019`) | `MSSQL$SQL2019` | `SQLAgent$SQL2019` | `SQLBrowser` |

- En **Tiempos y configuración**, indica el puerto TCP real de tu instancia (o deja `Instancia SQL` con `SQL2019` para resolverlo vía **SQL Browser UDP 1434**).
- En **Control y Firewall**, el agente detecta automáticamente los 3 servicios instalados en el Windows Server y muestra el estado y botones independientes para cada uno.

---

## 3. Permisos mínimos recomendados en SQL Server

Aunque puedes utilizar `sa`, si prefieres un usuario de solo monitorización con privilegios mínimos:

```sql
CREATE LOGIN [sql_monitor] WITH PASSWORD = 'TuPasswordSegura123!';
GRANT VIEW SERVER STATE TO [sql_monitor];
GRANT VIEW ANY DEFINITION TO [sql_monitor];
```

---

## 4. Seguridad de credenciales en el cliente

Las contraseñas de Windows Server y SQL Server **nunca se exportan ni se guardan en texto plano**. En el equipo donde corre **Windows-SQL Monitor**, se cifran localmente con **Windows DPAPI (`CryptProtectData`)** en `%APPDATA%\Windows-SQL Monitor\config.json`.
