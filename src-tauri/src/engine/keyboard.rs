#[cfg(target_os = "windows")]
use super::cycle::{execute_click_cycle, ClickCycleKind, ClickCyclePlan};
#[cfg(target_os = "windows")]
use super::worker::{sleep_interruptible, RunControl};
#[cfg(target_os = "windows")]
use super::AUTOCLICKER_EXTRA_INFO;

// ── Windows implementation ────────────────────────────────────────────────────

#[cfg(target_os = "windows")]
mod platform {
    use super::AUTOCLICKER_EXTRA_INFO;
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
        GetKeyState, MapVirtualKeyW, SendInput, INPUT, INPUT_KEYBOARD, KEYBDINPUT,
        KEYEVENTF_EXTENDEDKEY, KEYEVENTF_KEYUP, KEYEVENTF_SCANCODE, MAPVK_VK_TO_VSC_EX, VK_CAPITAL,
        VK_SHIFT,
    };

    #[inline]
    fn vk_to_scan(vk: u16) -> (u16, bool) {
        // MAPVK_VK_TO_VSC_EX returns the scan code in the low byte and, for
        // extended keys (arrows, Ins/Del/Home/End/PgUp/PgDn, numpad Enter, etc.),
        // a 0xE0/0xE1 prefix byte in the high byte. A non-zero high byte means
        // KEYEVENTF_EXTENDEDKEY must be set so apps that key off the extended
        // bit (or use raw input) see the correct key.
        let raw = unsafe { MapVirtualKeyW(vk as u32, MAPVK_VK_TO_VSC_EX) };
        ((raw & 0xFF) as u16, (raw >> 8) != 0)
    }

    #[inline]
    pub fn make_keyboard_input(vk: u16, flags: u32) -> INPUT {
        let (scan, extended) = vk_to_scan(vk);
        let ext_flag = if extended { KEYEVENTF_EXTENDEDKEY } else { 0 };
        INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: windows_sys::Win32::UI::Input::KeyboardAndMouse::INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: vk,
                    wScan: scan,
                    dwFlags: flags | KEYEVENTF_SCANCODE | ext_flag,
                    time: 0,
                    dwExtraInfo: AUTOCLICKER_EXTRA_INFO,
                },
            },
        }
    }

    #[inline]
    pub fn send_key_event(vk: u16, flags: u32) {
        let input = make_keyboard_input(vk, flags);
        unsafe { SendInput(1, &input, std::mem::size_of::<INPUT>() as i32) };
    }

    pub fn is_alphabetic_vk(vk: u16) -> bool {
        (b'A' as u16..=b'Z' as u16).contains(&vk)
    }

    fn caps_lock_enabled() -> bool {
        unsafe { (GetKeyState(VK_CAPITAL as i32) & 1) != 0 }
    }

    fn should_hold_shift_for_case(vk: u16, uppercase: bool) -> bool {
        is_alphabetic_vk(vk) && (caps_lock_enabled() != uppercase)
    }

    fn push_key_press(inputs: &mut Vec<INPUT>, vk: u16, use_shift: bool) {
        if use_shift {
            inputs.push(make_keyboard_input(VK_SHIFT, 0));
        }

        inputs.push(make_keyboard_input(vk, 0));
        inputs.push(make_keyboard_input(vk, KEYEVENTF_KEYUP));

        if use_shift {
            inputs.push(make_keyboard_input(VK_SHIFT, KEYEVENTF_KEYUP));
        }
    }

    fn send_key_down(vk: u16, use_shift: bool) {
        if use_shift {
            send_key_event(VK_SHIFT, 0);
        }
        send_key_event(vk, 0);
    }

    fn send_key_up(vk: u16, use_shift: bool) {
        send_key_event(vk, KEYEVENTF_KEYUP);
        if use_shift {
            send_key_event(VK_SHIFT, KEYEVENTF_KEYUP);
        }
    }

    pub fn send_key_batch(vk: u16, n: usize, uppercase: bool) {
        let use_shift = should_hold_shift_for_case(vk, uppercase);
        let inputs_per_press = if use_shift { 4 } else { 2 };
        let mut inputs: Vec<INPUT> = Vec::with_capacity(n * inputs_per_press);
        for _ in 0..n {
            push_key_press(&mut inputs, vk, use_shift);
        }
        unsafe {
            SendInput(
                inputs.len() as u32,
                inputs.as_ptr(),
                std::mem::size_of::<INPUT>() as i32,
            )
        };
    }

    pub fn send_key_presses(
        vk: u16,
        count: usize,
        uppercase: bool,
        plan: ClickCyclePlan,
        control: &RunControl,
        should_abort: &dyn Fn() -> bool,
    ) {
        if count == 0 {
            return;
        }

        if should_abort() {
            return;
        }

        if plan.kind == ClickCycleKind::Single && count > 1 && plan.first_hold_ms == 0 {
            send_key_batch(vk, count, uppercase);
            return;
        }

        let use_shift = should_hold_shift_for_case(vk, uppercase);
        let is_active = || control.is_active();
        let mut sleep_for = |duration| sleep_interruptible(duration, control);

        for _ in 0..count {
            if should_abort() {
                return;
            }
            if !execute_click_cycle(
                plan,
                &mut || send_key_down(vk, use_shift),
                &mut || send_key_up(vk, use_shift),
                &mut sleep_for,
                &is_active,
            ) {
                return;
            }
        }
    }
}

