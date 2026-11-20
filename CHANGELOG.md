# Changelog

## [1.2.2] - 2026

### Añadido
- Soporte multi-servidor para monitorizar hasta **3 servidores Windows / SQL Server** en paralelo.
- Panel de control directo para los **3 servicios SQL Server** (`Motor SQL`, `Agente SQL` y `SQLBrowser`) con detección automática de instancias con nombre (como `MSSQL$SQL2019` y `SQLAgent$SQL2019`), estado en vivo y botones **Iniciar**, **Reiniciar** y **Detener** por servicio.
- Modo **Claro / Oscuro / Sistema** con botón en la esquina superior derecha de la barra lateral.
- Barra lateral ocultable con área de servidores responsiva y créditos integrados en el pie de la barra lateral.
- Workflow de GitHub Actions por etiquetas (`v*`) con creación automática de Releases en borrador e inclusión de **ambos instaladores** (estándar ligero y offline con WebView2).
- Overlay `src-tauri/tauri.offline.conf.json` y parámetro `-Mode All|Standard|Offline` en `build-installer.ps1`.

### Corregido
- Paso seguro de nombres de servicio con `$` mediante variables de entorno en PowerShell (`VERSAT_SVC_NAME`), evitando fallos con instancias con nombre.
- Aplicación correcta del icono elegido al ejecutable `.exe`, barra de tareas e icono de bandeja del sistema.

## [1.0.0] - 2026

### Inicial
- Agente Rust + Tauri v2 con Ping ICMP Win32, sonda SQL TDS Pre-Login, métricas CIM/WinRM y DMVs con Tiberius.
- Cifrado de credenciales con Windows DPAPI, diagnóstico de puertos, reglas remotas de Firewall y reinicio de Windows Server.
