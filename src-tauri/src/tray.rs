use crate::monitor::{execute_probe_cycle, SharedMonitorState};
use std::sync::{Arc, Mutex};
use tauri::image::Image;
use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager};

const TRAY_BASE_PNG: &[u8] = include_bytes!("../icons/32x32.png");

#[derive(Debug, Clone, Copy)]
pub enum TrayVisualState {
    Nominal,
    Warning,
    Critical,
    Paused,
}

pub fn render_tray_icon(state: TrayVisualState) -> Image<'static> {
    let (r, g, b) = match state {
        TrayVisualState::Nominal => (22u8, 163u8, 74u8),
        TrayVisualState::Warning => (245u8, 158u8, 11u8),
        TrayVisualState::Critical => (239u8, 68u8, 68u8),
        TrayVisualState::Paused => (100u8, 116u8, 139u8),
    };

    if let Ok(base) = Image::from_bytes(TRAY_BASE_PNG) {
        let w = base.width() as usize;
        let h = base.height() as usize;
        if w > 0 && h > 0 {
            let mut rgba = base.rgba().to_vec();
            let cx = (w as f32) * 0.76;
            let cy = (h as f32) * 0.76;
            let outer_r = (w as f32) * 0.24;
            let inner_r = (w as f32) * 0.16;
            for y in 0..h {
                for x in 0..w {
                    let dx = x as f32 - cx;
                    let dy = y as f32 - cy;
                    let dist = (dx * dx + dy * dy).sqrt();
                    if dist <= outer_r {
                        let idx = (y * w + x) * 4;
                        if dist <= inner_r {
                            rgba[idx] = r;
                            rgba[idx + 1] = g;
                            rgba[idx + 2] = b;
                            rgba[idx + 3] = 255;
                        } else {
                            rgba[idx] = 15;
                            rgba[idx + 1] = 23;
                            rgba[idx + 2] = 42;
                            rgba[idx + 3] = 255;
                        }
                    }
                }
            }
            return Image::new_owned(rgba, w as u32, h as u32);
        }
    }

    let size = 32usize;
    let mut rgba = vec![0u8; size * size * 4];
    for y in 0..size {
        for x in 0..size {
            let idx = (y * size + x) * 4;
            let dx = x as f32 - 15.5;
            let dy = y as f32 - 15.5;
            let dist = (dx * dx + dy * dy).sqrt();
            if dist <= 13.5 {
                let is_ring = dist >= 10.5;
                if is_ring {
                    rgba[idx] = 255;
                    rgba[idx + 1] = 255;
                    rgba[idx + 2] = 255;
                    rgba[idx + 3] = 235;
                } else {
                    rgba[idx] = r;
                    rgba[idx + 1] = g;
                    rgba[idx + 2] = b;
                    rgba[idx + 3] = 255;
                }
            }
        }
    }
    Image::new_owned(rgba, size as u32, size as u32)
}

pub fn update_tray_visual(app: &AppHandle, state: TrayVisualState, tooltip: &str) {
    if let Some(tray) = app.tray_by_id("windows-sql-monitor-tray") {
        let _ = tray.set_icon(Some(render_tray_icon(state)));
        let _ = tray.set_tooltip(Some(tooltip));
    }
}

pub fn setup_tray(app: &AppHandle, state: Arc<Mutex<SharedMonitorState>>) -> tauri::Result<()> {
    let open_item = MenuItem::with_id(app, "open", "Abrir Windows-SQL Monitor", true, None::<&str>)?;
    let probe_item = MenuItem::with_id(app, "probe_now", "Ejecutar sondeo completo ahora", true, None::<&str>)?;
    let pause_item = MenuItem::with_id(app, "toggle_pause", "Pausar / Reanudar", true, None::<&str>)?;
    let quit_item = MenuItem::with_id(app, "quit", "Salir", true, None::<&str>)?;

    let menu = Menu::with_items(app, &[&open_item, &probe_item, &pause_item, &quit_item])?;

    let state_for_menu = state.clone();
    TrayIconBuilder::with_id("windows-sql-monitor-tray")
        .icon(render_tray_icon(TrayVisualState::Nominal))
        .tooltip("Windows-SQL Monitor v1.2 · Iniciando...")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(move |app, event| match event.id.as_ref() {
            "open" => {
                if let Some(win) = app.get_webview_window("main") {
                    let _ = win.show();
                    let _ = win.set_focus();
                }
            }
            "probe_now" => {
                let app_handle = app.clone();
                let st = state_for_menu.clone();
                tauri::async_runtime::spawn(async move {
                    let _ = execute_probe_cycle(&app_handle, &st, true, true).await;
                });
            }
            "toggle_pause" => {
                let mut guard = state_for_menu.lock().unwrap();
                guard.paused = !guard.paused;
            }
            "quit" => {
                app.exit(0);
            }
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                let app = tray.app_handle();
                if let Some(win) = app.get_webview_window("main") {
                    let _ = win.show();
                    let _ = win.set_focus();
                }
            }
        })
        .build(app)?;

    Ok(())
}
