use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use tauri::{AppHandle, Emitter, Manager};

use crate::engine::mouse::current_cursor_position;
#[cfg(target_os = "macos")]
use crate::engine::mouse::{current_virtual_screen_rect, VirtualScreenRect};
use crate::ClickerState;

const CURSOR_EMIT_INTERVAL: Duration = Duration::from_millis(16);

#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct SequencePointPickedPayload {
    x: i32,
    y: i32,
    continue_picking: bool,
}

// ── Shared state ──────────────────────────────────────────────────────────────

#[derive(Default)]
struct PickerRuntime {
    active: bool,
    app: Option<AppHandle>,
    last_cursor_emit: Option<Instant>,
    stop_after_right_up: bool,
    #[cfg(target_os = "windows")]
    mouse_hook: *mut std::ffi::c_void,
    #[cfg(target_os = "windows")]
    keyboard_hook: *mut std::ffi::c_void,
    #[cfg(target_os = "windows")]
    thread_id: u32,
}

static PICKER: OnceLock<Mutex<PickerRuntime>> = OnceLock::new();

fn picker() -> &'static Mutex<PickerRuntime> {
    PICKER.get_or_init(|| Mutex::new(PickerRuntime::default()))
}

// ── Windows implementation ────────────────────────────────────────────────────

#[cfg(target_os = "windows")]
mod platform {
    use std::ffi::c_void;
    use std::sync::MutexGuard;
    use std::time::Instant;

