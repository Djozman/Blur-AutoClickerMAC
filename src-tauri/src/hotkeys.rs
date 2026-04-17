//! macOS hotkey backend.
//! Uses Carbon GetKeys for key state polling and CGEventTap for scroll wheel.

use crate::engine::worker::now_epoch_ms;
use crate::engine::worker::start_clicker_inner;
use crate::engine::worker::stop_clicker_inner;
use crate::engine::worker::toggle_clicker_inner;
use crate::AppHandle;
use crate::ClickerState;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Duration;
use tauri::Manager;

pub const VK_SCROLL_UP_PSEUDO: i32 = -1;
pub const VK_SCROLL_DOWN_PSEUDO: i32 = -2;
pub const VK_NUMPAD_ENTER_PSEUDO: i32 = -3;

const SCROLL_WINDOW_MS: u64 = 200;

static SCROLL_UP_AT: AtomicU64 = AtomicU64::new(0);
static SCROLL_DOWN_AT: AtomicU64 = AtomicU64::new(0);
static NUMPAD_ENTER_DOWN: AtomicBool = AtomicBool::new(false);

#[link(name = "CoreGraphics", kind = "framework")]
extern "C" {
    fn CGEventTapCreate(
        tap: u32, place: u32, options: u32, events_of_interest: u64,
        callback: unsafe extern "C" fn(
            proxy: *mut std::ffi::c_void,
            type_: u32,
            event: *mut std::ffi::c_void,
            user_info: *mut std::ffi::c_void,
        ) -> *mut std::ffi::c_void,
        user_info: *mut std::ffi::c_void,
    ) -> *mut std::ffi::c_void;
    fn CGEventGetIntegerValueField(event: *mut std::ffi::c_void, field: i32) -> i64;
}

#[link(name = "CoreFoundation", kind = "framework")]
extern "C" {
    fn CFMachPortCreateRunLoopSource(
        allocator: *const std::ffi::c_void,
        tap: *mut std::ffi::c_void,
        order: isize,
    ) -> *mut std::ffi::c_void;
    fn CFRunLoopGetCurrent() -> *mut std::ffi::c_void;
    fn CFRunLoopAddSource(
        rl: *mut std::ffi::c_void,
        source: *mut std::ffi::c_void,
        mode: *const std::ffi::c_void,
    );
    fn CFRunLoopRun();
    static kCFRunLoopDefaultMode: *const std::ffi::c_void;
}

const K_CG_EVENT_SCROLL_WHEEL: u32 = 22;
const K_CG_SCROLL_WHEEL_EVENT_DELTA_AXIS_1: i32 = 11;
const K_CG_HID_EVENT_TAP: u32 = 0;
const K_CG_HEAD_INSERT_EVENT_TAP: u32 = 0;
const K_CG_EVENT_TAP_OPTION_LISTEN_ONLY: u32 = 1;

