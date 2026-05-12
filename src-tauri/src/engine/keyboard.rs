// ── Windows implementation ────────────────────────────────────────────────────

#[cfg(target_os = "windows")]
mod platform {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
        GetKeyState, MapVirtualKeyW, SendInput, INPUT, INPUT_KEYBOARD, KEYBDINPUT,
        KEYEVENTF_EXTENDEDKEY, KEYEVENTF_KEYUP, KEYEVENTF_SCANCODE, MAPVK_VK_TO_VSC_EX, VK_CAPITAL,
        VK_SHIFT,
    };

    #[inline]
    fn vk_to_scan(vk: u16) -> (u16, bool) {
        let raw = unsafe { MapVirtualKeyW(vk as u32, MAPVK_VK_TO_VSC_EX) };
        ((raw & 0xFF) as u16, (raw >> 8) != 0)
    }

    #[inline]
    fn make_keyboard_input(vk: u16, flags: u32) -> INPUT {
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
                    dwExtraInfo: 0,
                },
            },
        }
    }

    #[inline]
    fn send_key_event(vk: u16, flags: u32) {
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

    fn send_key_batch(vk: u16, n: usize, uppercase: bool) {
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
        hold_ms: u32,
        uppercase: bool,
        use_double_press_gap: bool,
        double_press_delay_ms: u32,
        control: &RunControl,
    ) {
        if count == 0 {
            return;
        }

        if !use_double_press_gap && count > 1 && hold_ms == 0 {
            send_key_batch(vk, count, uppercase);
            return;
        }

        for index in 0..count {
            if !control.is_active() {
                return;
            }

            let use_shift = should_hold_shift_for_case(vk, uppercase);
            send_key_down(vk, use_shift);
            if hold_ms > 0 {
                sleep_interruptible(Duration::from_millis(hold_ms as u64), control);
            }
            send_key_up(vk, use_shift);

            if !control.is_active() {
                return;
            }

            if index + 1 < count && use_double_press_gap && double_press_delay_ms > 0 {
                sleep_interruptible(Duration::from_millis(double_press_delay_ms as u64), control);
            }
        }
    }
}

// ── macOS implementation ──────────────────────────────────────────────────────

#[cfg(target_os = "macos")]
mod platform {
    use super::super::worker::{sleep_interruptible, RunControl};
    use std::ffi::c_void;

    const CG_HID_EVENT_TAP: u32 = 1; // kCGSessionEventTap

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
        fn CFRelease(cf: *mut c_void);
    }

    pub fn is_alphabetic_vk(vk: u16) -> bool {
        // macOS virtual key codes for letters: A = 0, B = 11, ... Z = 6
        // But upstream uses Windows VK codes (A=65..Z=90).
        // We accept both macOS raw VK codes (0..25) and Windows VK codes (65..90).
        (vk < 26) || (b'A' as u16..=b'Z' as u16).contains(&vk)
    }

    /// Convert a Windows-style virtual key code to a macOS virtual key code.
    fn win_vk_to_mac_vk(win_vk: u16) -> u16 {
        match win_vk {
            // Letters: A-Z (Windows 65-90 -> macOS 0-25)
            65..=90 => win_vk - 65,
            // Digits: 0-9 (Windows 48-57 -> macOS 29-38)
            48..=57 => win_vk - 48 + 29,
            // Space
            32 => 49,
            // Enter/Return
            13 => 36,
            // Escape
            27 => 53,
            // Tab
            9 => 48,
            // Backspace/Delete
            8 => 51,
            // Fallback: use as-is (may already be a mac VK for modifiers etc.)
            _ => win_vk,
        }
    }

    fn send_key_event(vk: u16, key_down: bool) {
        let mac_vk = win_vk_to_mac_vk(vk);
        unsafe {
            let event = CGEventCreateKeyboardEvent(std::ptr::null_mut(), mac_vk, key_down);
            if !event.is_null() {
                CGEventPost(CG_HID_EVENT_TAP, event);
                CFRelease(event);
            }
        }
    }

    fn send_key_batch(vk: u16, n: usize, uppercase: bool) {
        let mac_vk = win_vk_to_mac_vk(vk);
        unsafe {
            let ev_down = CGEventCreateKeyboardEvent(std::ptr::null_mut(), mac_vk, true);
            let ev_up = CGEventCreateKeyboardEvent(std::ptr::null_mut(), mac_vk, false);
            let shift_down = if uppercase && is_alphabetic_vk(vk) {
                CGEventCreateKeyboardEvent(std::ptr::null_mut(), VK_SHIFT, true)
            } else {
                std::ptr::null_mut()
            };
            let shift_up = if uppercase && is_alphabetic_vk(vk) {
                CGEventCreateKeyboardEvent(std::ptr::null_mut(), VK_SHIFT, false)
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
                    CGEventPost(CG_HID_EVENT_TAP, shift_down);
                }
                CGEventPost(CG_HID_EVENT_TAP, ev_down);
                CGEventPost(CG_HID_EVENT_TAP, ev_up);
                if uppercase && is_alphabetic_vk(vk) {
                    CGEventPost(CG_HID_EVENT_TAP, shift_up);
                }
            }

            CFRelease(ev_down);
            CFRelease(ev_up);
        }
    }

    pub fn send_key_presses(
        vk: u16,
        count: usize,
        hold_ms: u32,
        uppercase: bool,
        use_double_press_gap: bool,
        double_press_delay_ms: u32,
        control: &RunControl,
    ) {
        if count == 0 {
            return;
        }

        let needs_shift = uppercase && is_alphabetic_vk(vk);

        if !use_double_press_gap && count > 1 && hold_ms == 0 {
            send_key_batch(vk, count, uppercase);
            return;
        }

        for index in 0..count {
            if !control.is_active() {
                return;
            }

            if needs_shift {
                send_key_event(VK_SHIFT, true);
            }
            send_key_event(vk, true);
            if hold_ms > 0 {
                sleep_interruptible(std::time::Duration::from_millis(hold_ms as u64), control);
            }
            send_key_event(vk, false);
            if needs_shift {
                send_key_event(VK_SHIFT, false);
            }

            if !control.is_active() {
                return;
            }

            if index + 1 < count && use_double_press_gap && double_press_delay_ms > 0 {
                sleep_interruptible(
                    std::time::Duration::from_millis(double_press_delay_ms as u64),
                    control,
                );
            }
        }
    }
}

// ── Public API ─────────────────────────────────────────────────────────────────

pub use platform::{is_alphabetic_vk, send_key_presses};
