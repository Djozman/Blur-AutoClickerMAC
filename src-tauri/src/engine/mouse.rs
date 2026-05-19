use std::time::Duration;

use super::rng::SmallRng;
use super::worker::{sleep_interruptible, RunControl};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VirtualScreenRect {
    pub left: i32,
    pub top: i32,
    pub width: i32,
    pub height: i32,
}

impl VirtualScreenRect {
    #[inline]
    pub fn new(left: i32, top: i32, width: i32, height: i32) -> Self {
        Self {
            left,
            top,
            width,
            height,
        }
    }

    #[inline]
    pub fn right(self) -> i32 {
        self.left + self.width
    }

    #[inline]
    pub fn bottom(self) -> i32 {
        self.top + self.height
    }

    #[inline]
    pub fn contains(self, x: i32, y: i32) -> bool {
        x >= self.left && x < self.right() && y >= self.top && y < self.bottom()
    }

    #[inline]
    pub fn offset_from(self, origin: VirtualScreenRect) -> Self {
        Self::new(
            self.left - origin.left,
            self.top - origin.top,
            self.width,
            self.height,
        )
    }

    #[allow(dead_code)]
    fn normalize_x(&self, pixel_x: i32) -> i32 {
        let relative_x = pixel_x as f64 - self.left as f64;
        let ratio = relative_x / self.width as f64;
        (ratio * 65535.0).round() as i32
    }

    #[allow(dead_code)]
    fn normalize_y(&self, pixel_y: i32) -> i32 {
        let relative_y = pixel_y as f64 - self.top as f64;
        let ratio = relative_y / self.height as f64;
        (ratio * 65535.0).round() as i32
    }
}

// ── Windows implementation ────────────────────────────────────────────────────