unsafe extern "C" fn scroll_event_callback(
    _proxy: *mut std::ffi::c_void,
    type_: u32,
    event: *mut std::ffi::c_void,
    _user_info: *mut std::ffi::c_void,
) -> *mut std::ffi::c_void {
    if type_ == K_CG_EVENT_SCROLL_WHEEL {
        let delta = CGEventGetIntegerValueField(event, K_CG_SCROLL_WHEEL_EVENT_DELTA_AXIS_1);
        let now = now_epoch_ms();
        if delta > 0 { SCROLL_UP_AT.store(now, Ordering::SeqCst); }
        else if delta < 0 { SCROLL_DOWN_AT.store(now, Ordering::SeqCst); }
    }
    event
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HotkeyBinding {
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    pub super_key: bool,
    pub main_vk: i32,
    pub key_token: String,
}

pub fn register_hotkey_inner(app: &AppHandle, hotkey: String) -> Result<String, String> {
    let binding = parse_hotkey_binding(&hotkey)?;
    let state = app.state::<ClickerState>();
    state.suppress_hotkey_until_ms.store(now_epoch_ms().saturating_add(250), Ordering::SeqCst);
    state.suppress_hotkey_until_release.store(true, Ordering::SeqCst);
    *state.registered_hotkey.lock().unwrap() = Some(binding.clone());
    Ok(format_hotkey_binding(&binding))
}

#[link(name = "Carbon", kind = "framework")]
extern "C" {
    fn GetKeys(key_map: *mut u8);
}

fn is_mac_keycode_down(keycode: u16) -> bool {
    let mut key_map = [0u8; 16];
    unsafe { GetKeys(key_map.as_mut_ptr()) };
    let byte = (keycode / 8) as usize;
    let bit = keycode % 8;
    if byte >= 16 { return false; }
    (key_map[byte] >> bit) & 1 != 0
}

fn vk_to_mac_keycode(vk: i32) -> Option<u16> {
    const MAC_KEYCODES: &[(i32, u16)] = &[
        (b'A' as i32, 0x00), (b'B' as i32, 0x0B), (b'C' as i32, 0x08),
        (b'D' as i32, 0x02), (b'E' as i32, 0x0E), (b'F' as i32, 0x03),
        (b'G' as i32, 0x05), (b'H' as i32, 0x04), (b'I' as i32, 0x22),
        (b'J' as i32, 0x26), (b'K' as i32, 0x28), (b'L' as i32, 0x25),
        (b'M' as i32, 0x2E), (b'N' as i32, 0x2D), (b'O' as i32, 0x1F),
        (b'P' as i32, 0x23), (b'Q' as i32, 0x0C), (b'R' as i32, 0x0F),
        (b'S' as i32, 0x01), (b'T' as i32, 0x11), (b'U' as i32, 0x20),
        (b'V' as i32, 0x09), (b'W' as i32, 0x0D), (b'X' as i32, 0x07),
        (b'Y' as i32, 0x10), (b'Z' as i32, 0x06),
        (b'0' as i32, 0x1D), (b'1' as i32, 0x12), (b'2' as i32, 0x13),
        (b'3' as i32, 0x14), (b'4' as i32, 0x15), (b'5' as i32, 0x17),
        (b'6' as i32, 0x16), (b'7' as i32, 0x1A), (b'8' as i32, 0x1C),
        (b'9' as i32, 0x19),
        (0x70, 0x7A), (0x71, 0x78), (0x72, 0x63), (0x73, 0x76),
        (0x74, 0x60), (0x75, 0x61), (0x76, 0x62), (0x77, 0x64),
        (0x78, 0x65), (0x79, 0x6D), (0x7A, 0x67), (0x7B, 0x6F),
        (0x1B, 0x35), (0x0D, 0x24), (0x20, 0x31), (0x09, 0x30),
        (0x08, 0x33), (0x2E, 0x75), (0x23, 0x73), (0x22, 0x77),
        (0x21, 0x74), (0x26, 0x7E), (0x28, 0x7D), (0x25, 0x7B),
        (0x27, 0x7C),
        (0x60, 0x52), (0x61, 0x53), (0x62, 0x54), (0x63, 0x55),
        (0x64, 0x56), (0x65, 0x57), (0x66, 0x58), (0x67, 0x59),
        (0x68, 0x5B), (0x69, 0x5C), (0x6B, 0x4C),
        (0x11, 0x3B), (0x12, 0x3A), (0x10, 0x38),
        (0x5B, 0x37), (0x5C, 0x36),
    ];
    MAC_KEYCODES.iter().find(|&&(w, _)| w == vk).map(|&(_, m)| m)
}

pub fn is_vk_down(vk: i32) -> bool {
    vk_to_mac_keycode(vk).map(is_mac_keycode_down).unwrap_or(false)
}

fn is_modifier_down_mac(vk: i32) -> bool {
    match vk {
        0x11 => is_mac_keycode_down(0x3B) || is_mac_keycode_down(0x3E),
        0x10 => is_mac_keycode_down(0x38) || is_mac_keycode_down(0x3C),
        0x12 => is_mac_keycode_down(0x3A) || is_mac_keycode_down(0x3D),
        0x5B | 0x5C => is_mac_keycode_down(0x37) || is_mac_keycode_down(0x36),
        _ => is_vk_down(vk),
    }
}

fn is_main_key_active(vk: i32) -> bool {
    match vk {
        VK_SCROLL_UP_PSEUDO => {
            let ts = SCROLL_UP_AT.load(Ordering::SeqCst);
            ts != 0 && now_epoch_ms().saturating_sub(ts) < SCROLL_WINDOW_MS
        }
        VK_SCROLL_DOWN_PSEUDO => {
            let ts = SCROLL_DOWN_AT.load(Ordering::SeqCst);
            ts != 0 && now_epoch_ms().saturating_sub(ts) < SCROLL_WINDOW_MS
        }
        VK_NUMPAD_ENTER_PSEUDO => NUMPAD_ENTER_DOWN.load(Ordering::SeqCst),
        _ => is_vk_down(vk),
    }
}

pub fn is_hotkey_binding_pressed(binding: &HotkeyBinding, strict: bool) -> bool {
    let ctrl_down  = is_modifier_down_mac(0x11);
    let alt_down   = is_modifier_down_mac(0x12);
    let shift_down = is_modifier_down_mac(0x10);
    let super_down = is_modifier_down_mac(0x5B);
    if !modifiers_match(binding, ctrl_down, alt_down, shift_down, super_down, strict) {
        return false;
    }
    is_main_key_active(binding.main_vk)
}

fn modifiers_match(
    binding: &HotkeyBinding,
    ctrl_down: bool, alt_down: bool, shift_down: bool, super_down: bool,
    strict: bool,
) -> bool {
    if binding.ctrl && !ctrl_down { return false; }
    if binding.alt && !alt_down { return false; }
    if binding.shift && !shift_down { return false; }
    if binding.super_key && !super_down { return false; }
    if strict {
        if ctrl_down && !binding.ctrl { return false; }
        if alt_down && !binding.alt { return false; }
        if shift_down && !binding.shift { return false; }
        if super_down && !binding.super_key { return false; }
    }
    true
}

pub fn start_scroll_hook() {
    std::thread::spawn(|| unsafe {
        let mask: u64 = 1u64 << K_CG_EVENT_SCROLL_WHEEL;
        let tap = CGEventTapCreate(
            K_CG_HID_EVENT_TAP, K_CG_HEAD_INSERT_EVENT_TAP,
            K_CG_EVENT_TAP_OPTION_LISTEN_ONLY, mask,
            scroll_event_callback, std::ptr::null_mut(),
        );
        if tap.is_null() {
            log::warn!("[Hotkeys] CGEventTap failed — grant Accessibility permission in System Settings.");
            return;
        }
        let source = CFMachPortCreateRunLoopSource(std::ptr::null(), tap, 0);
        let rl = CFRunLoopGetCurrent();
        CFRunLoopAddSource(rl, source, kCFRunLoopDefaultMode);
        CFRunLoopRun();
    });
}

pub fn start_hotkey_listener(app: AppHandle) {
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
                .map(|b| is_hotkey_binding_pressed(b, strict))
                .unwrap_or(false);
            let suppress_until = app.state::<ClickerState>().suppress_hotkey_until_ms.load(Ordering::SeqCst);
            let suppress_until_release = app.state::<ClickerState>().suppress_hotkey_until_release.load(Ordering::SeqCst);
            let hotkey_capture_active = app.state::<ClickerState>().hotkey_capture_active.load(Ordering::SeqCst);
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
                app.state::<ClickerState>().suppress_hotkey_until_release.store(false, Ordering::SeqCst);
                was_pressed = false;
                std::thread::sleep(Duration::from_millis(12));
                continue;
            }
            if now_epoch_ms() < suppress_until {
                was_pressed = currently_pressed;
                std::thread::sleep(Duration::from_millis(12));
                continue;
            }
            if currently_pressed && !was_pressed { handle_hotkey_pressed(&app); }
            else if !currently_pressed && was_pressed { handle_hotkey_released(&app); }
            was_pressed = currently_pressed;
            std::thread::sleep(Duration::from_millis(12));
        }
    });
}