    use tauri::Emitter;
    use windows_sys::Win32::Foundation::{LPARAM, LRESULT, WPARAM};
    use windows_sys::Win32::System::Threading::GetCurrentThreadId;
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
        GetAsyncKeyState, VK_CONTROL, VK_ESCAPE, VK_SHIFT,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        CallNextHookEx, GetMessageW, PostThreadMessageW, SetWindowsHookExW, UnhookWindowsHookEx,
        HHOOK, KBDLLHOOKSTRUCT, MSG, MSLLHOOKSTRUCT, WH_KEYBOARD_LL, WH_MOUSE_LL, WM_KEYDOWN,
        WM_MOUSEMOVE, WM_QUIT, WM_RBUTTONDBLCLK, WM_RBUTTONDOWN, WM_RBUTTONUP, WM_SYSKEYDOWN,
    };

    use super::{picker, PickerRuntime, SequencePointPickedPayload, CURSOR_EMIT_INTERVAL};
    use crate::engine::mouse::current_cursor_position;

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub enum MouseHookDecision {
        Pass,
        Swallow,
        Pick { continue_picking: bool },
        Delete,
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub enum KeyboardHookDecision {
        Pass,
        Cancel,
    }

    pub fn classify_mouse_message(
        message: u32,
        shift_down: bool,
        ctrl_down: bool,
    ) -> MouseHookDecision {
        match message {
            WM_RBUTTONDOWN if ctrl_down => MouseHookDecision::Delete,
            WM_RBUTTONDOWN => MouseHookDecision::Pick {
                continue_picking: shift_down,
            },
            WM_RBUTTONUP | WM_RBUTTONDBLCLK => MouseHookDecision::Swallow,
            _ => MouseHookDecision::Pass,
        }
    }

    pub fn classify_keyboard_message(message: u32, virtual_key: u32) -> KeyboardHookDecision {
        match (message, virtual_key) {
            (WM_KEYDOWN | WM_SYSKEYDOWN, key) if key == VK_ESCAPE as u32 => {
                KeyboardHookDecision::Cancel
            }
            _ => KeyboardHookDecision::Pass,
        }
    }

    unsafe extern "system" fn mouse_hook_proc(
        code: i32,
        w_param: WPARAM,
        l_param: LPARAM,
    ) -> LRESULT {
        if code < 0 {
            return CallNextHookEx(0, code, w_param, l_param);
        }

        let (app, stop_after_right_up) = {
            let runtime = picker().lock().unwrap();
            (runtime.app.clone(), runtime.stop_after_right_up)
        };

        if let Some(app) = app {
            let msg = w_param as u32;
            let shift_down = (unsafe { GetAsyncKeyState(VK_SHIFT as i32) } as u16) >> 15 != 0;
            let ctrl_down = (unsafe { GetAsyncKeyState(VK_CONTROL as i32) } as u16) >> 15 != 0;

            match classify_mouse_message(msg, shift_down, ctrl_down) {
                MouseHookDecision::Pick { continue_picking } => {
                    if let Some((x, y)) = current_cursor_position() {
                        let _ = app.emit(
                            "sequence-point-picked",
                            SequencePointPickedPayload {
                                x,
                                y,
                                continue_picking,
                            },
                        );
                    }

                    if !continue_picking {
                        let mut runtime = picker().lock().unwrap();
                        runtime.stop_after_right_up = true;
                    }

                    return 1; // Swallow the message
                }
                MouseHookDecision::Delete => {
                    if let Some((x, y)) = current_cursor_position() {
                        let _ = app.emit(
                            "sequence-point-delete-requested",
                            SequencePointPickedPayload {
                                x,
                                y,
                                continue_picking: false,
                            },
                        );
                    }
                    return 1;
                }
                MouseHookDecision::Swallow => {
                    if stop_after_right_up {
                        let runtime = picker().lock().unwrap();
                        if let Some(app) = &runtime.app {
                            let _ = app.emit("sequence-pick-ended", ());
                        }
                        drop(runtime);
                        super::cancel_sequence_point_pick_inner(&app);
                    } else {
                        return 1;
                    }
                }
                MouseHookDecision::Pass => {}
            }

            // Emit cursor position periodically for overlay tracking
            let now = Instant::now();
            let mut runtime = picker().lock().unwrap();
            let should_emit = runtime
                .last_cursor_emit
                .map_or(true, |t| now.duration_since(t) >= CURSOR_EMIT_INTERVAL);
            if should_emit {
                runtime.last_cursor_emit = Some(now);
                drop(runtime);
                if let Some((x, y)) = current_cursor_position() {
                    if let Some(bounds) = current_virtual_screen_rect() {
                        let offset = VirtualScreenRect::new(x, y, 1, 1).offset_from(bounds);
                        let _ = app.emit(
                            "sequence-pick-cursor",
                            serde_json::json!({ "x": offset.left, "y": offset.top }),
                        );
                    }
                }
            }
        }

        CallNextHookEx(0, code, w_param, l_param)
    }

    unsafe extern "system" fn keyboard_hook_proc(
        code: i32,
        w_param: WPARAM,
        l_param: LPARAM,
    ) -> LRESULT {
        if code < 0 {
            return CallNextHookEx(0, code, w_param, l_param);
        }

        let msg = w_param as u32;
        let kb = &*(l_param as *const KBDLLHOOKSTRUCT);

        if classify_keyboard_message(msg, kb.vkCode) == KeyboardHookDecision::Cancel {
            let app = picker().lock().unwrap().app.clone();
            drop(picker());
            if let Some(app) = app {
                super::cancel_sequence_point_pick_inner(&app);
            }
            return 1;
        }

        CallNextHookEx(0, code, w_param, l_param)
    }

    pub fn start_hooks(app: &AppHandle) -> Result<(), String> {
        let (ready_tx, ready_rx) = mpsc::channel();
        let app = app.clone();

        std::thread::spawn(move || unsafe {
            let thread_id = GetCurrentThreadId();
            let mouse_hook = SetWindowsHookExW(WH_MOUSE_LL, Some(mouse_hook_proc), 0, 0);
            if mouse_hook == 0 {
                let _ = ready_tx.send(Err(String::from("Failed to install mouse hook")));
                return;
            }

            let keyboard_hook = SetWindowsHookExW(WH_KEYBOARD_LL, Some(keyboard_hook_proc), 0, 0);
            if keyboard_hook == 0 {
                UnhookWindowsHookEx(mouse_hook);
                let _ = ready_tx.send(Err(String::from("Failed to install keyboard hook")));
                return;
            }

            {
                let mut runtime = picker().lock().unwrap();
                runtime.thread_id = thread_id;
                runtime.mouse_hook = mouse_hook;
                runtime.keyboard_hook = keyboard_hook;
            }
            let _ = ready_tx.send(Ok(()));

            let mut msg = std::mem::zeroed::<MSG>();
            while GetMessageW(&mut msg, 0, 0, 0) > 0 {}

            UnhookWindowsHookEx(mouse_hook);
            UnhookWindowsHookEx(keyboard_hook);
            let mut runtime = picker().lock().unwrap();
            if runtime.mouse_hook == mouse_hook {
                runtime.mouse_hook = 0;
            }
            if runtime.keyboard_hook == keyboard_hook {
                runtime.keyboard_hook = 0;
            }
            if runtime.mouse_hook == 0 && runtime.keyboard_hook == 0 {
                runtime.thread_id = 0;
            }
        });

        match ready_rx.recv_timeout(Duration::from_secs(1)) {
            Ok(Ok(())) => Ok(()),
            Ok(Err(e)) => {
                super::cancel_sequence_point_pick_inner(&app);
                Err(e)
            }
            Err(_) => {
                super::cancel_sequence_point_pick_inner(&app);
                Err(String::from("Timed out starting hooks"))
            }
        }
    }

    pub fn stop_hooks(notify_overlay: bool) -> Option<AppHandle> {
        let (app, thread_id) = {
            let mut runtime = picker().lock().unwrap();
            let app = runtime.app.clone();
            let thread_id = runtime.thread_id;
            runtime.active = false;
            runtime.app = None;
            runtime.last_cursor_emit = None;
            runtime.stop_after_right_up = false;
            (app, thread_id)
        };

        if let Some(app) = &app {
            app.state::<ClickerState>()
                .sequence_pick_active
                .store(false, Ordering::SeqCst);
            if notify_overlay {
                let _ = app.emit("sequence-pick-ended", ());
            }
        }

        if thread_id != 0 {
            unsafe {
                PostThreadMessageW(thread_id, WM_QUIT, 0, 0);
            }
        }

        app
    }
}