#[cfg(target_os = "windows")]
mod platform {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
        SendInput, INPUT, INPUT_MOUSE, MOUSEEVENTF_ABSOLUTE, MOUSEEVENTF_LEFTDOWN,
        MOUSEEVENTF_LEFTUP, MOUSEEVENTF_MIDDLEDOWN, MOUSEEVENTF_MIDDLEUP, MOUSEEVENTF_MOVE,
        MOUSEEVENTF_RIGHTDOWN, MOUSEEVENTF_RIGHTUP, MOUSEEVENTF_VIRTUALDESK, MOUSEINPUT,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GetSystemMetrics, SM_CXVIRTUALSCREEN, SM_CYVIRTUALSCREEN, SM_XVIRTUALSCREEN,
        SM_YVIRTUALSCREEN,
    };

    use super::VirtualScreenRect;

    pub const MOUSE_LEFT_DOWN: u32 = MOUSEEVENTF_LEFTDOWN;
    pub const MOUSE_LEFT_UP: u32 = MOUSEEVENTF_LEFTUP;
    pub const MOUSE_RIGHT_DOWN: u32 = MOUSEEVENTF_RIGHTDOWN;
    pub const MOUSE_RIGHT_UP: u32 = MOUSEEVENTF_RIGHTUP;
    pub const MOUSE_MIDDLE_DOWN: u32 = MOUSEEVENTF_MIDDLEDOWN;
    pub const MOUSE_MIDDLE_UP: u32 = MOUSEEVENTF_MIDDLEUP;

    pub fn current_cursor_position() -> Option<(i32, i32)> {
        use windows_sys::Win32::Foundation::POINT;
        use windows_sys::Win32::UI::WindowsAndMessaging::GetCursorPos;

        let mut point = POINT { x: 0, y: 0 };
        let ok = unsafe { GetCursorPos(&mut point) };
        if ok == 0 {
            None
        } else {
            Some((point.x, point.y))
        }
    }

    pub fn current_virtual_screen_rect() -> Option<VirtualScreenRect> {
        let left = unsafe { GetSystemMetrics(SM_XVIRTUALSCREEN) };
        let top = unsafe { GetSystemMetrics(SM_YVIRTUALSCREEN) };
        let width = unsafe { GetSystemMetrics(SM_CXVIRTUALSCREEN) };
        let height = unsafe { GetSystemMetrics(SM_CYVIRTUALSCREEN) };
        if width <= 0 || height <= 0 {
            return None;
        }
        Some(VirtualScreenRect::new(left, top, width, height))
    }

    pub fn current_monitor_rects() -> Option<Vec<VirtualScreenRect>> {
        use std::ptr;
        use windows_sys::Win32::Foundation::RECT;
        use windows_sys::Win32::Graphics::Gdi::{
            EnumDisplayMonitors, GetMonitorInfoW, MONITORINFO,
        };

        unsafe extern "system" fn enum_monitor_proc(
            monitor: isize,
            _hdc: isize,
            _clip_rect: *mut RECT,
            user_data: isize,
        ) -> i32 {
            let monitors = &mut *(user_data as *mut Vec<VirtualScreenRect>);
            let mut info = std::mem::zeroed::<MONITORINFO>();
            info.cbSize = std::mem::size_of::<MONITORINFO>() as u32;
            if GetMonitorInfoW(monitor, &mut info as *mut MONITORINFO as *mut _) == 0 {
                return 1;
            }
            let rect = info.rcMonitor;
            let width = rect.right - rect.left;
            let height = rect.bottom - rect.top;
            if width > 0 && height > 0 {
                monitors.push(VirtualScreenRect::new(rect.left, rect.top, width, height));
            }
            1
        }

        let mut monitors = Vec::new();
        let ok = unsafe {
            EnumDisplayMonitors(
                0,
                ptr::null(),
                Some(enum_monitor_proc),
                &mut monitors as *mut Vec<VirtualScreenRect> as isize,
            )
        };
        if ok == 0 || monitors.is_empty() {
            return current_virtual_screen_rect().map(|screen| vec![screen]);
        }
        monitors.sort_by_key(|m: &VirtualScreenRect| (m.top, m.left));
        Some(monitors)
    }

    pub fn move_mouse(target_x: i32, target_y: i32) {
        if let Some(screen_rect) = current_virtual_screen_rect() {
            let end_x = screen_rect.normalize_x(target_x);
            let end_y = screen_rect.normalize_y(target_y);
            let movement = make_movement(end_x, end_y);
            unsafe { SendInput(1, &movement, std::mem::size_of::<INPUT>() as i32) };
        }
    }

    fn make_movement(end_x: i32, end_y: i32) -> INPUT {
        INPUT {
            r#type: INPUT_MOUSE,
            Anonymous: windows_sys::Win32::UI::Input::KeyboardAndMouse::INPUT_0 {
                mi: MOUSEINPUT {
                    dx: end_x,
                    dy: end_y,
                    mouseData: 0,
                    dwFlags: MOUSEEVENTF_ABSOLUTE | MOUSEEVENTF_MOVE | MOUSEEVENTF_VIRTUALDESK,
                    time: 0,
                    dwExtraInfo: 0,
                },
            },
        }
    }

    fn make_input(flags: u32, time: u32) -> INPUT {
        INPUT {
            r#type: INPUT_MOUSE,
            Anonymous: windows_sys::Win32::UI::Input::KeyboardAndMouse::INPUT_0 {
                mi: MOUSEINPUT {
                    dx: 0,
                    dy: 0,
                    mouseData: 0,
                    dwFlags: flags,
                    time,
                    dwExtraInfo: 0,
                },
            },
        }
    }

    pub fn send_mouse_event(flags: u32) {
        let input = make_input(flags, 0);
        unsafe { SendInput(1, &input, std::mem::size_of::<INPUT>() as i32) };
    }

    pub fn send_batch(down: u32, up: u32, n: usize, _hold_ms: u32) {
        let mut inputs: Vec<INPUT> = Vec::with_capacity(n * 2);
        for _ in 0..n {
            inputs.push(make_input(down, 0));
            inputs.push(make_input(up, 0));
        }
        unsafe {
            SendInput(
                inputs.len() as u32,
                inputs.as_ptr(),
                std::mem::size_of::<INPUT>() as i32,
            )
        };
    }
}

// ── macOS implementation ──────────────────────────────────────────────────────

#[cfg(target_os = "macos")]
mod platform {
    use super::VirtualScreenRect;
    use std::ffi::c_void;

    // The u32 "flags" used throughout this module encode CGEventType values:
    pub const MOUSE_LEFT_DOWN: u32 = 1; // kCGEventLeftMouseDown
    pub const MOUSE_LEFT_UP: u32 = 2; // kCGEventLeftMouseUp
    pub const MOUSE_RIGHT_DOWN: u32 = 3; // kCGEventRightMouseDown
    pub const MOUSE_RIGHT_UP: u32 = 4; // kCGEventRightMouseUp
    pub const MOUSE_MIDDLE_DOWN: u32 = 25; // kCGEventOtherMouseDown
    pub const MOUSE_MIDDLE_UP: u32 = 26; // kCGEventOtherMouseUp