pub fn handle_hotkey_pressed(app: &AppHandle) {
    let mode = app.state::<ClickerState>().settings.lock().unwrap().mode.clone();
    if mode == "Toggle" { let _ = toggle_clicker_inner(app); }
    else if mode == "Hold" { let _ = start_clicker_inner(app); }
}

pub fn handle_hotkey_released(app: &AppHandle) {
    let mode = app.state::<ClickerState>().settings.lock().unwrap().mode.clone();
    if mode == "Hold" { let _ = stop_clicker_inner(app, Some(String::from("Stopped from hold hotkey"))); }
}

pub fn normalize_hotkey(value: &str) -> String {
    value.trim().to_lowercase()
        .replace("control", "ctrl")
        .replace("command", "super")
        .replace("meta", "super")
        .replace("win", "super")
}

pub fn parse_hotkey_binding(hotkey: &str) -> Result<HotkeyBinding, String> {
    let normalized = normalize_hotkey(hotkey);
    let mut ctrl = false; let mut alt = false;
    let mut shift = false; let mut super_key = false;
    let mut main_key: Option<(i32, String)> = None;
    for token in normalized.split('+').map(str::trim) {
        if token.is_empty() { return Err(format!("Invalid hotkey '{hotkey}': found empty key token")); }
        match token {
            "alt" | "option" => alt = true,
            "ctrl" | "control" => ctrl = true,
            "shift" => shift = true,
            "super" | "command" | "cmd" | "meta" | "win" => super_key = true,
            _ => {
                if main_key.replace(parse_hotkey_main_key(token, hotkey)?).is_some() {
                    return Err(format!("Invalid hotkey '{hotkey}': use modifiers first and only one main key"));
                }
            }
        }
    }
    let (main_vk, key_token) = main_key.ok_or_else(|| format!("Invalid hotkey '{hotkey}': missing main key"))?;
    Ok(HotkeyBinding { ctrl, alt, shift, super_key, main_vk, key_token })
}

