# Windows-SQL Monitor v1.2 (Rust + Tauri v2)

Aplicación de escritorio ligera para la bandeja del sistema de Windows (área de notificación junto al reloj).
Monitoriza de forma continua hasta 3 servidores en paralelo:
1. Conectividad ICMP (Ping nativo Win32 `IcmpSendEcho`, sin ventanas emergentes ni permisos de administrador).
2. Disponibilidad real del motor SQL Server mediante handshake TCP + paquete **TDS Pre-Login**.
3. Recursos del **Windows Server** (CPU %, RAM usada/total, discos libres, uptime) mediante sesión CIM/WinRM autenticada con usuario y contraseña de Windows.
4. Recursos internos de **SQL Server** (memoria del proceso SQL, Buffer Cache Hit Ratio, Page Life Expectancy, sesiones activas y bloqueos, tamaño de bases de datos) mediante consultas a DMVs con usuario `sa` o login de monitorización.
5. Cifrado local de credenciales en Windows usando **DPAPI (`CryptProtectData`)** para no almacenar contraseñas en texto plano.
6. Alertas nativas de Windows (Toast Notifications) ante latencia elevada, cortes de red, CPU/RAM/disco al límite, bloqueos o caída de SQL Server.

## Requisitos en Windows para compilar el .exe

1. Instalar [Rust (rustup)](https://rustup.rs/) con el toolchain `x86_64-pc-windows-msvc`.
2. Tener instalado Visual Studio Build Tools (C++ desktop workload).
3. Ejecutar en PowerShell dentro de esta carpeta:

```powershell
.\build-installer.ps1
```

El instalador final se generará en:
`src-tauri\target\release\bundle\nsis\Windows-SQL Monitor_1.2.0_x64-setup.exe`

> Nota sobre seguridad en SQL Server: aunque puedes usar `sa`, en producción también puedes crear un login dedicado con permiso mínimo `VIEW SERVER STATE`. En Windows Server, habilita WinRM (`winrm quickconfig`) para recolectar CPU, RAM y discos remotos.

## Autor y créditos

- **Versión:** 1.2 (2026)
- **Autor:** William Jiménez Puentes
- **Correo:** willjp93@gmail.com
- **GitHub:** https://github.com/willljp93
