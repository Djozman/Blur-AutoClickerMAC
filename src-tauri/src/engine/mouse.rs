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
}

// ── Windows implementation ────────────────────────────────────────────────────

#[cfg(target_os = "windows")]
mod platform {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
        SendInput, INPUT, INPUT_MOUSE, MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP,
        MOUSEEVENTF_MIDDLEDOWN, MOUSEEVENTF_MIDDLEUP, MOUSEEVENTF_RIGHTDOWN, MOUSEEVENTF_RIGHTUP,
        MOUSEINPUT,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GetSystemMetrics, SetCursorPos, SM_CXVIRTUALSCREEN, SM_CYVIRTUALSCREEN, SM_XVIRTUALSCREEN,
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

    pub fn move_mouse(x: i32, y: i32) {
        unsafe { SetCursorPos(x, y) };
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
    const CG_HID_EVENT_TAP: u32 = 0;
    const CG_EVENT_MOUSE_MOVED: u32 = 5;

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

    fn event_source() -> *mut c_void {
        static mut SRC: *mut c_void = std::ptr::null_mut();
        unsafe {
            if SRC.is_null() {
                SRC = CGEventSourceCreate(1); // kCGEventSourceStateHIDSystemState
            }
            SRC
        }
    }

    fn get_cursor_point() -> CGPoint {
        unsafe {
            let event = CGEventCreate(event_source());
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

    pub fn move_mouse(x: i32, y: i32) {
        unsafe {
            let point = CGPoint {
                x: x as f64,
                y: y as f64,
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
        let src = event_source();
        unsafe {
            let event = CGEventCreateMouseEvent(src, event_type, pos, mouse_button);
            if !event.is_null() {
                CGEventPost(CG_HID_EVENT_TAP, event);
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
        let src = event_source();
        unsafe {
            for _ in 0..n {
                let ev_down = CGEventCreateMouseEvent(src, down, pos, mouse_button);
                if !ev_down.is_null() {
                    CGEventPost(CG_HID_EVENT_TAP, ev_down);
                    CFRelease(ev_down);
                }
                let ev_up = CGEventCreateMouseEvent(src, up, pos, mouse_button);
                if !ev_up.is_null() {
                    CGEventPost(CG_HID_EVENT_TAP, ev_up);
                    CFRelease(ev_up);
                }
            }
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
                std::ptr::null_mut(),
                CG_EVENT_MOUSE_MOVED,
                point,
                CG_MOUSE_BUTTON_LEFT,
            );
            if !event.is_null() {
                CGEventPost(CG_HID_EVENT_TAP, event);
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

pub fn smooth_move(
    start_x: i32,
    start_y: i32,
    end_x: i32,
    end_y: i32,
    duration_ms: u64,
    rng: &mut SmallRng,
) {
    if duration_ms < 5 {
        move_mouse(end_x, end_y);
        return;
    }

    let (sx, sy) = (start_x as f64, start_y as f64);
    let (ex, ey) = (end_x as f64, end_y as f64);
    let (dx, dy) = (ex - sx, ey - sy);
    let distance = (dx * dx + dy * dy).sqrt();
    if distance < 1.0 {
        return;
    }

    let (perp_x, perp_y) = (-dy / distance, dx / distance);
    let sign = |b: bool| if b { 1.0f64 } else { -1.0 };
    let o1 = (rng.next_f64() * 0.3 + 0.15) * distance * sign(rng.next_f64() >= 0.5);
    let o2 = (rng.next_f64() * 0.3 + 0.15) * distance * sign(rng.next_f64() >= 0.5);
    let cp1x = sx + dx * 0.33 + perp_x * o1;
    let cp1y = sy + dy * 0.33 + perp_y * o1;
    let cp2x = sx + dx * 0.66 + perp_x * o2;
    let cp2y = sy + dy * 0.66 + perp_y * o2;

    let steps = (duration_ms as usize).clamp(10, 200);
    let step_dur = Duration::from_millis(duration_ms / steps as u64);

    for i in 0..=steps {
        let t = ease_in_out_quad(i as f64 / steps as f64);
        let mx = cubic_bezier(t, sx, cp1x, cp2x, ex) as i32;
        let my = cubic_bezier(t, sy, cp1y, cp2y, ey) as i32;
        move_mouse(mx, my);
        if i < steps {
            std::thread::sleep(step_dur);
        }
    }
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