pub fn parse_hotkey_main_key(token: &str, original_hotkey: &str) -> Result<(i32, String), String> {
    let lower = token.trim().to_lowercase();
    let mapped = match lower.as_str() {
        "mouseleft" | "mouse1" => Some((1, String::from("mouseleft"))),
        "mouseright" | "mouse2" => Some((2, String::from("mouseright"))),
        "mousemiddle" | "mouse3" => Some((4, String::from("mousemiddle"))),
        "scrollup" | "wheelup" => Some((VK_SCROLL_UP_PSEUDO, String::from("scrollup"))),
        "scrolldown" | "wheeldown" => Some((VK_SCROLL_DOWN_PSEUDO, String::from("scrolldown"))),
        "numpadenter" => Some((VK_NUMPAD_ENTER_PSEUDO, String::from("numpadenter"))),
        "numpad0" => Some((0x60, String::from("numpad0"))),
        "numpad1" => Some((0x61, String::from("numpad1"))),
        "numpad2" => Some((0x62, String::from("numpad2"))),
        "numpad3" => Some((0x63, String::from("numpad3"))),
        "numpad4" => Some((0x64, String::from("numpad4"))),
        "numpad5" => Some((0x65, String::from("numpad5"))),
        "numpad6" => Some((0x66, String::from("numpad6"))),
        "numpad7" => Some((0x67, String::from("numpad7"))),
        "numpad8" => Some((0x68, String::from("numpad8"))),
        "numpad9" => Some((0x69, String::from("numpad9"))),
        "space" | "spacebar" => Some((0x20, String::from("space"))),
        "tab" => Some((0x09, String::from("tab"))),
        "enter" => Some((0x0D, String::from("enter"))),
        "backspace" => Some((0x08, String::from("backspace"))),
        "delete" => Some((0x2E, String::from("delete"))),
        "home" => Some((0x23, String::from("home"))),
        "end" => Some((0x22, String::from("end"))),
        "pageup" => Some((0x21, String::from("pageup"))),
        "pagedown" => Some((0x22, String::from("pagedown"))),
        "up" => Some((0x26, String::from("up"))),
        "down" => Some((0x28, String::from("down"))),
        "left" => Some((0x25, String::from("left"))),
        "right" => Some((0x27, String::from("right"))),
        "esc" | "escape" => Some((0x1B, String::from("escape"))),
        _ => None,
    };
    if let Some(b) = mapped { return Ok(b); }
    if lower.starts_with('f') && lower.len() <= 3 {
        if let Ok(n) = lower[1..].parse::<i32>() {
            let vk = if (1..=12).contains(&n) { 0x70 + (n - 1) } else { -1 };
            if vk >= 0 { return Ok((vk, lower)); }
        }
    }
    if let Some(l) = lower.strip_prefix("key") { if l.len() == 1 { return parse_hotkey_main_key(l, original_hotkey); } }
    if let Some(d) = lower.strip_prefix("digit") { if d.len() == 1 { return parse_hotkey_main_key(d, original_hotkey); } }
    if lower.len() == 1 {
        let ch = lower.as_bytes()[0];
        if ch.is_ascii_lowercase() { return Ok((ch.to_ascii_uppercase() as i32, lower)); }
        if ch.is_ascii_digit() { return Ok((ch as i32, lower)); }
    }
    Err(format!("Couldn't recognize '{token}' as a valid key in '{original_hotkey}'"))
}

pub fn format_hotkey_binding(binding: &HotkeyBinding) -> String {
    let mut parts: Vec<String> = Vec::new();
    if binding.ctrl { parts.push(String::from("ctrl")); }
    if binding.alt { parts.push(String::from("alt")); }
    if binding.shift { parts.push(String::from("shift")); }
    if binding.super_key { parts.push(String::from("super")); }
    parts.push(binding.key_token.clone());
    parts.join("+")
}

#[cfg(test)]
mod tests {
    use super::{format_hotkey_binding, modifiers_match, parse_hotkey_binding};
    #[test]
    fn numpad_tokens_round_trip() {
        for token in ["numpad0","numpad9","numpadenter","scrollup","scrolldown"] {
            let hotkey = format!("ctrl+shift+{token}");
            let binding = parse_hotkey_binding(&hotkey).expect("token should parse");
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
        let binding = parse_hotkey_binding("f11").expect("should parse");
        assert!(modifiers_match(&binding, false, false, true, false, false));
    }
    #[test]
    fn extra_modifiers_block_hotkeys_in_strict_mode() {
        let binding = parse_hotkey_binding("f11").expect("should parse");
        assert!(!modifiers_match(&binding, false, false, true, false, true));
        assert!(modifiers_match(&binding, false, false, false, false, true));
    }
}