// ── macOS implementation ──────────────────────────────────────────────────────

#[cfg(target_os = "macos")]
mod platform {
    use std::ffi::c_void;
    use std::sync::atomic::{AtomicBool, Ordering};

    use tauri::{AppHandle, Emitter, Manager};

    use super::{picker, SequencePointPickedPayload};
    use crate::engine::mouse::{current_virtual_screen_rect, VirtualScreenRect};
    use crate::ClickerState;

    // Event types
    const NX_RMOUSEDOWN: u32 = 4;
    const NX_RMOUSEUP: u32 = 5;
    const NX_KEYDOWN: u32 = 10;

    // Event fields
    const KCG_KEYBOARD_EVENT_KEYCODE: u32 = 9;

    #[repr(C)]
    struct CGPoint {
        x: f64,
        y: f64,
    }

    // macOS virtual key code for Escape
    const VK_ESCAPE: u16 = 53;

    // Event tap constants
    const KCG_HID_EVENT_TAP: u32 = 0;
    const KCG_HEAD_INSERT_EVENT_TAP: u32 = 0;
    const KCG_EVENT_TAP_OPTION_LISTEN_ONLY: u32 = 1;

    #[link(name = "CoreGraphics", kind = "framework")]
    extern "C" {
        fn CGEventTapCreate(
            tap: u32,
            place: u32,
            options: u32,
            events_of_interest: u64,
            callback: unsafe extern "C" fn(
                proxy: *mut c_void,
                event_type: u32,
                event: *mut c_void,
                user_info: *mut c_void,
            ) -> *mut c_void,
            user_info: *mut c_void,
        ) -> *mut c_void;
        fn CGEventTapEnable(tap: *mut c_void, enable: bool);
        fn CGEventGetIntegerValueField(event: *mut c_void, field: u32) -> i64;
        fn CGEventGetLocation(event: *mut c_void) -> CGPoint;
    }

    #[link(name = "CoreFoundation", kind = "framework")]
    extern "C" {
        fn CFMachPortCreateRunLoopSource(
            allocator: *mut c_void,
            port: *mut c_void,
            order: i64,
        ) -> *mut c_void;
        fn CFRunLoopGetCurrent() -> *mut c_void;
        fn CFRunLoopAddSource(rl: *mut c_void, source: *mut c_void, mode: *mut c_void);
        fn CFRunLoopRun();
        fn CFRelease(cf: *mut c_void);
        static kCFRunLoopCommonModes: *mut c_void;
    }

