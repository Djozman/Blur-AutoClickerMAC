use crate::engine::worker::now_epoch_ms;
use crate::engine::worker::start_clicker_inner;
use crate::engine::worker::stop_clicker_inner;
use crate::engine::worker::toggle_clicker_inner;
use crate::AppHandle;
use crate::ClickerState;
use std::sync::atomic::Ordering;
use std::time::Duration;
use tauri::Manager;

// ── Platform-specific virtual-key constants ───────────────────────────────────

#[cfg(target_os = "windows")]
use windows_sys::Win32::UI::Input::KeyboardAndMouse::*;

/// On macOS these map to CGKeyCode values (u16).
/// Mouse-button codes use the range 0xFFF0–0xFFF4 (not real keycodes;
/// handled specially in is_vk_down).
#[cfg(target_os = "macos")]
mod vk_codes {
    pub const VK_CONTROL: u16 = 0x3B; // left Control
    pub const VK_RCONTROL: u16 = 0x3E; // right Control
    pub const VK_MENU: u16 = 0x3A; // left Option / Alt
    pub const VK_RMENU: u16 = 0x3D; // right Option
    pub const VK_SHIFT: u16 = 0x38; // left Shift
    pub const VK_RSHIFT: u16 = 0x3C; // right Shift
    pub const VK_LWIN: u16 = 0x37; // left Command
    pub const VK_RWIN: u16 = 0x36; // right Command

    pub const VK_SPACE: u16 = 0x31;
    pub const VK_TAB: u16 = 0x30;
    pub const VK_RETURN: u16 = 0x24;
    pub const VK_BACK: u16 = 0x33;
    pub const VK_DELETE: u16 = 0x75;
    pub const VK_INSERT: u16 = 0x72; // Help key on Mac
    pub const VK_HOME: u16 = 0x73;
    pub const VK_END: u16 = 0x77;
    pub const VK_PRIOR: u16 = 0x74; // Page Up
    pub const VK_NEXT: u16 = 0x79; // Page Down
    pub const VK_UP: u16 = 0x7E;
    pub const VK_DOWN: u16 = 0x7D;
    pub const VK_LEFT: u16 = 0x7B;
    pub const VK_RIGHT: u16 = 0x7C;
    pub const VK_ESCAPE: u16 = 0x35;
    pub const VK_CAPITAL: u16 = 0x39; // Caps Lock
    pub const VK_NUMLOCK: u16 = 0x47; // Clear on Mac numpad
    pub const VK_SCROLL: u16 = 0xFF; // no Scroll Lock on Mac
    pub const VK_APPS: u16 = 0xFF; // no Apps key on Mac
    pub const VK_SNAPSHOT: u16 = 0xFF; // no Print Screen on Mac
    pub const VK_PAUSE: u16 = 0xFF; // no Pause on Mac

    // OEM / punctuation (US ANSI layout)
    pub const VK_OEM_2: u16 = 0x2C; // /
    pub const VK_OEM_5: u16 = 0x2A; // backslash
    pub const VK_OEM_1: u16 = 0x29; // ;
    pub const VK_OEM_7: u16 = 0x27; // '
    pub const VK_OEM_4: u16 = 0x21; // [
    pub const VK_OEM_6: u16 = 0x1E; // ]
    pub const VK_OEM_MINUS: u16 = 0x1B; // -
    pub const VK_OEM_PLUS: u16 = 0x18; // =
    pub const VK_OEM_3: u16 = 0x32; // `
    pub const VK_OEM_COMMA: u16 = 0x2B; // ,
    pub const VK_OEM_PERIOD: u16 = 0x2F; // .
    pub const VK_OEM_102: u16 = 0x0A; // IntlBackslash (non-US)

    // Function keys (non-sequential on macOS)
    pub const VK_F1: u16 = 0x7A;

