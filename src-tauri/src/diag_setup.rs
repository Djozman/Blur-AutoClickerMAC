use tauri::AppHandle;
use tauri_plugin_log::{RotationStrategy, Target, TargetKind, TimezoneStrategy};

/// Install a panic hook that writes a panic report into the diagnostics
/// PanicReports directory. On macOS this is our crash-capture path; the
/// crashpad out-of-process handler is Windows-only and disabled in this build.
pub fn setup_panic_hook() {
    std::panic::set_hook(Box::new(|info| {
        let msg = info.to_string();
        let location = info
            .location()
            .map(|l| format!("{}:{}", l.file(), l.line()))
            .unwrap_or_else(|| "unknown".to_string());
        let timestamp = chrono::Local::now().to_rfc3339();
        let backtrace = std::backtrace::Backtrace::force_capture();
        let report = format!(
            "BlurAutoClicker panic report\nTimestamp: {timestamp}\nLocation: {location}\nMessage: {msg}\n\nBacktrace:\n{backtrace}\n"
        );
        log::error!("[Panic] {msg} (at {location})");
        crate::diagnostics::write_panic_report(&report);
    }));
}

/// Configure tauri-plugin-log: stdout, a rotating `session` log file in the
/// diagnostics Logs directory, the webview console, and the app-events
/// telemetry dispatch target.
pub fn setup_logging(handle: &AppHandle) {
    let _ = crate::diagnostics::ensure_diagnostics_dirs();

    let log_level = if cfg!(debug_assertions) {
        log::LevelFilter::Trace
    } else {
        log::LevelFilter::Info
    };

    let mut targets = vec![Target::new(TargetKind::Stdout)];
    if let Some(log_dir) = crate::diagnostics::logs_dir() {
        targets.push(Target::new(TargetKind::Folder {
            path: log_dir,
            file_name: Some("session".to_string()),
        }));
    }
    targets.push(Target::new(TargetKind::Webview));
    targets.push(Target::new(TargetKind::Dispatch(
        crate::app_events::create_app_events_target(),
    )));

    let builder = tauri_plugin_log::Builder::default()
        .targets(targets)
        .level(log_level)
        .level_for("tao", log::LevelFilter::Warn)
        .max_file_size(2_500_000)
        .rotation_strategy(RotationStrategy::KeepSome(0))
        .timezone_strategy(TimezoneStrategy::UseLocal);

    let _ = handle.plugin(builder.build());
}