    /// The event tap runs in a dedicated thread. When it captures a right-click,
    /// it writes the position here; the main thread polls this and emits events.
    ///
    /// We can't emit Tauri events directly from the tap callback because
    /// Tauri's event system requires being on the main thread.
    static PICK_REQUESTED: AtomicBool = AtomicBool::new(false);
    static DELETE_REQUESTED: AtomicBool = AtomicBool::new(false);
    static CANCEL_REQUESTED: AtomicBool = AtomicBool::new(false);
    static PICK_X: std::sync::atomic::AtomicI32 = std::sync::atomic::AtomicI32::new(0);
    static PICK_Y: std::sync::atomic::AtomicI32 = std::sync::atomic::AtomicI32::new(0);

    unsafe extern "C" fn tap_callback(
        _proxy: *mut c_void,
        event_type: u32,
        event: *mut c_void,
        _user_info: *mut c_void,
    ) -> *mut c_void {
        match event_type {
            NX_RMOUSEDOWN => {
                let point = CGEventGetLocation(event);
                PICK_X.store(point.x as i32, std::sync::atomic::Ordering::SeqCst);
                PICK_Y.store(point.y as i32, std::sync::atomic::Ordering::SeqCst);
                PICK_REQUESTED.store(true, std::sync::atomic::Ordering::SeqCst);
                std::ptr::null_mut() // swallow the event so it doesn't trigger context menus
            }
            NX_RMOUSEUP => {
                std::ptr::null_mut() // swallow
            }
            NX_KEYDOWN => {
                let code = CGEventGetIntegerValueField(event, KCG_KEYBOARD_EVENT_KEYCODE) as u16;
                if code == VK_ESCAPE {
                    CANCEL_REQUESTED.store(true, std::sync::atomic::Ordering::SeqCst);
                    std::ptr::null_mut() // swallow escape
                } else {
                    event // pass through
                }
            }
            _ => event, // pass through all other events
        }
    }

    pub fn start_hooks(app: &AppHandle) -> Result<(), String> {
        use std::sync::mpsc;

        // Reset state
        PICK_REQUESTED.store(false, Ordering::SeqCst);
        DELETE_REQUESTED.store(false, Ordering::SeqCst);
        CANCEL_REQUESTED.store(false, Ordering::SeqCst);

        let (ready_tx, ready_rx) = mpsc::channel();

        std::thread::spawn(move || unsafe {
            let events_of_interest =
                (1u64 << NX_RMOUSEDOWN) | (1u64 << NX_RMOUSEUP) | (1u64 << NX_KEYDOWN);

            let tap = CGEventTapCreate(
                KCG_HID_EVENT_TAP,
                KCG_HEAD_INSERT_EVENT_TAP,
                KCG_EVENT_TAP_OPTION_LISTEN_ONLY,
                events_of_interest,
                tap_callback,
                std::ptr::null_mut(),
            );

            if tap.is_null() {
                let _ = ready_tx.send(Err(String::from(
                    "Event tap creation failed – Accessibility permissions may be needed",
                )));
                return;
            }

            let source = CFMachPortCreateRunLoopSource(std::ptr::null_mut(), tap, 0);
            if source.is_null() {
                CFRelease(tap);
                let _ = ready_tx.send(Err(String::from("Run loop source creation failed")));
                return;
            }

            let _ = ready_tx.send(Ok(()));

            let rl = CFRunLoopGetCurrent();
            CFRunLoopAddSource(rl, source, kCFRunLoopCommonModes);
            CGEventTapEnable(tap, true);

            // Run the loop — blocks until CFRunLoopStop is called
            CFRunLoopRun();

            CGEventTapEnable(tap, false);
            CFRelease(source);
            CFRelease(tap);
        });

        match ready_rx.recv_timeout(std::time::Duration::from_secs(1)) {
            Ok(Ok(())) => Ok(()),
            Ok(Err(e)) => {
                super::cancel_sequence_point_pick_inner(app);
                Err(e)
            }
            Err(_) => {
                super::cancel_sequence_point_pick_inner(app);
                Err(String::from("Timed out starting event tap"))
            }
        }
    }