    // Numpad
    pub const VK_NUMPAD0: u16 = 0x52;
    pub const VK_NUMPAD1: u16 = 0x53;
    pub const VK_NUMPAD2: u16 = 0x54;
    pub const VK_NUMPAD3: u16 = 0x55;
    pub const VK_NUMPAD4: u16 = 0x56;
    pub const VK_NUMPAD5: u16 = 0x57;
    pub const VK_NUMPAD6: u16 = 0x58;
    pub const VK_NUMPAD7: u16 = 0x59;
    pub const VK_NUMPAD8: u16 = 0x5B;
    pub const VK_NUMPAD9: u16 = 0x5C;
    pub const VK_ADD: u16 = 0x45;
    pub const VK_SUBTRACT: u16 = 0x4E;
    pub const VK_MULTIPLY: u16 = 0x43;
    pub const VK_DIVIDE: u16 = 0x4B;
    pub const VK_DECIMAL: u16 = 0x41;

    // Mouse buttons encoded above the CGKeyCode range
    pub const VK_LBUTTON: u16 = 0xFFF0;
    pub const VK_RBUTTON: u16 = 0xFFF1;
    pub const VK_MBUTTON: u16 = 0xFFF2;
    pub const VK_XBUTTON1: u16 = 0xFFF3;
    pub const VK_XBUTTON2: u16 = 0xFFF4;
}

#[cfg(target_os = "macos")]
use vk_codes::*;

// ── macOS key-state polling ───────────────────────────────────────────────────

#[cfg(target_os = "macos")]
mod macos_input {
    #[link(name = "CoreGraphics", kind = "framework")]
    extern "C" {
        pub fn CGEventSourceKeyState(state_id: i32, key: u16) -> bool;
        pub fn CGEventSourceButtonState(state_id: i32, button: u32) -> bool;
    }

    pub const HID_SYSTEM_STATE: i32 = 1;
}

// ── macOS CGEventTap-based key-state tracking (lock‑free) ────────────────────
// On macOS 14+ (especially with M‑series chips), CGEventSourceKeyState with
// kCGEventSourceStateHIDSystemState does NOT report key presses when the app is
// not the frontmost process.  A CGEventTap fixes this because it hooks into the
// HID event stream *before* the window server filters events by active Space.
#[cfg(target_os = "macos")]
mod macos_event_tap {
    use super::macos_input;
    use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

    // ── CGEventTap FFI ────────────────────────────────────────────────────────
    #[link(name = "CoreGraphics", kind = "framework")]
    extern "C" {
        fn CGEventTapCreate(
            tap: u32,
            place: u32,
            options: u32,
            events_of_interest: u64,
            callback: extern "C" fn(
                *mut std::ffi::c_void,
                u32,
                *mut std::ffi::c_void,
                *mut std::ffi::c_void,
            ) -> *mut std::ffi::c_void,
            user_info: *mut std::ffi::c_void,
        ) -> *mut std::ffi::c_void;
        fn CGEventTapEnable(tap: *mut std::ffi::c_void, enable: bool);
        fn CGEventGetIntegerValueField(event: *mut std::ffi::c_void, field: u32) -> i64;
    }

    #[link(name = "CoreFoundation", kind = "framework")]
    extern "C" {
        fn CFMachPortCreateRunLoopSource(
            allocator: *mut std::ffi::c_void,
            port: *mut std::ffi::c_void,
            order: i64,
        ) -> *mut std::ffi::c_void;
        fn CFRunLoopGetCurrent() -> *mut std::ffi::c_void;
        fn CFRunLoopAddSource(
            rl: *mut std::ffi::c_void,
            source: *mut std::ffi::c_void,
            mode: *mut std::ffi::c_void,
        );
        fn CFRunLoopRun();
        fn CFRunLoopStop(rl: *mut std::ffi::c_void);
        static kCFRunLoopCommonModes: *mut std::ffi::c_void;
    }

    // ── Constants ────────────────────────────────────────────────────────────
    const KCG_HID_EVENT_TAP: u32 = 0;
    const KCG_HEAD_INSERT_EVENT_TAP: u32 = 0;
    const KCG_EVENT_TAP_OPTION_LISTEN_ONLY: u32 = 1;
    const KCG_EVENT_KEY_DOWN: u32 = 10; // NX_KEYDOWN
    const KCG_EVENT_KEY_UP: u32 = 11; // NX_KEYUP
    const KCG_KEYBOARD_EVENT_KEYCODE: u32 = 9;

