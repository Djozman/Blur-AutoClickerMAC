//! macOS mouse backend — replaces the Windows-only windows-sys implementation.
//! Uses Core Graphics (via the `core-graphics` crate) for cursor position,
//! mouse movement, and synthetic click events.

use std::time::Duration;

use super::rng::SmallRng;
use super::worker::{sleep_interruptible, RunControl};

#[link(name = "CoreGraphics", kind = "framework")]
extern "C" {
    fn CGEventCreate(source: *const std::ffi::c_void) -> *mut std::ffi::c_void;
    fn CGEventGetLocation(event: *mut std::ffi::c_void) -> CGPoint;
    fn CFRelease(cf: *mut std::ffi::c_void);
    fn CGWarpMouseCursorPosition(new_cursor_position: CGPoint) -> i32;
    fn CGEventPost(tap: u32, event: *mut std::ffi::c_void);
    fn CGEventCreateMouseEvent(
        source: *const std::ffi::c_void,
        mouse_type: u32,
        mouse_cursor_position: CGPoint,
        mouse_button: u32,
    ) -> *mut std::ffi::c_void;
    fn CGMainDisplayID() -> u32;
    fn CGDisplayPixelsWide(display: u32) -> usize;
    fn CGDisplayPixelsHigh(display: u32) -> usize;
    fn CGDisplayBounds(display: u32) -> CGRect;
    fn CGGetActiveDisplayList(
        max_displays: u32,
        active_displays: *mut u32,
        display_count: *mut u32,
    ) -> i32;
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct CGPoint {
    pub x: f64,
    pub y: f64,
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct CGRect {
    pub origin: CGPoint,
    pub size: CGSize,
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct CGSize {
    pub width: f64,
    pub height: f64,
}

const K_CG_EVENT_LEFT_MOUSE_DOWN: u32 = 1;
const K_CG_EVENT_LEFT_MOUSE_UP: u32 = 2;
const K_CG_EVENT_RIGHT_MOUSE_DOWN: u32 = 3;
const K_CG_EVENT_RIGHT_MOUSE_UP: u32 = 4;
const K_CG_OTHER_MOUSE_DOWN: u32 = 25;
const K_CG_OTHER_MOUSE_UP: u32 = 26;
const K_CG_MOUSE_BUTTON_LEFT: u32 = 0;
const K_CG_MOUSE_BUTTON_RIGHT: u32 = 1;
const K_CG_MOUSE_BUTTON_CENTER: u32 = 2;
const K_CG_HID_EVENT_TAP: u32 = 1;

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
        Self { left, top, width, height }
    }
    #[inline]
    pub fn right(self) -> i32 { self.left + self.width }
    #[inline]
    pub fn bottom(self) -> i32 { self.top + self.height }
    #[inline]
    pub fn contains(self, x: i32, y: i32) -> bool {
        x >= self.left && x < self.right() && y >= self.top && y < self.bottom()
    }
    #[inline]
    pub fn offset_from(self, origin: VirtualScreenRect) -> Self {
        Self::new(self.left - origin.left, self.top - origin.top, self.width, self.height)
    }
}

pub fn current_cursor_position() -> Option<(i32, i32)> {
    unsafe {
        let event = CGEventCreate(std::ptr::null());
        if event.is_null() { return None; }
        let pt = CGEventGetLocation(event);
        CFRelease(event);
        Some((pt.x as i32, pt.y as i32))
    }
}

#[inline]
pub fn get_cursor_pos() -> (i32, i32) {
    current_cursor_position().unwrap_or((0, 0))
}

pub fn current_virtual_screen_rect() -> Option<VirtualScreenRect> {
    unsafe {
        let display = CGMainDisplayID();
        let w = CGDisplayPixelsWide(display) as i32;
        let h = CGDisplayPixelsHigh(display) as i32;
        if w <= 0 || h <= 0 { return None; }
        Some(VirtualScreenRect::new(0, 0, w, h))
    }
}

pub fn current_monitor_rects() -> Option<Vec<VirtualScreenRect>> {
    unsafe {
        let mut displays: [u32; 32] = [0; 32];
        let mut count: u32 = 0;
        let err = CGGetActiveDisplayList(32, displays.as_mut_ptr(), &mut count);
        if err != 0 || count == 0 {
            return current_virtual_screen_rect().map(|s| vec![s]);
        }
        let mut rects: Vec<VirtualScreenRect> = (0..count as usize)
            .filter_map(|i| {
                let r = CGDisplayBounds(displays[i]);
                let w = r.size.width as i32;
                let h = r.size.height as i32;
                if w > 0 && h > 0 {
                    Some(VirtualScreenRect::new(r.origin.x as i32, r.origin.y as i32, w, h))
                } else { None }
            })
            .collect();
        if rects.is_empty() {
            return current_virtual_screen_rect().map(|s| vec![s]);
        }
        rects.sort_by_key(|r| (r.top, r.left));
        Some(rects)
    }
}

#[inline]
pub fn move_mouse(x: i32, y: i32) {
    unsafe { CGWarpMouseCursorPosition(CGPoint { x: x as f64, y: y as f64 }); }
}

fn post_mouse_event(event_type: u32, x: i32, y: i32, button: u32) {
    unsafe {
        let pos = CGPoint { x: x as f64, y: y as f64 };
        let event = CGEventCreateMouseEvent(std::ptr::null(), event_type, pos, button);
        if !event.is_null() {
            CGEventPost(K_CG_HID_EVENT_TAP, event);
            CFRelease(event);
        }
    }
}

#[inline]
pub fn get_button_flags(button: i32) -> (u32, u32) {
    match button {
        2 => (K_CG_EVENT_RIGHT_MOUSE_DOWN, K_CG_EVENT_RIGHT_MOUSE_UP),
        3 => (K_CG_OTHER_MOUSE_DOWN, K_CG_OTHER_MOUSE_UP),
        _ => (K_CG_EVENT_LEFT_MOUSE_DOWN, K_CG_EVENT_LEFT_MOUSE_UP),
    }
}

fn button_for_type(event_type: u32) -> u32 {
    match event_type {
        K_CG_EVENT_RIGHT_MOUSE_DOWN | K_CG_EVENT_RIGHT_MOUSE_UP => K_CG_MOUSE_BUTTON_RIGHT,
        K_CG_OTHER_MOUSE_DOWN | K_CG_OTHER_MOUSE_UP => K_CG_MOUSE_BUTTON_CENTER,
        _ => K_CG_MOUSE_BUTTON_LEFT,
    }
}

pub fn send_mouse_event(event_type: u32) {
    let (x, y) = get_cursor_pos();
    post_mouse_event(event_type, x, y, button_for_type(event_type));
}

pub fn send_batch(down: u32, up: u32, n: usize, _hold_ms: u32) {
    let (x, y) = get_cursor_pos();
    let btn = button_for_type(down);
    for _ in 0..n {
        post_mouse_event(down, x, y, btn);
        post_mouse_event(up, x, y, btn);
    }
}

pub fn send_clicks(
    down: u32,
    up: u32,
    count: usize,
    hold_ms: u32,
    use_double_click_gap: bool,
    double_click_delay_ms: u32,
    control: &RunControl,
) {
    if count == 0 { return; }
    if !use_double_click_gap && count > 1 && hold_ms == 0 {
        send_batch(down, up, count, hold_ms);
        return;
    }
    for index in 0..count {
        if !control.is_active() { return; }
        send_mouse_event(down);
        if hold_ms > 0 {
            sleep_interruptible(Duration::from_millis(hold_ms as u64), control);
            if !control.is_active() { return; }
        }
        send_mouse_event(up);
        if index + 1 < count && use_double_click_gap && double_click_delay_ms > 0 {
            sleep_interruptible(Duration::from_millis(double_click_delay_ms as u64), control);
        }
    }
}

#[inline]
pub fn ease_in_out_quad(t: f64) -> f64 {
    if t < 0.5 { 2.0 * t * t } else { 1.0 - (-2.0 * t + 2.0).powi(2) / 2.0 }
}

#[inline]
pub fn cubic_bezier(t: f64, p0: f64, p1: f64, p2: f64, p3: f64) -> f64 {
    let u = 1.0 - t;
    u * u * u * p0 + 3.0 * u * u * t * p1 + 3.0 * u * t * t * p2 + t * t * t * p3
}

pub fn smooth_move(
    start_x: i32, start_y: i32, end_x: i32, end_y: i32,
    duration_ms: u64, rng: &mut SmallRng,
) {
    if duration_ms < 5 { move_mouse(end_x, end_y); return; }
    let (sx, sy) = (start_x as f64, start_y as f64);
    let (ex, ey) = (end_x as f64, end_y as f64);
    let (dx, dy) = (ex - sx, ey - sy);
    let distance = (dx * dx + dy * dy).sqrt();
    if distance < 1.0 { return; }
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
        move_mouse(
            cubic_bezier(t, sx, cp1x, cp2x, ex) as i32,
            cubic_bezier(t, sy, cp1y, cp2y, ey) as i32,
        );
        if i < steps { std::thread::sleep(step_dur); }
    }
}
