use crate::monitor::{execute_probe_cycle, SharedMonitorState};
use std::sync::{Arc, Mutex};
use tauri::image::Image;
use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager};

const TRAY_BASE_PNG: &[u8] = include_bytes!("../icons/32x32.png");

#[derive(Debug, Clone, Copy)]
pub enum TrayVisualState {
    Nominal,
    Warning,
    Critical,
    Paused,
}

#[cfg(windows)]
fn compute_tray_popup_pos(click_x: i32, click_y: i32, popup_w: i32, popup_h: i32) -> (i32, i32) {
    use windows_sys::Win32::Foundation::RECT;
    use windows_sys::Win32::UI::WindowsAndMessaging::{SystemParametersInfoW, SPI_GETWORKAREA};

    let mut work = RECT {
        left: 0,
        top: 0,
        right: 1920,
        bottom: 1040,
    };
    unsafe {
        let _ = SystemParametersInfoW(
            SPI_GETWORKAREA,
            0,
            &mut work as *mut RECT as *mut std::ffi::c_void,
            0,
        );
    }

    let min_x = work.left + 8;
    let max_x = (work.right - popup_w - 8).max(min_x);
    let min_y = work.top + 8;
    let max_y = (work.bottom - popup_h - 8).max(min_y);

    let px = (click_x - popup_w / 2).clamp(min_x, max_x);
    let mid_y = work.top + (work.bottom - work.top) / 2;
    let raw_y = if click_y >= mid_y {
        (work.bottom - popup_h - 10).min(click_y - popup_h - 10)
    } else {
        (work.top + 10).max(click_y + 10)
    };
    let py = raw_y.clamp(min_y, max_y);
    (px, py)
}

#[cfg(not(windows))]
fn compute_tray_popup_pos(click_x: i32, click_y: i32, popup_w: i32, popup_h: i32) -> (i32, i32) {
    ((click_x - popup_w / 2).max(8), (click_y - popup_h - 10).max(8))
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
    let state_for_click = state;
    TrayIconBuilder::with_id("windows-sql-monitor-tray")
        .icon(render_tray_icon(TrayVisualState::Nominal))
        .tooltip(concat!("Windows-SQL Monitor v", env!("CARGO_PKG_VERSION"), " · Iniciando..."))
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(move |app, event| match event.id.as_ref() {
            "open" => {
                if let Some(tray_win) = app.get_webview_window("tray") {
                    let _ = tray_win.hide();
                }
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
                {
                    let mut guard = state_for_menu.lock().unwrap();
                    guard.paused = !guard.paused;
                }
                let _ = app.emit("probe-updated", ());
            }
            "quit" => {
                app.exit(0);
            }
            _ => {}
        })
        .on_tray_icon_event(move |tray, event| match event {
            TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                position,
                ..
            } => {
                let app = tray.app_handle();
                if let Some(tray_win) = app.get_webview_window("tray") {
                    if tray_win.is_visible().unwrap_or(false) {
                        let _ = tray_win.hide();
                    } else {
                        let srv_count = state_for_click
                            .lock()
                            .unwrap()
                            .config
                            .effective_servers()
                            .len()
                            .clamp(1, 3);
                        let logical_h = 204.0 + (srv_count as f64) * 46.0;
                        let _ = tray_win.set_size(tauri::LogicalSize::new(310.0, logical_h));
                        let scale = tray_win.scale_factor().unwrap_or(1.0).max(1.0);
                        let pw = (310.0 * scale) as i32;
                        let ph = (logical_h * scale) as i32;
                        let (px, py) = compute_tray_popup_pos(position.x as i32, position.y as i32, pw, ph);
                        let _ = tray_win.set_position(tauri::Position::Physical(
                            tauri::PhysicalPosition::new(px, py),
                        ));
                        let _ = app.emit("tray-refresh", ());
                        let _ = tray_win.show();
                        let _ = tray_win.set_focus();
                    }
                }
            }
            TrayIconEvent::DoubleClick {
                button: MouseButton::Left,
                ..
            } => {
                let app = tray.app_handle();
                if let Some(tray_win) = app.get_webview_window("tray") {
                    let _ = tray_win.hide();
                }
                if let Some(win) = app.get_webview_window("main") {
                    let _ = win.show();
                    let _ = win.set_focus();
                }
            }
            _ => {}
        })
        .build(app)?;

    Ok(())
}