    // ── Lock‑free key state (4 × AtomicU64 covering CGKeyCode 0…255) ─────────
    static B0: AtomicU64 = AtomicU64::new(0); // keys   0… 63
    static B1: AtomicU64 = AtomicU64::new(0); // keys  64…127
    static B2: AtomicU64 = AtomicU64::new(0); // keys 128…191
    static B3: AtomicU64 = AtomicU64::new(0); // keys 192…255
    pub static ACTIVE: AtomicBool = AtomicBool::new(false);

    fn set(code: u16, down: bool) {
        let code = code as usize;
        if code > 255 {
            return;
        }
        let bucket = code / 64;
        let mask = 1u64 << (code as u64 % 64);
        let atom = match bucket {
            0 => &B0,
            1 => &B1,
            2 => &B2,
            3 => &B3,
            _ => return,
        };
        if down {
            atom.fetch_or(mask, Ordering::SeqCst);
        } else {
            atom.fetch_and(!mask, Ordering::SeqCst);
        }
    }

    pub fn is_down(code: u16) -> bool {
        let code = code as usize;
        if code > 255 {
            return false;
        }
        let bucket = code / 64;
        let mask = 1u64 << (code as u64 % 64);
        let atom = match bucket {
            0 => &B0,
            1 => &B1,
            2 => &B2,
            3 => &B3,
            _ => return false,
        };
        (atom.load(Ordering::SeqCst) & mask) != 0
    }

    extern "C" fn callback(
        _proxy: *mut std::ffi::c_void,
        event_type: u32,
        event: *mut std::ffi::c_void,
        _user_info: *mut std::ffi::c_void,
    ) -> *mut std::ffi::c_void {
        let code = unsafe { CGEventGetIntegerValueField(event, KCG_KEYBOARD_EVENT_KEYCODE) } as u16;
        set(code, event_type == KCG_EVENT_KEY_DOWN);
        event // pass through
    }

    /// Spawn a background thread that creates a key‑event tap and updates the
    /// atomic bitmap.  Returns immediately; the thread keeps running.
    pub fn start() {
        std::thread::spawn(|| unsafe {
            let tap = CGEventTapCreate(
                KCG_HID_EVENT_TAP,
                KCG_HEAD_INSERT_EVENT_TAP,
                KCG_EVENT_TAP_OPTION_LISTEN_ONLY,
                (1u64 << KCG_EVENT_KEY_DOWN) | (1u64 << KCG_EVENT_KEY_UP),
                callback,
                std::ptr::null_mut(),
            );

            if tap.is_null() {
                log::warn!(
                    "[Hotkey] CGEventTapCreate failed – Accessibility permissions needed. \
                     Polling fallback active."
                );
                return;
            }

            let source = CFMachPortCreateRunLoopSource(std::ptr::null_mut(), tap, 0);
            if source.is_null() {
                log::warn!("[Hotkey] CFMachPortCreateRunLoopSource failed.");
                return;
            }

            let rl = CFRunLoopGetCurrent();
            CFRunLoopAddSource(rl, source, kCFRunLoopCommonModes);
            CGEventTapEnable(tap, true);

            ACTIVE.store(true, Ordering::SeqCst);
            log::info!("[Hotkey] Event tap active.");

            CFRunLoopRun(); // blocks until CFRunLoopStop is called
        });
    }

    // ── Mouse‑button state keeps using CGEventSourceButtonState ──────────────
    pub fn is_mouse_down(button: u32) -> bool {
        unsafe { macos_input::CGEventSourceButtonState(macos_input::HID_SYSTEM_STATE, button) }
    }
}

// ── Shared data types ─────────────────────────────────────────────────────────

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HotkeyBinding {
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    pub super_key: bool,
    pub main_vk: i32,
    pub key_token: String,
}

// ── Public hotkey API ─────────────────────────────────────────────────────────

pub fn register_hotkey_inner(app: &AppHandle, hotkey: String) -> Result<String, String> {
    let binding = parse_hotkey_binding(&hotkey)?;
    let state = app.state::<ClickerState>();
    state
        .suppress_hotkey_until_ms
        .store(now_epoch_ms().saturating_add(250), Ordering::SeqCst);
    state
        .suppress_hotkey_until_release
        .store(true, Ordering::SeqCst);
    *state.registered_hotkey.lock().unwrap() = Some(binding.clone());

    Ok(format_hotkey_binding(&binding))
}

