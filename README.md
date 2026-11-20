# Windows-SQL Monitor

**Versión 1.2.2 (2026)** · Aplicación de escritorio ligera en **Rust + Tauri v2** para monitorizar en tiempo real hasta **3 servidores Windows Server y SQL Server** desde la bandeja del sistema (*System Tray*).

---

## Características principales

- **Monitorización multi-servidor (hasta 3 instancias)**: cambia rápidamente entre servidores desde la barra lateral con estado individual en tiempo real.
- **Conectividad ICMP nativa Win32**: utiliza `IcmpSendEcho` directo desde Rust sin abrir ventanas de consola ni requerir permisos de administrador en el cliente.
- **Sondeo real SQL Server (TDS Pre-Login + SQL Browser UDP 1434)**: verifica disponibilidad del motor SQL y resuelve puertos dinámicos de instancias con nombre (por ejemplo `SQL2019`).
- **Métricas del Windows Server (CIM / WinRM)**: lectura de CPU %, memoria RAM usada/total, discos locales con porcentaje libre y tiempo activo.
- **Métricas internas de SQL Server (DMVs vía Tiberius)**: memoria del proceso SQL, *Buffer Cache Hit Ratio*, *Page Life Expectancy*, conexiones activas, sesiones bloqueadas y tamaño de bases de datos principales.
- **Control remoto y solución de problemas**:
  - Diagnóstico de puertos (ICMPv4, TCP SQL, WinRM 5985, WMI RPC 135).
  - Creación remota de reglas de Firewall (Ping ICMP, SQL TCP y SQL Browser UDP 1434).
  - Detección automática y estado en vivo de los 3 servicios (**Motor SQL**, **Agente SQL** y **SQL Server Browser**, incluyendo instancias como `MSSQL$SQL2019` y `SQLAgent$SQL2019`) con botones directos de **Iniciar**, **Reiniciar** y **Detener**.
  - Reinicio remoto controlado de Windows Server.
- **Seguridad local con Windows DPAPI**: todas las contraseñas se cifran con `CryptProtectData` vinculadas al usuario de Windows (nunca en texto plano).
- **Interfaz moderna adaptable**: tema **Sistema / Claro / Oscuro**, barra lateral ocultable, alertas Toast nativas e icono dinámico en bandeja con indicador de estado.

---

## Requisitos

### Para ejecutar el instalador `.exe` en otra PC Windows
- Windows 10 u 11 (x64).
- **Microsoft Edge WebView2 Runtime** (incluido por defecto en Windows 11 y Windows 10 actualizados; consulta [docs/GUIA_COMPILACION.md](docs/GUIA_COMPILACION.md) si necesitas empaquetarlo 100% offline dentro del `.exe`).
- *No requiere instalar Rust ni Visual Studio en el equipo cliente.*

### Para compilar desde el código fuente
- [Rust (rustup)](https://rustup.rs/) con toolchain `x86_64-pc-windows-msvc`.
- Microsoft Visual Studio Build Tools (carga de trabajo **Desarrollo para el escritorio con C++**).
- Tauri CLI v2 (`cargo install tauri-cli --version "^2.0"`).

---

## Compilación rápida en Windows

Ejecuta en PowerShell dentro de la raíz del proyecto:

```powershell
.\build-installer.ps1
```

Por defecto genera **automáticamente ambos instaladores** en la carpeta `release-bundles\` *(también puedes pasar `-Mode Standard` o `-Mode Offline` si solo deseas uno)*:

```text
release-bundles\Windows-SQL-Monitor_1.2.2_x64-setup.exe                 (~5-8 MB, estándar ligero)
release-bundles\Windows-SQL-Monitor_1.2.2_x64-offline-WebView2-setup.exe (~135 MB, WebView2 incluido sin Internet)
```

---

## Compilación automática y Releases en GitHub Actions

El repositorio incluye el workflow [`.github/workflows/build-windows-exe.yml`](.github/workflows/build-windows-exe.yml):

1. Sube una etiqueta (*tag*) con la versión:
   ```bash
   git tag v1.2.2
   git push origin v1.2.2
   ```
2. O ejecútalo manualmente desde la pestaña **Actions → Compilar Windows-SQL Monitor (.exe) → Run workflow**.
3. GitHub Actions compilará **ambos instaladores** en `windows-latest` y creará automáticamente un **Release en borrador (Draft)** con los dos `.exe` adjuntos (`estándar` y `offline con WebView2`), listo para revisar y publicar.

---

## Documentación adicional

- [Guía completa de compilación y WebView2 offline](docs/GUIA_COMPILACION.md)
- [Guía de preparación de servidores Windows y SQL Server](docs/GUIA_SERVIDORES.md)
- [Historial de cambios (CHANGELOG)](CHANGELOG.md)
- [Licencia MIT](LICENSE)

---

## Estructura del proyecto

```text
.
├── .github/workflows/
│   └── build-windows-exe.yml   # Compilación dual automática y Releases en GitHub
├── docs/
│   ├── GUIA_COMPILACION.md     # Compilación local, Actions y modo offline WebView2
│   └── GUIA_SERVIDORES.md      # WinRM, Firewall, instancias y permisos SQL
├── src-tauri/
│   ├── src/
│   │   ├── admin_ops.rs        # Pruebas, Firewall, servicios SQL y reinicio
│   │   ├── config.rs           # Configuración multi-servidor + DPAPI
│   │   ├── lib.rs              # Comandos Tauri y ventana principal
│   │   ├── monitor.rs          # Bucle asíncrono de sondeo y alertas Toast
│   │   ├── ping.rs             # Ping ICMP nativo Win32
│   │   ├── secrets.rs          # Cifrado Windows DPAPI
│   │   ├── sql_metrics.rs      # Consultas DMV con Tiberius
│   │   ├── sql_probe.rs        # Handshake TDS Pre-Login + SQL Browser + sc.exe
│   │   ├── tray.rs             # Bandeja del sistema con punto de estado
│   │   └── win_metrics.rs      # Métricas Windows Server (CPU/RAM/Discos)
│   ├── icons/                  # Icono fuente e iconos generados (.ico / .png)
│   ├── Cargo.toml
│   ├── tauri.conf.json         # Configuración base (instalador estándar ligero)
│   └── tauri.offline.conf.json # Overlay Tauri para instalador offline WebView2
├── ui/
│   ├── default-config.json     # Configuración inicial exportada
│   └── index.html              # Interfaz embebida (Claro / Oscuro / Sistema)
├── build-installer.ps1         # Script automático para compilar ambos instaladores .exe
├── CHANGELOG.md
├── LICENSE
└── README.md
```

---

## Autor y créditos

- **Aplicación:** Windows-SQL Monitor
- **Versión:** 1.2.2 (2026)
- **Autor:** William Jiménez Puentes
- **Correo:** willjp93@gmail.com
- **GitHub:** https://github.com/willljp93