    const CG_MOUSE_BUTTON_LEFT: u32 = 0;
    const CG_MOUSE_BUTTON_RIGHT: u32 = 1;
    const CG_MOUSE_BUTTON_CENTER: u32 = 2;
    const CG_EVENT_TAP_HID: u32 = 0; // kCGHIDEventTap – lowest level, treated as hardware input
    const CG_EVENT_MOUSE_MOVED: u32 = 5;
    const CG_EVENT_SOURCE_STATE_HID: i32 = 1; // kCGEventSourceStateHIDSystemState

    #[repr(C)]
    #[derive(Clone, Copy)]
    pub struct CGPoint {
        pub x: f64,
        pub y: f64,
    }

    #[repr(C)]
    struct CGSize {
        width: f64,
        height: f64,
    }

    #[repr(C)]
    struct CGRect {
        origin: CGPoint,
        size: CGSize,
    }

    #[link(name = "CoreGraphics", kind = "framework")]
    extern "C" {
        fn CGEventCreate(source: *mut c_void) -> *mut c_void;
        fn CGEventGetLocation(event: *mut c_void) -> CGPoint;
        fn CGEventCreateMouseEvent(
            source: *mut c_void,
            mouse_type: u32,
            mouse_cursor_position: CGPoint,
            mouse_button: u32,
        ) -> *mut c_void;
        fn CGEventSetFlags(event: *mut c_void, flags: u64);
        fn CGEventPost(tap: u32, event: *mut c_void);
        fn CGEventSourceCreate(state_id: i32) -> *mut c_void;
        fn CGDisplayBounds(display: u32) -> CGRect;
        fn CGMainDisplayID() -> u32;
        fn CGGetActiveDisplayList(
            max_displays: u32,
            active_displays: *mut u32,
            display_count: *mut u32,
        ) -> u32;
        fn CGDisplayMoveCursorToPoint(display: u32, point: CGPoint);
        fn CFRelease(cf: *mut c_void);
    }

    // mach_absolute_time – always linked via libSystem
    extern "C" {
        fn mach_absolute_time() -> u64;
    }

    /// Wrapper around a CGEventSourceRef that is safe to share across threads.
    struct EventSource(*mut c_void);
    unsafe impl Send for EventSource {}
    unsafe impl Sync for EventSource {}

    /// Lazily create (once) and return a HID-system-state event source.
    fn event_source() -> *mut c_void {
        use std::sync::OnceLock;
        static SOURCE: OnceLock<EventSource> = OnceLock::new();
        SOURCE
            .get_or_init(|| EventSource(unsafe { CGEventSourceCreate(CG_EVENT_SOURCE_STATE_HID) }))
            .0
    }

    fn get_cursor_point() -> CGPoint {
        unsafe {
            let event = CGEventCreate(std::ptr::null_mut());
            if event.is_null() {
                return CGPoint { x: 0.0, y: 0.0 };
            }
            let point = CGEventGetLocation(event);
            CFRelease(event);
            point
        }
    }

    pub fn current_cursor_position() -> Option<(i32, i32)> {
        let p = get_cursor_point();
        Some((p.x as i32, p.y as i32))
    }

    pub fn current_virtual_screen_rect() -> Option<VirtualScreenRect> {
        unsafe {
            let display = CGMainDisplayID();
            let bounds = CGDisplayBounds(display);
            let width = bounds.size.width as i32;
            let height = bounds.size.height as i32;
            if width <= 0 || height <= 0 {
                return None;
            }
            Some(VirtualScreenRect::new(
                bounds.origin.x as i32,
                bounds.origin.y as i32,
                width,
                height,
            ))
        }
    }

    pub fn current_monitor_rects() -> Option<Vec<VirtualScreenRect>> {
        unsafe {
            let mut displays = [0u32; 16];
            let mut count = 0u32;
            let err = CGGetActiveDisplayList(16, displays.as_mut_ptr(), &mut count);
            if err != 0 || count == 0 {
                return current_virtual_screen_rect().map(|s| vec![s]);
            }
            let mut rects: Vec<VirtualScreenRect> = displays[..count as usize]
                .iter()
                .filter_map(|&display| {
                    let bounds = CGDisplayBounds(display);
                    let width = bounds.size.width as i32;
                    let height = bounds.size.height as i32;
                    if width > 0 && height > 0 {
                        Some(VirtualScreenRect::new(
                            bounds.origin.x as i32,
                            bounds.origin.y as i32,
                            width,
                            height,
                        ))
                    } else {
                        None
                    }
                })
                .collect();
            rects.sort_by_key(|r: &VirtualScreenRect| (r.top, r.left));
            Some(rects)
        }
    }