pub fn normalize_hotkey(value: &str) -> String {
    value.trim().to_ascii_lowercase()
}

pub fn parse_hotkey_binding(hotkey: &str) -> Result<HotkeyBinding, String> {
    let normalized = normalize_hotkey(hotkey);
    let mut ctrl = false;
    let mut alt = false;
    let mut shift = false;
    let mut super_key = false;
    let mut main_key: Option<(i32, String)> = None;

    for token in normalized.split('+').map(str::trim) {
        if token.is_empty() {
            return Err(format!("Invalid hotkey '{hotkey}': found empty key token"));
        }

        match normalize_modifier_token(token) {
            Some("ctrl") => ctrl = true,
            Some("alt") => alt = true,
            Some("shift") => shift = true,
            Some("super") => super_key = true,
            Some(_) => {}
            None => {
                if main_key
                    .replace(parse_hotkey_main_key(token, hotkey)?)
                    .is_some()
                {
                    return Err(format!(
                        "Invalid hotkey '{hotkey}': use modifiers first and only one main key"
                    ));
                }
            }
        }
    }

    let (main_vk, key_token) =
        main_key.ok_or_else(|| format!("Invalid hotkey '{hotkey}': missing main key"))?;

    Ok(HotkeyBinding {
        ctrl,
        alt,
        shift,
        super_key,
        main_vk,
        key_token,
    })
}

pub fn parse_hotkey_main_key(token: &str, original_hotkey: &str) -> Result<(i32, String), String> {
    let lower = token.trim().to_ascii_lowercase();

    if let Some(binding) = parse_named_key_token(&lower) {
        return Ok(binding);
    }
    if let Some(binding) = parse_mouse_button_token(&lower) {
        return Ok(binding);
    }
    if let Some(binding) = parse_numpad_token(&lower) {
        return Ok(binding);
    }
    if let Some(binding) = parse_function_key_token(&lower) {
        return Ok(binding);
    }

    if let Some(letter) = lower.strip_prefix("key") {
        if letter.len() == 1 {
            return parse_hotkey_main_key(letter, original_hotkey);
        }
    }
    if let Some(digit) = lower.strip_prefix("digit") {
        if digit.len() == 1 {
            return parse_hotkey_main_key(digit, original_hotkey);
        }
    }

    if lower.len() == 1 {
        let ch = lower.as_bytes()[0] as char;
        if ch.is_ascii_lowercase() {
            return letter_to_vk(ch)
                .map(|vk| (vk, lower.clone()))
                .ok_or_else(|| {
                    format!("Couldn't recognize '{token}' as a valid key in '{original_hotkey}'")
                });
        }
        if ch.is_ascii_digit() {
            return digit_to_vk(ch)
                .map(|vk| (vk, lower.clone()))
                .ok_or_else(|| {
                    format!("Couldn't recognize '{token}' as a valid key in '{original_hotkey}'")
                });
        }
    }

    Err(format!(
        "Couldn't recognize '{token}' as a valid key in '{original_hotkey}'"
    ))
}

// ── Platform-specific: letter → VK code ──────────────────────────────────────

/// On Windows, VK_A…VK_Z equal the ASCII uppercase value.
#[cfg(target_os = "windows")]
fn letter_to_vk(ch: char) -> Option<i32> {
    Some(ch.to_ascii_uppercase() as i32)
}

/// On macOS, letters map to CGKeyCode (ANSI US layout positions).
#[cfg(target_os = "macos")]
fn letter_to_vk(ch: char) -> Option<i32> {
    let code: u16 = match ch {
        'a' => 0x00,
        's' => 0x01,
        'd' => 0x02,
        'f' => 0x03,
        'h' => 0x04,
        'g' => 0x05,
        'z' => 0x06,
        'x' => 0x07,
        'c' => 0x08,
        'v' => 0x09,
        'b' => 0x0B,
        'q' => 0x0C,
        'w' => 0x0D,
        'e' => 0x0E,
        'r' => 0x0F,
        'y' => 0x10,
        't' => 0x11,
        'o' => 0x1F,
        'u' => 0x20,
        'i' => 0x22,
        'p' => 0x23,
        'l' => 0x25,
        'j' => 0x26,
        'k' => 0x28,
        'n' => 0x2D,
        'm' => 0x2E,
        _ => return None,
    };
    Some(code as i32)
}

