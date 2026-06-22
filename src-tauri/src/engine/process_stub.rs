// macOS stub for the process list / task switcher feature.
// The Windows implementation lives in `process.rs` and uses Win32 APIs
// (EnumWindows, CreateToolhelp32Snapshot, ExtractIconExW, etc.) that have
// no direct macOS equivalent without pulling in AppKit. For the macOS port
// we expose the same API surface so the worker and UI commands compile, but
// the feature is effectively a no-op: no processes are enumerated, the task
// switcher is never reported active, and the process list never triggers a
// stop or pause. The UI still lets users configure entries; they just won't
// match anything on macOS until a native implementation is added.

use super::{ClickerConfig, ProcessListBehavior};

#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProcessInfo {
    pub name: String,
    pub display_name: String,
    pub pid: u32,
    pub icon_base64: Option<String>,
}

/// Normalize a process name. On Windows this lowercases and strips the path;
/// on macOS we keep the same behavior for consistency with stored settings.
pub fn normalize_process_name(name: &str) -> String {
    let trimmed = name.trim();
    let lower = trimmed.to_lowercase();
    lower
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or(&lower)
        .to_string()
}

/// Returns the foreground process name. Not implemented on macOS.
#[allow(dead_code)]
pub fn get_foreground_process_name() -> Option<String> {
    None
}

/// Lists running processes. Not implemented on macOS — returns an empty list.
pub fn list_running_processes() -> Vec<ProcessInfo> {
    Vec::new()
}

/// Checks the process list against the foreground process. Always returns
/// `None` on macOS (no-op), so the clicker never stops or pauses due to the
/// process list.
pub fn check_process_list(_config: &ClickerConfig) -> Option<ProcessListBehavior> {
    None
}

/// Detects whether the Alt+Tab / Cmd+Tab task switcher is active. Not
/// implemented on macOS — always returns `false`.
pub fn is_task_switcher_active() -> bool {
    false
}