    // move_mouse now receives absolute coordinates.
    pub fn move_mouse(target_x: i32, target_y: i32) {
        unsafe {
            let point = CGPoint {
                x: target_x as f64,
                y: target_y as f64,
            };
            let display = CGMainDisplayID();
            CGDisplayMoveCursorToPoint(display, point);
        }
    }

    pub fn send_mouse_event(event_type: u32) {
        let pos = get_cursor_point();
        let mouse_button = match event_type {
            1 | 2 => CG_MOUSE_BUTTON_LEFT,
            3 | 4 => CG_MOUSE_BUTTON_RIGHT,
            _ => CG_MOUSE_BUTTON_CENTER,
        };
        unsafe {
            let source = event_source();
            let event = CGEventCreateMouseEvent(source, event_type, pos, mouse_button);
            if !event.is_null() {
                CGEventSetFlags(event, 0);
                CGEventPost(CG_EVENT_TAP_HID, event);
                CFRelease(event);
            }
        }
    }

    pub fn send_batch(down: u32, up: u32, n: usize, _hold_ms: u32) {
        let pos = get_cursor_point();
        send_batch_at(pos, down, up, n);
    }

    pub fn send_batch_at(pos: CGPoint, down: u32, up: u32, n: usize) {
        let mouse_button = match down {
            1 => CG_MOUSE_BUTTON_LEFT,
            3 => CG_MOUSE_BUTTON_RIGHT,
            _ => CG_MOUSE_BUTTON_CENTER,
        };
        unsafe {
            let ev_down = CGEventCreateMouseEvent(event_source(), down, pos, mouse_button);
            let ev_up = CGEventCreateMouseEvent(event_source(), up, pos, mouse_button);
            if ev_down.is_null() || ev_up.is_null() {
                if !ev_down.is_null() {
                    CFRelease(ev_down);
                }
                if !ev_up.is_null() {
                    CFRelease(ev_up);
                }
                return;
            }
            CGEventSetFlags(ev_down, 0);
            CGEventSetFlags(ev_up, 0);
            for _ in 0..n {
                CGEventPost(CG_EVENT_TAP_HID, ev_down);
                CGEventPost(CG_EVENT_TAP_HID, ev_up);
            }
            CFRelease(ev_down);
            CFRelease(ev_up);
        }
    }

    // Used by smooth_move to post a mouse-moved event after CGDisplayMoveCursorToPoint
    pub fn post_mouse_moved(x: i32, y: i32) {
        let point = CGPoint {
            x: x as f64,
            y: y as f64,
        };
        unsafe {
            let event = CGEventCreateMouseEvent(
                event_source(),
                CG_EVENT_MOUSE_MOVED,
                point,
                CG_MOUSE_BUTTON_LEFT,
            );
            if !event.is_null() {
                CGEventSetFlags(event, 0);
                CGEventPost(CG_EVENT_TAP_HID, event);
                CFRelease(event);
            }
        }
    }
}

// ── Public API (delegates to platform module) ─────────────────────────────────

pub use platform::{
    MOUSE_LEFT_DOWN, MOUSE_LEFT_UP, MOUSE_MIDDLE_DOWN, MOUSE_MIDDLE_UP, MOUSE_RIGHT_DOWN,
    MOUSE_RIGHT_UP,
};

pub fn current_cursor_position() -> Option<(i32, i32)> {
    platform::current_cursor_position()
}

pub fn current_virtual_screen_rect() -> Option<VirtualScreenRect> {
    platform::current_virtual_screen_rect()
}

pub fn current_monitor_rects() -> Option<Vec<VirtualScreenRect>> {
    platform::current_monitor_rects()
}

#[inline]
pub fn get_cursor_pos() -> (i32, i32) {
    current_cursor_position().unwrap_or((0, 0))
}

#[inline]
pub fn move_mouse(x: i32, y: i32) {
    platform::move_mouse(x, y);
}

#[inline]
pub fn send_mouse_event(flags: u32) {
    platform::send_mouse_event(flags);
}