/// On Windows, VK_0…VK_9 equal the ASCII digit value.
#[cfg(target_os = "windows")]
fn digit_to_vk(ch: char) -> Option<i32> {
    Some(ch as i32)
}

/// On macOS, digit keys have their own CGKeyCodes.
#[cfg(target_os = "macos")]
fn digit_to_vk(ch: char) -> Option<i32> {
    let code: u16 = match ch {
        '1' => 0x12,
        '2' => 0x13,
        '3' => 0x14,
        '4' => 0x15,
        '6' => 0x16,
        '5' => 0x17,
        '9' => 0x19,
        '7' => 0x1A,
        '8' => 0x1C,
        '0' => 0x1D,
        _ => return None,
    };
    Some(code as i32)
}

// ── Platform-specific: function key parsing ───────────────────────────────────

/// Windows function keys are sequential from VK_F1.
#[cfg(target_os = "windows")]
fn parse_function_key_token(token: &str) -> Option<(i32, String)> {
    if !token.starts_with('f') || token.len() > 3 {
        return None;
    }
    let number = token[1..].parse::<i32>().ok()?;
    let vk = match number {
        1..=24 => VK_F1 as i32 + (number - 1),
        _ => return None,
    };
    Some(binding(vk, token))
}

/// macOS function keys are non-sequential CGKeyCodes.
#[cfg(target_os = "macos")]
fn parse_function_key_token(token: &str) -> Option<(i32, String)> {
    if !token.starts_with('f') || token.len() > 3 {
        return None;
    }
    let number = token[1..].parse::<u32>().ok()?;
    let vk: u16 = match number {
        1 => 0x7A,
        2 => 0x78,
        3 => 0x63,
        4 => 0x76,
        5 => 0x60,
        6 => 0x61,
        7 => 0x62,
        8 => 0x64,
        9 => 0x65,
        10 => 0x6D,
        11 => 0x67,
        12 => 0x6F,
        13 => 0x69,
        14 => 0x6B,
        15 => 0x71,
        16 => 0x6A,
        17 => 0x40,
        18 => 0x4F,
        19 => 0x50,
        20 => 0x5A,
        _ => return None,
    };
    Some(binding(vk as i32, token))
}

// ── Platform-specific: is_vk_down ────────────────────────────────────────────

#[cfg(target_os = "windows")]
pub fn is_vk_down(vk: i32) -> bool {
    unsafe { (GetAsyncKeyState(vk) as u16 & 0x8000) != 0 }
}

#[cfg(target_os = "macos")]
pub fn is_vk_down(vk: i32) -> bool {
    match vk as u16 {
        0xFFF0 => macos_event_tap::is_mouse_down(0),
        0xFFF1 => macos_event_tap::is_mouse_down(1),
        0xFFF2 => macos_event_tap::is_mouse_down(2),
        0xFFF3 => macos_event_tap::is_mouse_down(3),
        0xFFF4 => macos_event_tap::is_mouse_down(4),
        0xFF => false, // placeholder for keys with no Mac equivalent
        key => {
            if macos_event_tap::ACTIVE.load(Ordering::SeqCst) {
                macos_event_tap::is_down(key)
            } else {
                unsafe { macos_input::CGEventSourceKeyState(macos_input::HID_SYSTEM_STATE, key) }
            }
        }
    }
}

// ── Hotkey listener / dispatcher ──────────────────────────────────────────────