// ── macOS implementation ──────────────────────────────────────────────────────

#[cfg(target_os = "macos")]
mod platform {
    use super::super::cycle::{execute_click_cycle, ClickCycleKind, ClickCyclePlan};
    use super::super::worker::{sleep_interruptible, RunControl};
    use std::ffi::c_void;

    const CG_EVENT_TAP_HID: u32 = 1; // kCGSessionEventTap — session level (faster path)
    const CG_EVENT_SOURCE_STATE_HID: i32 = 1;

    // Standard macOS virtual key codes for modifier keys
    const VK_SHIFT: u16 = 56;

    #[link(name = "CoreGraphics", kind = "framework")]
    extern "C" {
        fn CGEventCreateKeyboardEvent(
            source: *mut c_void,
            virtual_key: u16,
            key_down: bool,
        ) -> *mut c_void;
        fn CGEventPost(tap: u32, event: *mut c_void);
        fn CGEventSourceCreate(state_id: i32) -> *mut c_void;
        fn CFRelease(cf: *mut c_void);
    }

    struct EventSource(*mut c_void);
    unsafe impl Send for EventSource {}
    unsafe impl Sync for EventSource {}

    fn event_source() -> *mut c_void {
        use std::sync::OnceLock;
        static SOURCE: OnceLock<EventSource> = OnceLock::new();
        SOURCE
            .get_or_init(|| EventSource(unsafe { CGEventSourceCreate(CG_EVENT_SOURCE_STATE_HID) }))
            .0
    }

    pub fn is_alphabetic_vk(vk: u16) -> bool {
        (vk < 26) || (b'A' as u16..=b'Z' as u16).contains(&vk)
    }

    fn win_vk_to_mac_vk(win_vk: u16) -> u16 {
        match win_vk {
            65..=90 => win_vk - 65,
            48..=57 => win_vk - 48 + 29,
            32 => 49,
            13 => 36,
            27 => 53,
            9 => 48,
            8 => 51,
            _ => win_vk,
        }
    }

    fn send_key_event(vk: u16, key_down: bool) {
        let mac_vk = win_vk_to_mac_vk(vk);
        unsafe {
            let event = CGEventCreateKeyboardEvent(event_source(), mac_vk, key_down);
            if !event.is_null() {
                CGEventPost(CG_EVENT_TAP_HID, event);
                CFRelease(event);
            }
        }
    }

    fn send_key_batch(vk: u16, n: usize, uppercase: bool) {
        let mac_vk = win_vk_to_mac_vk(vk);
        unsafe {
            let ev_down = CGEventCreateKeyboardEvent(event_source(), mac_vk, true);
            let ev_up = CGEventCreateKeyboardEvent(event_source(), mac_vk, false);
            let shift_down = if uppercase && is_alphabetic_vk(vk) {
                CGEventCreateKeyboardEvent(event_source(), VK_SHIFT, true)
            } else {
                std::ptr::null_mut()
            };
            let shift_up = if uppercase && is_alphabetic_vk(vk) {
                CGEventCreateKeyboardEvent(event_source(), VK_SHIFT, false)
            } else {
                std::ptr::null_mut()
            };
            if ev_down.is_null() || ev_up.is_null() {
                if !ev_down.is_null() {
                    CFRelease(ev_down);
                }
                if !ev_up.is_null() {
                    CFRelease(ev_up);
                }
                return;
            }
            for _ in 0..n {
                if uppercase && is_alphabetic_vk(vk) {
                    CGEventPost(CG_EVENT_TAP_HID, shift_down);
                }
                CGEventPost(CG_EVENT_TAP_HID, ev_down);
                CGEventPost(CG_EVENT_TAP_HID, ev_up);
                if uppercase && is_alphabetic_vk(vk) {
                    CGEventPost(CG_EVENT_TAP_HID, shift_up);
                }
            }
            CFRelease(ev_down);
            CFRelease(ev_up);
        }
    }

    pub fn send_key_presses(
        vk: u16,
        count: usize,
        uppercase: bool,
        plan: ClickCyclePlan,
        control: &RunControl,
        should_abort: &dyn Fn() -> bool,
    ) {
        if count == 0 {
            return;
        }
        if should_abort() {
            return;
        }
        if plan.kind == ClickCycleKind::Single && count > 1 && plan.first_hold_ms == 0 {
            send_key_batch(vk, count, uppercase);
            return;
        }
        let needs_shift = uppercase && is_alphabetic_vk(vk);
        let is_active = || control.is_active();
        let mut sleep_for = |duration| sleep_interruptible(duration, control);

        for _ in 0..count {
            if should_abort() {
                return;
            }
            if !execute_click_cycle(
                plan,
                &mut || {
                    if needs_shift {
                        send_key_event(VK_SHIFT, true);
                    }
                    send_key_event(vk, true);
                },
                &mut || {
                    send_key_event(vk, false);
                    if needs_shift {
                        send_key_event(VK_SHIFT, false);
                    }
                },
                &mut sleep_for,
                &is_active,
            ) {
                return;
            }
        }
    }
}

// ── Public API ─────────────────────────────────────────────────────────────────

pub use platform::{is_alphabetic_vk, send_key_presses};