    pub fn stop_hooks(notify_overlay: bool) -> Option<AppHandle> {
        let app = {
            let mut runtime = picker().lock().unwrap();
            let app = runtime.app.clone();
            runtime.active = false;
            runtime.app = None;
            runtime.last_cursor_emit = None;
            runtime.stop_after_right_up = false;
            app
        };

        if let Some(app) = &app {
            app.state::<ClickerState>()
                .sequence_pick_active
                .store(false, Ordering::SeqCst);
            if notify_overlay {
                let _ = app.emit("sequence-pick-ended", ());
            }
        }

        // The event tap thread will stop when the run loop source is removed,
        // but CFRunLoopStop requires the run loop reference. Since the tap is
        // a listen-only tap, the thread will exit when the app exits or we
        // can just let it be cleaned up naturally.
        // For now, we rely on the active flag to stop processing.

        app
    }

    /// Called from the main thread polling loop to check for pick events
    pub fn poll_pick(app: &AppHandle) -> bool {
        if CANCEL_REQUESTED.swap(false, Ordering::SeqCst) {
            let app = app.clone();
            super::cancel_sequence_point_pick_inner(&app);
            return false;
        }

        if PICK_REQUESTED.swap(false, Ordering::SeqCst) {
            let x = PICK_X.load(Ordering::SeqCst);
            let y = PICK_Y.load(Ordering::SeqCst);

            let continue_picking = false; // shift-check not easily available from event tap
            let _ = app.emit(
                "sequence-point-picked",
                SequencePointPickedPayload {
                    x,
                    y,
                    continue_picking,
                },
            );

            // Emit cursor position for overlay tracking
            if let Some(bounds) = current_virtual_screen_rect() {
                let offset = VirtualScreenRect::new(x, y, 1, 1).offset_from(bounds);
                let _ = app.emit(
                    "sequence-pick-cursor",
                    serde_json::json!({ "x": offset.left, "y": offset.top }),
                );
            }

            return true;
        }

        false
    }
}

// ── Public API ─────────────────────────────────────────────────────────────────

pub fn start_sequence_point_pick_inner(app: AppHandle) -> Result<(), String> {
    crate::custom_stop_zone_picker::cancel_custom_stop_zone_pick_inner(&app);

    {
        let mut runtime = picker().lock().unwrap();
        if runtime.active {
            crate::overlay::show_sequence_pick_overlay(&app)?;
            return Ok(());
        }

        runtime.active = true;
        runtime.app = Some(app.clone());
        runtime.last_cursor_emit = None;
        runtime.stop_after_right_up = false;
    }

    app.state::<ClickerState>()
        .sequence_pick_active
        .store(true, std::sync::atomic::Ordering::SeqCst);

    crate::overlay::show_sequence_pick_overlay(&app)?;

    #[cfg(target_os = "macos")]
    {
        platform::start_hooks(&app)?;

        // Spawn a polling thread that checks for pick events from the event tap
        // and emits them as Tauri events (which must happen on a Tauri-capable context).
        let app_handle = app.clone();
        std::thread::spawn(move || {
            let mut cursor_emit = std::time::Instant::now();
            loop {
                let runtime = picker().lock().unwrap();
                if !runtime.active {
                    break;
                }
                drop(runtime);

                platform::poll_pick(&app_handle);

                // Emit cursor position periodically for overlay tracking
                let now = std::time::Instant::now();
                if now.duration_since(cursor_emit) >= CURSOR_EMIT_INTERVAL {
                    cursor_emit = now;
                    if let Some((x, y)) = current_cursor_position() {
                        if let Some(bounds) = current_virtual_screen_rect() {
                            let offset = VirtualScreenRect::new(x, y, 1, 1).offset_from(bounds);
                            let _ = app_handle.emit(
                                "sequence-pick-cursor",
                                serde_json::json!({ "x": offset.left, "y": offset.top }),
                            );
                        }
                    }
                }

                std::thread::sleep(Duration::from_millis(16));
            }
        });
    }

    #[cfg(target_os = "windows")]
    platform::start_hooks(&app)?;

    Ok(())
}

pub fn cancel_sequence_point_pick_inner(app: &AppHandle) {
    let app_opt = platform::stop_hooks(true);
    if let Some(a) = app_opt.or(Some(app.clone())) {
        let _ = crate::overlay::hide_overlay(a);
    }
}