pub fn start_hotkey_listener(app: AppHandle) {
    // On macOS, try to start the CGEventTap for reliable background key tracking.
    // This runs on its own thread and falls back to polling if Accessibility is
    // not granted.
    #[cfg(target_os = "macos")]
    macos_event_tap::start();

    std::thread::spawn(move || {
        let mut was_pressed = false;

        loop {
            let (binding, strict) = {
                let state = app.state::<ClickerState>();
                let binding = state.registered_hotkey.lock().unwrap().clone();
                let strict = state.settings.lock().unwrap().strict_hotkey_modifiers;
                (binding, strict)
            };

            let currently_pressed = binding
                .as_ref()
                .map(|binding| is_hotkey_binding_pressed(binding, strict))
                .unwrap_or(false);

            let suppress_until = app
                .state::<ClickerState>()
                .suppress_hotkey_until_ms
                .load(Ordering::SeqCst);
            let suppress_until_release = app
                .state::<ClickerState>()
                .suppress_hotkey_until_release
                .load(Ordering::SeqCst);
            let hotkey_capture_active = app
                .state::<ClickerState>()
                .hotkey_capture_active
                .load(Ordering::SeqCst);

            if hotkey_capture_active {
                was_pressed = currently_pressed;
                std::thread::sleep(Duration::from_millis(12));
                continue;
            }

            if suppress_until_release {
                if currently_pressed {
                    was_pressed = true;
                    std::thread::sleep(Duration::from_millis(12));
                    continue;
                }
                app.state::<ClickerState>()
                    .suppress_hotkey_until_release
                    .store(false, Ordering::SeqCst);
                was_pressed = false;
                std::thread::sleep(Duration::from_millis(12));
                continue;
            }

            if now_epoch_ms() < suppress_until {
                was_pressed = currently_pressed;
                std::thread::sleep(Duration::from_millis(12));
                continue;
            }

            if currently_pressed && !was_pressed {
                handle_hotkey_pressed(&app);
            } else if !currently_pressed && was_pressed {
                handle_hotkey_released(&app);
            }

            was_pressed = currently_pressed;
            std::thread::sleep(Duration::from_millis(12));
        }
    });
}

pub fn handle_hotkey_pressed(app: &AppHandle) {
    let mode = {
        let state = app.state::<ClickerState>();
        let mode = state.settings.lock().unwrap().mode.clone();
        mode
    };
    if mode == "Toggle" {
        let _ = toggle_clicker_inner(app);
    } else if mode == "Hold" {
        let _ = start_clicker_inner(app);
    }
}

pub fn handle_hotkey_released(app: &AppHandle) {
    let mode = {
        let state = app.state::<ClickerState>();
        let mode = state.settings.lock().unwrap().mode.clone();
        mode
    };
    if mode == "Hold" {
        let _ = stop_clicker_inner(app, Some(String::from("Stopped from hold hotkey")));
    }
}

pub fn is_hotkey_binding_pressed(binding: &HotkeyBinding, strict: bool) -> bool {
    let ctrl_down = is_vk_down(VK_CONTROL as i32) || is_vk_down(VK_RCONTROL as i32);
    let alt_down = is_vk_down(VK_MENU as i32) || is_vk_down(VK_RMENU as i32);
    let shift_down = is_vk_down(VK_SHIFT as i32) || is_vk_down(VK_RSHIFT as i32);
    let super_down = is_vk_down(VK_LWIN as i32) || is_vk_down(VK_RWIN as i32);

    if !modifiers_match(binding, ctrl_down, alt_down, shift_down, super_down, strict) {
        return false;
    }
    is_vk_down(binding.main_vk)
}

fn modifiers_match(
    binding: &HotkeyBinding,
    ctrl_down: bool,
    alt_down: bool,
    shift_down: bool,
    super_down: bool,
    strict: bool,
) -> bool {
    if binding.ctrl && !ctrl_down {
        return false;
    }
    if binding.alt && !alt_down {
        return false;
    }
    if binding.shift && !shift_down {
        return false;
    }
    if binding.super_key && !super_down {
        return false;
    }

    if strict {
        if ctrl_down && !binding.ctrl {
            return false;
        }
        if alt_down && !binding.alt {
            return false;
        }
        if shift_down && !binding.shift {
            return false;
        }
        if super_down && !binding.super_key {
            return false;
        }
    }
    true
}

// ── Shared key-token parsers ──────────────────────────────────────────────────

pub fn format_hotkey_binding(binding: &HotkeyBinding) -> String {
    let mut parts: Vec<String> = Vec::new();
    if binding.ctrl {
        parts.push(String::from("ctrl"));
    }
    if binding.alt {
        parts.push(String::from("alt"));
    }
    if binding.shift {
        parts.push(String::from("shift"));
    }
    if binding.super_key {
        parts.push(String::from("super"));
    }
    parts.push(binding.key_token.clone());
    parts.join("+")
}