pub fn send_batch(down: u32, up: u32, n: usize, hold_ms: u32) {
    platform::send_batch(down, up, n, hold_ms);
}

fn dispatch_click<FSend, FSleep, FActive>(
    down: u32,
    up: u32,
    hold_ms: u32,
    send_event: &mut FSend,
    sleep_for: &mut FSleep,
    is_active: &FActive,
) -> bool
where
    FSend: FnMut(u32),
    FSleep: FnMut(Duration),
    FActive: Fn() -> bool,
{
    if !is_active() {
        return false;
    }

    send_event(down);
    if hold_ms > 0 {
        sleep_for(Duration::from_millis(hold_ms as u64));
        if !is_active() {
            send_event(up);
            return false;
        }
    }

    send_event(up);
    true
}

pub fn send_clicks_at(
    down: u32,
    up: u32,
    count: usize,
    hold_ms: u32,
    use_double_click_gap: bool,
    double_click_delay_ms: u32,
    control: &RunControl,
    cursor_pos: Option<(i32, i32)>,
) {
    if count == 0 {
        return;
    }

    if !use_double_click_gap && hold_ms == 0 {
        #[cfg(target_os = "macos")]
        if let Some((x, y)) = cursor_pos {
            platform::send_batch_at(
                platform::CGPoint {
                    x: x as f64,
                    y: y as f64,
                },
                down,
                up,
                count,
            );
            return;
        }
        send_batch(down, up, count, hold_ms);
        return;
    }

    let is_active = || control.is_active();
    let mut send_event = |flags| send_mouse_event(flags);
    let mut sleep_for = |duration| sleep_interruptible(duration, control);

    for index in 0..count {
        if !dispatch_click(
            down,
            up,
            hold_ms,
            &mut send_event,
            &mut sleep_for,
            &is_active,
        ) {
            return;
        }

        if index + 1 < count && use_double_click_gap && double_click_delay_ms > 0 {
            sleep_interruptible(Duration::from_millis(double_click_delay_ms as u64), control);
        }
    }
}

#[inline]
pub fn get_button_flags(button: i32) -> (u32, u32) {
    match button {
        2 => (MOUSE_RIGHT_DOWN, MOUSE_RIGHT_UP),
        3 => (MOUSE_MIDDLE_DOWN, MOUSE_MIDDLE_UP),
        _ => (MOUSE_LEFT_DOWN, MOUSE_LEFT_UP),
    }
}

#[inline]
pub fn ease_in_out_quad(t: f64) -> f64 {
    if t < 0.5 {
        2.0 * t * t
    } else {
        1.0 - (-2.0 * t + 2.0).powi(2) / 2.0
    }
}

#[inline]
pub fn cubic_bezier(t: f64, p0: f64, p1: f64, p2: f64, p3: f64) -> f64 {
    let u = 1.0 - t;
    u * u * u * p0 + 3.0 * u * u * t * p1 + 3.0 * u * t * t * p2 + t * t * t * p3
}

