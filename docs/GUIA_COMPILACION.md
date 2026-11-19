# Guía de compilación e instalador (.exe)

## 1. Compilación local en Windows

### Requisitos previos
1. **Rust (MSVC)**: descarga e instala desde [rustup.rs](https://rustup.rs/) eligiendo el toolchain `x86_64-pc-windows-msvc`.
2. **Visual Studio Build Tools**: instala el componente **Desarrollo para el escritorio con C++** (incluye MSVC y Windows SDK).
3. *(Opcional)* **Tauri CLI v2**: el script lo instala si falta, o puedes instalarlo manualmente:
   ```powershell
   cargo install tauri-cli --version "^2.0"
   ```

### Generar el instalador
Abre PowerShell en la carpeta raíz del proyecto y ejecuta:

```powershell
.\build-installer.ps1
```

El script limpia iconos antiguos, genera `icon.ico` y PNGs desde el icono seleccionado, compila en modo *release* optimizado y crea:
`src-tauri\target\release\bundle\nsis\Windows-SQL Monitor_1.2.0_x64-setup.exe`

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
3. Crear un **Release en borrador** en GitHub (`Windows-SQL Monitor v1.2.0`) con el instalador `.exe` adjunto.

> Recuerda mantener sincronizado el número de versión del *tag* (por ejemplo `v1.2.0`) con `"version"` en `src-tauri/tauri.conf.json` y `src-tauri/Cargo.toml`.

---

## 3. Cómo incluir WebView2 dentro del `.exe` (Instalación 100% sin Internet)

Por defecto, Tauri usa `downloadBootstrapper` en Windows: el instalador pesa pocos MB y, si el equipo destino no tiene **Microsoft Edge WebView2 Runtime** (por ejemplo, Windows 10 antiguo o un Windows Server aislado sin internet), intenta descargarlo durante la instalación.

Si quieres instalar el `.exe` en PCs o servidores **sin acceso a Internet**, edita `src-tauri/tauri.conf.json` dentro de `bundle.windows`:

### Opción A (Recomendada): `offlineInstaller`
Incrusta el instalador offline oficial de Microsoft WebView2 dentro de tu `Setup.exe`.

```json
"windows": {
  "webviewInstallMode": {
    "type": "offlineInstaller"
  },
  "nsis": {
    "languages": ["Spanish"],
    "displayLanguageSelector": false,
    "installMode": "currentUser",
    "installerIcon": "icons/icon.ico"
  }
}
```

- **Ventaja:** instala WebView2 a nivel de sistema si hace falta, **sin usar Internet** en la PC destino.
- **Tamaño:** el `.exe` pasa a pesar alrededor de **130–140 MB** (al compilar, la máquina que compila descarga una sola vez `MicrosoftEdgeWebview2RuntimeInstallerX64.exe`).

### Opción B: `fixedRuntime` (100% portable, sin instalar WebView2 en el sistema)
Empaqueta una carpeta fija del runtime de WebView2 junto a tu aplicación sin modificar el sistema operativo.

1. Descarga desde Microsoft el paquete **Fixed Version** (x64) de WebView2 (`.cab`) y extráelo en una carpeta, por ejemplo `src-tauri/webview2-fixed`.
2. Configura `src-tauri/tauri.conf.json`:
   ```json
   "windows": {
     "webviewInstallMode": {
       "type": "fixedRuntime",
       "path": "./webview2-fixed"
     },
     "nsis": {
       "languages": ["Spanish"],
       "displayLanguageSelector": false,
       "installMode": "currentUser",
       "installerIcon": "icons/icon.ico"
     }
   }
   ```
- **Tamaño:** añade ~180 MB al instalador y ~300+ MB descomprimidos.
- **Uso ideal:** equipos muy bloqueados donde ni siquiera se permite ejecutar el instalador de WebView2.

### Comparativa rápida de modos

| Modo | ¿Requiere Internet al instalar? | Tamaño extra aproximado |
| :--- | :--- | :--- |
| `downloadBootstrapper` *(por defecto)* | Sí (solo si falta WebView2) | ~0 MB |
| `embedBootstrapper` | Sí (solo si falta WebView2) | ~1.8 MB |
| `offlineInstaller` | **No** | ~127 MB |
| `fixedRuntime` | **No** | ~180 MB |