fn normalize_modifier_token(token: &str) -> Option<&'static str> {
    match token {
        "alt" | "option" => Some("alt"),
        "ctrl" | "control" => Some("ctrl"),
        "shift" => Some("shift"),
        "super" | "command" | "cmd" | "meta" | "win" => Some("super"),
        _ => None,
    }
}

fn binding(vk: i32, token: &str) -> (i32, String) {
    (vk, token.to_string())
}

fn parse_named_key_token(token: &str) -> Option<(i32, String)> {
    match token {
        "<" | ">" | "intlbackslash" | "oem102" | "nonusbackslash" => {
            Some(binding(VK_OEM_102 as i32, "IntlBackslash"))
        }
        "space" | "spacebar" => Some(binding(VK_SPACE as i32, "space")),
        "tab" => Some(binding(VK_TAB as i32, "tab")),
        "enter" | "return" => Some(binding(VK_RETURN as i32, "enter")),
        "backspace" => Some(binding(VK_BACK as i32, "backspace")),
        "delete" | "del" => Some(binding(VK_DELETE as i32, "delete")),
        "insert" | "ins" => Some(binding(VK_INSERT as i32, "insert")),
        "home" => Some(binding(VK_HOME as i32, "home")),
        "end" => Some(binding(VK_END as i32, "end")),
        "pageup" | "pgup" => Some(binding(VK_PRIOR as i32, "pageup")),
        "pagedown" | "pgdn" => Some(binding(VK_NEXT as i32, "pagedown")),
        "up" | "arrowup" => Some(binding(VK_UP as i32, "up")),
        "down" | "arrowdown" => Some(binding(VK_DOWN as i32, "down")),
        "left" | "arrowleft" => Some(binding(VK_LEFT as i32, "left")),
        "right" | "arrowright" => Some(binding(VK_RIGHT as i32, "right")),
        "esc" | "escape" => Some(binding(VK_ESCAPE as i32, "escape")),
        "capslock" => Some(binding(VK_CAPITAL as i32, "capslock")),
        "numlock" => Some(binding(VK_NUMLOCK as i32, "numlock")),
        "scrolllock" => Some(binding(VK_SCROLL as i32, "scrolllock")),
        "menu" | "apps" | "contextmenu" => Some(binding(VK_APPS as i32, "menu")),
        "printscreen" | "prtsc" | "snapshot" => Some(binding(VK_SNAPSHOT as i32, "printscreen")),
        "pause" | "break" => Some(binding(VK_PAUSE as i32, "pause")),
        "/" | "slash" => Some(binding(VK_OEM_2 as i32, "/")),
        "\\" | "backslash" => Some(binding(VK_OEM_5 as i32, "\\")),
        ";" | "semicolon" => Some(binding(VK_OEM_1 as i32, ";")),
        "'" | "quote" | "apostrophe" => Some(binding(VK_OEM_7 as i32, "'")),
        "[" | "bracketleft" => Some(binding(VK_OEM_4 as i32, "[")),
        "]" | "bracketright" => Some(binding(VK_OEM_6 as i32, "]")),
        "-" | "minus" => Some(binding(VK_OEM_MINUS as i32, "-")),
        "=" | "equal" => Some(binding(VK_OEM_PLUS as i32, "=")),
        "`" | "backquote" | "grave" => Some(binding(VK_OEM_3 as i32, "`")),
        "," | "comma" => Some(binding(VK_OEM_COMMA as i32, ",")),
        "." | "period" | "dot" => Some(binding(VK_OEM_PERIOD as i32, ".")),
        _ => None,
    }
}

fn parse_mouse_button_token(token: &str) -> Option<(i32, String)> {
    match token {
        "mouseleft" | "leftmouse" | "leftbutton" | "mouse1" | "lmb" => {
            Some(binding(VK_LBUTTON as i32, "mouseleft"))
        }
        "mouseright" | "rightmouse" | "rightbutton" | "mouse2" | "rmb" => {
            Some(binding(VK_RBUTTON as i32, "mouseright"))
        }
        "mousemiddle" | "middlemouse" | "middlebutton" | "mouse3" | "mmb" | "scrollbutton"
        | "middleclick" => Some(binding(VK_MBUTTON as i32, "mousemiddle")),
        "mouse4" | "xbutton1" | "mouseback" | "browserback" | "backbutton" => {
            Some(binding(VK_XBUTTON1 as i32, "mouse4"))
        }
        "mouse5" | "xbutton2" | "mouseforward" | "browserforward" | "forwardbutton" => {
            Some(binding(VK_XBUTTON2 as i32, "mouse5"))
        }
        _ => None,
    }
}

