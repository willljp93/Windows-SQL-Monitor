# Guía de compilación e instaladores (.exe)

## 1. Compilación local en Windows (ambos instaladores)

### Requisitos previos
1. **Rust (MSVC)**: descarga e instala desde [rustup.rs](https://rustup.rs/) eligiendo el toolchain `x86_64-pc-windows-msvc`.
2. **Visual Studio Build Tools**: instala el componente **Desarrollo para el escritorio con C++** (incluye MSVC y Windows SDK).
3. *(Opcional)* **Tauri CLI v2**: el script lo instala si falta, o puedes instalarlo manualmente:
   ```powershell
   cargo install tauri-cli --version "^2.0"
   ```

### Generar los instaladores
Abre PowerShell en la carpeta raíz del proyecto y ejecuta:

```powershell
.\build-installer.ps1
```

El script limpia iconos antiguos, genera `icon.ico` y PNGs desde el icono seleccionado, compila en modo *release* optimizado y genera **ambos instaladores** en la carpeta `release-bundles\`:

- `release-bundles\Windows-SQL-Monitor_1.2.0_x64-setup.exe` *(estándar ligero, ~5–8 MB)*
- `release-bundles\Windows-SQL-Monitor_1.2.0_x64-offline-WebView2-setup.exe` *(100% offline con WebView2 incluido, ~135 MB)*

Si en algún momento quieres generar solo una variante localmente, puedes usar el parámetro `-Mode`:

```powershell
.\build-installer.ps1 -Mode Standard
.\build-installer.ps1 -Mode Offline
```

---

## 2. Compilación en la nube con GitHub Actions

El archivo [`.github/workflows/build-windows-exe.yml`](../.github/workflows/build-windows-exe.yml) está configurado para:

1. Compilar automáticamente al subir un *tag* `v*`:
   ```bash
   git add .
   git commit -m "Release v1.2.0"
   git tag v1.2.0
   git push origin main
   git push origin v1.2.0
   ```
2. Permitir ejecución manual desde **Actions → Compilar Windows-SQL Monitor (.exe) → Run workflow**.
3. Compilar las dos variantes (`estándar` y `offline con WebView2` usando `src-tauri/tauri.offline.conf.json`) y crear un **Release en borrador** en GitHub (`Windows-SQL Monitor v1.2.0`) con **ambos archivos `.exe` adjuntos**.

> Recuerda mantener sincronizado el número de versión del *tag* (por ejemplo `v1.2.0`) con `"version"` en `src-tauri/tauri.conf.json` y `src-tauri/Cargo.toml`.

---

## 3. Cómo funciona el instalador con WebView2 incluido (`tauri.offline.conf.json`)

El proyecto ya incluye el archivo adicional [`src-tauri/tauri.offline.conf.json`](../src-tauri/tauri.offline.conf.json):

```json
{
  "$schema": "https://schema.tauri.app/config/2",
  "bundle": {
    "windows": {
      "webviewInstallMode": {
        "type": "offlineInstaller"
      }
    }
  }
}
```

Al ejecutar `cargo tauri build --bundles nsis --config src-tauri/tauri.offline.conf.json`, Tauri combina esa configuración con `src-tauri/tauri.conf.json`, reutiliza el binario Rust ya compilado y descarga una sola vez `MicrosoftEdgeWebview2RuntimeInstallerX64.exe` en la máquina que compila para incrustarlo dentro del `.exe`.

### Alternativa `fixedRuntime` (100% portable, sin instalar WebView2 en el sistema)
Si en un equipo muy restringido tampoco se permite ejecutar el instalador de WebView2:

1. Descarga desde Microsoft el paquete **Fixed Version** (x64) de WebView2 (`.cab`) y extráelo en `src-tauri/webview2-fixed`.
2. Sustituye `offlineInstaller` por:
   ```json
   "webviewInstallMode": {
     "type": "fixedRuntime",
     "path": "./webview2-fixed"
   }
   ```

### Comparativa rápida de modos

| Modo | ¿Requiere Internet al instalar? | Tamaño aproximado del `.exe` |
| :--- | :--- | :--- |
| `downloadBootstrapper` *(Estándar)* | Sí (solo si falta WebView2) | ~5–8 MB |
| `offlineInstaller` *(Offline WebView2)* | **No** | ~135 MB |
| `fixedRuntime` *(Portable manual)* | **No** | ~185+ MB |