fn smooth_move_inner(
    start_x: i32,
    start_y: i32,
    end_x: i32,
    end_y: i32,
    duration_ms: u64,
    rng: &mut SmallRng,
    allow_overshoot: bool,
) {
    if duration_ms < 3 || (start_x == end_x && start_y == end_y) {
        move_mouse(end_x, end_y);
        return;
    }

    let (start_x_f, start_y_f) = (start_x as f64, start_y as f64);
    let (target_x_f, target_y_f) = (end_x as f64, end_y as f64);
    let delta_x = target_x_f - start_x_f;
    let delta_y = target_y_f - start_y_f;
    let distance = delta_x.hypot(delta_y);

    if distance < 3.0 {
        move_mouse(end_x, end_y);
        return;
    }

    let steps = if duration_ms <= 12 {
        (duration_ms / 3).clamp(1, 4) as usize
    } else {
        ((duration_ms / 8) as usize).clamp(4, 75)
    };

    let tick_duration = Duration::from_millis(duration_ms) / steps as u32;
    let start_time = std::time::Instant::now();

    let cp1_ratio = rng.next_f64() * 0.28 + 0.20;
    let cp2_ratio = rng.next_f64() * 0.24 + 0.55;

    let max_perp_offset = (distance * 0.29).min(76.0);

    let perp_x = -delta_y / distance;
    let perp_y = delta_x / distance;

    let offset_1 = (rng.next_f64() * 0.41 + 0.07)
        * max_perp_offset
        * (if rng.next_f64() >= 0.5 { 1.0 } else { -1.0 });
    let offset_2 = (rng.next_f64() * 0.41 + 0.07)
        * max_perp_offset
        * (if rng.next_f64() >= 0.5 { 1.0 } else { -1.0 });

    let control_1x = start_x_f + delta_x * cp1_ratio + perp_x * offset_1;
    let control_1y = start_y_f + delta_y * cp1_ratio + perp_y * offset_1;
    let control_2x = start_x_f + delta_x * cp2_ratio + perp_x * offset_2;
    let control_2y = start_y_f + delta_y * cp2_ratio + perp_y * offset_2;

    let mid_wobble = rng.next_f64() < 0.37 && duration_ms > 22;
    let wobble_step = if mid_wobble { steps / 2 } else { 0 };

    for i in 0..=steps {
        let t = i as f64 / steps as f64;
        let ease = ease_in_out_quad(t);

        let mut current_x = cubic_bezier(ease, start_x_f, control_1x, control_2x, target_x_f);
        let mut current_y = cubic_bezier(ease, start_y_f, control_1y, control_2y, target_y_f);

        if mid_wobble && i == wobble_step {
            let wobble = rng.next_f64() * 1.7 + 0.7;
            let sign = if rng.next_f64() >= 0.5 { 1.0 } else { -1.0 };
            current_x += perp_x * wobble * sign;
            current_y += perp_y * wobble * sign;
        }

        move_mouse(current_x as i32, current_y as i32);

        if i < steps {
            let elapsed = start_time.elapsed();
            let expected = tick_duration * (i + 1) as u32;
            if expected > elapsed {
                std::thread::sleep(expected - elapsed);
            }
        }
    }

    if allow_overshoot && duration_ms > 16 && rng.next_f64() < 0.47 {
        let overshoot_amount = rng.next_f64() * 6.3 + 2.2;
        let dir_x = delta_x / distance;
        let dir_y = delta_y / distance;

        let over_x = (target_x_f + dir_x * overshoot_amount) as i32;
        let over_y = (target_y_f + dir_y * overshoot_amount) as i32;

        let correction_ms = (duration_ms as f64 * 0.19).max(4.0) as u64;

        smooth_move_inner(end_x, end_y, over_x, over_y, correction_ms, rng, false);
        smooth_move_inner(
            over_x,
            over_y,
            end_x,
            end_y,
            (correction_ms * 2 / 3).max(3),
            rng,
            false,
        );
    }
}

pub fn smooth_move(
    start_x: i32,
    start_y: i32,
    end_x: i32,
    end_y: i32,
    duration_ms: u64,
    rng: &mut SmallRng,
) {
    smooth_move_inner(start_x, start_y, end_x, end_y, duration_ms, rng, true);
}

#[cfg(test)]
mod tests {
    use std::cell::{Cell, RefCell};

    use super::dispatch_click;

    #[test]
    fn dispatch_click_skips_events_when_run_is_already_stopped() {
        let events = RefCell::new(Vec::new());
        let mut send_event = |flags| events.borrow_mut().push(flags);
        let mut sleep_for = |_| {};
        let is_active = || false;

        let sent = dispatch_click(1, 2, 5, &mut send_event, &mut sleep_for, &is_active);

        assert!(!sent);
        assert!(events.borrow().is_empty());
    }

    #[test]
    fn dispatch_click_releases_button_when_run_stops_during_hold() {
        let events = RefCell::new(Vec::new());
        let mut send_event = |flags| events.borrow_mut().push(flags);
        let active = Cell::new(true);
        let mut sleep_for = |_| active.set(false);
        let is_active = || active.get();

        let sent = dispatch_click(1, 2, 5, &mut send_event, &mut sleep_for, &is_active);

        assert!(!sent);
        assert_eq!(&*events.borrow(), &[1, 2]);
    }

    #[test]
    fn dispatch_click_sends_normal_down_and_up_when_run_stays_active() {
        let events = RefCell::new(Vec::new());
        let mut send_event = |flags| events.borrow_mut().push(flags);
        let mut sleep_for = |_| {};
        let is_active = || true;

        let sent = dispatch_click(1, 2, 5, &mut send_event, &mut sleep_for, &is_active);

        assert!(sent);
        assert_eq!(&*events.borrow(), &[1, 2]);
    }
}