fn parse_numpad_token(token: &str) -> Option<(i32, String)> {
    match token {
        "numpad0" | "num0" => Some(binding(VK_NUMPAD0 as i32, "numpad0")),
        "numpad1" | "num1" => Some(binding(VK_NUMPAD1 as i32, "numpad1")),
        "numpad2" | "num2" => Some(binding(VK_NUMPAD2 as i32, "numpad2")),
        "numpad3" | "num3" => Some(binding(VK_NUMPAD3 as i32, "numpad3")),
        "numpad4" | "num4" => Some(binding(VK_NUMPAD4 as i32, "numpad4")),
        "numpad5" | "num5" => Some(binding(VK_NUMPAD5 as i32, "numpad5")),
        "numpad6" | "num6" => Some(binding(VK_NUMPAD6 as i32, "numpad6")),
        "numpad7" | "num7" => Some(binding(VK_NUMPAD7 as i32, "numpad7")),
        "numpad8" | "num8" => Some(binding(VK_NUMPAD8 as i32, "numpad8")),
        "numpad9" | "num9" => Some(binding(VK_NUMPAD9 as i32, "numpad9")),
        "numpadadd" | "numadd" | "numpadplus" | "numplus" => {
            Some(binding(VK_ADD as i32, "numpadadd"))
        }
        "numpadsubtract" | "numsubtract" | "numsub" | "numpadminus" | "numminus" => {
            Some(binding(VK_SUBTRACT as i32, "numpadsubtract"))
        }
        "numpadmultiply" | "nummultiply" | "nummul" | "numpadmul" => {
            Some(binding(VK_MULTIPLY as i32, "numpadmultiply"))
        }
        "numpaddivide" | "numdivide" | "numdiv" | "numpaddiv" => {
            Some(binding(VK_DIVIDE as i32, "numpaddivide"))
        }
        "numpaddecimal" | "numdecimal" | "numdot" | "numdel" | "numpadpoint" => {
            Some(binding(VK_DECIMAL as i32, "numpaddecimal"))
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::{format_hotkey_binding, modifiers_match, parse_hotkey_binding};

    #[test]
    fn numpad_tokens_round_trip() {
        for token in [
            "numpad0",
            "numpad1",
            "numpad2",
            "numpad3",
            "numpad4",
            "numpad5",
            "numpad6",
            "numpad7",
            "numpad8",
            "numpad9",
            "numpadadd",
            "numpadsubtract",
            "numpadmultiply",
            "numpaddivide",
            "numpaddecimal",
        ] {
            let hotkey = format!("ctrl+shift+{token}");
            let binding = parse_hotkey_binding(&hotkey).expect("token should parse");
            assert_eq!(binding.key_token, token);
            assert_eq!(format_hotkey_binding(&binding), hotkey);
        }
    }

    #[test]
    fn empty_hotkeys_are_rejected() {
        assert!(parse_hotkey_binding("").is_err());
        assert!(parse_hotkey_binding("ctrl+").is_err());
    }

    #[test]
    fn extra_modifiers_do_not_block_hotkeys_in_relaxed_mode() {
        let binding = parse_hotkey_binding("f11").expect("hotkey should parse");
        assert!(modifiers_match(&binding, false, false, true, false, false));
        assert!(modifiers_match(&binding, true, true, true, true, false));
    }

    #[test]
    fn extra_modifiers_block_hotkeys_in_strict_mode() {
        let binding = parse_hotkey_binding("f11").expect("hotkey should parse");
        assert!(!modifiers_match(&binding, false, false, true, false, true));
        assert!(!modifiers_match(&binding, true, true, true, true, true));
        assert!(modifiers_match(&binding, false, false, false, false, true));
    }
}
