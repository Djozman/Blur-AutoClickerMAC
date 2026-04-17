//! macOS overlay backend — replaces all Win32 calls with Tauri's cross-platform window API.
//! Window level is forced to NSScreenSaverWindowLevel (2000) via objc so the overlay
//! is never in the normal window stack and never flashes on launch.

use crate::app_state::ClickerState;
use crate::engine::mouse::{current_monitor_rects, current_virtual_screen_rect};
use std::sync::atomic::Ordering;
use std::sync::Mutex;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager};
use raw_window_handle::HasWindowHandle;

static LAST_ZONE_SHOW: Mutex<Option<Instant>> = Mutex::new(None);
pub static OVERLAY_THREAD_RUNNING: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(true);

/// Force the overlay NSWindow to NSScreenSaverWindowLevel (2000) and
/// configure it so it never becomes the key window or appears in Exposé.
#[cfg(target_os = "macos")]
unsafe fn set_overlay_window_level(ns_view: *mut objc::runtime::Object) {
    use objc::{msg_send, sel, sel_impl};
    // Get NSWindow from NSView.
    let ns_window: *mut objc::runtime::Object = msg_send![ns_view, window];
    if ns_window.is_null() {
        log::warn!("[Overlay] ns_window is null, skipping level config");
        return;
    }
    // NSScreenSaverWindowLevel = 2000
    let _: () = msg_send![ns_window, setLevel: 2000i64];
    // Never become key or main window.
    let _: () = msg_send![ns_window, setCanBecomeKeyWindow: false];
    let _: () = msg_send![ns_window, setCanBecomeMainWindow: false];
    // NSWindowCollectionBehaviorStationary (1 << 7) — hidden from Mission Control.
    let _: () = msg_send![ns_window, setCollectionBehavior: 128u64];
    // Ignore mouse events at NSWindow level.
    let _: () = msg_send![ns_window, setIgnoresMouseEvents: true];
}

pub fn init_overlay(app: &AppHandle) -> Result<(), String> {
    let window = app
        .get_webview_window("overlay")
        .ok_or_else(|| "Overlay window not found".to_string())?;

    log::info!("[Overlay] Running one-time init...");

    // Hide immediately — prevents WebView flash before config is applied.
    let _ = window.hide();

    window.set_ignore_cursor_events(true).map_err(|e| e.to_string())?;
    let _ = window.set_decorations(false);
    let _ = window.set_always_on_top(true);

    if let Some(b) = current_virtual_screen_rect() {
        let _ = window.set_position(tauri::PhysicalPosition::new(b.left, b.top));
        let _ = window.set_size(tauri::PhysicalSize::new(b.width as u32, b.height as u32));
    }

    // Apply macOS-specific window level via objc.
    #[cfg(target_os = "macos")]
    {
        use raw_window_handle::RawWindowHandle;
        if let Ok(handle) = window.window_handle() {
            if let RawWindowHandle::AppKit(appkit) = handle.as_raw() {
                let ns_view = appkit.ns_view.as_ptr() as *mut objc::runtime::Object;
                unsafe { set_overlay_window_level(ns_view) };
                log::info!("[Overlay] NSScreenSaverWindowLevel applied");
            }
        }
    }

    // Hide again after configuration.
    let _ = window.hide();
    log::info!("[Overlay] Init complete — window hidden at screen-saver level");
    Ok(())
}

pub fn show_overlay(app: &AppHandle) -> Result<(), String> {
    let state = app.state::<ClickerState>();
    if !state.settings_initialized.load(Ordering::SeqCst) { return Ok(()); }
    {
        let settings = state.settings.lock().unwrap();
        if !settings.show_stop_overlay { return Ok(()); }
    }
    let window = app
        .get_webview_window("overlay")
        .ok_or_else(|| "Overlay window not found".to_string())?;
    let bounds = current_virtual_screen_rect()
        .ok_or_else(|| "Virtual screen bounds not available".to_string())?;

    let _ = window.set_position(tauri::PhysicalPosition::new(bounds.left, bounds.top));
    let _ = window.set_size(tauri::PhysicalSize::new(bounds.width as u32, bounds.height as u32));
    if !window.is_visible().unwrap_or(false) { let _ = window.show(); }

    *LAST_ZONE_SHOW.lock().unwrap() = Some(Instant::now());

    let settings = state.settings.lock().unwrap();
    let monitors = current_monitor_rects().unwrap_or_else(|| vec![bounds]);
    let monitor_payload: Vec<_> = monitors.into_iter().map(|m| {
        let offset = m.offset_from(bounds);
        serde_json::json!({ "x": offset.left, "y": offset.top, "width": offset.width, "height": offset.height })
    }).collect();

    let _ = window.emit("zone-data", serde_json::json!({
        "edgeStopEnabled": settings.edge_stop_enabled,
        "edgeStopTop": settings.edge_stop_top,
        "edgeStopRight": settings.edge_stop_right,
        "edgeStopBottom": settings.edge_stop_bottom,
        "edgeStopLeft": settings.edge_stop_left,
        "cornerStopEnabled": settings.corner_stop_enabled,
        "cornerStopTL": settings.corner_stop_tl,
        "cornerStopTR": settings.corner_stop_tr,
        "cornerStopBL": settings.corner_stop_bl,
        "cornerStopBR": settings.corner_stop_br,
        "screenWidth": bounds.width,
        "screenHeight": bounds.height,
        "monitors": monitor_payload,
        "_showDisabledEdges": !settings.edge_stop_enabled,
        "_showDisabledCorners": !settings.corner_stop_enabled,
    }));
    Ok(())
}

pub fn check_auto_hide(app: &AppHandle) {
    let mut last = LAST_ZONE_SHOW.lock().unwrap();
    if let Some(instant) = *last {
        if instant.elapsed() >= Duration::from_secs(3) {
            *last = None;
            if let Some(window) = app.get_webview_window("overlay") {
                log::info!("[Overlay] Auto-hide");
                let _ = window.hide();
            }
        }
    }
}

#[tauri::command]
pub fn hide_overlay(app: AppHandle) -> Result<(), String> {
    *LAST_ZONE_SHOW.lock().unwrap() = None;
    if let Some(window) = app.get_webview_window("overlay") {
        let _ = window.hide();
    }
    Ok(())
}
