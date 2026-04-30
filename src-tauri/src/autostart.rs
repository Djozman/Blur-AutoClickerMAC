use std::io;

// ── Windows ───────────────────────────────────────────────────────────────────

#[cfg(target_os = "windows")]
const APP_NAME: &str = "BlurAutoClicker";
#[cfg(target_os = "windows")]
const RUN_KEY: &str = "SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\Run";

#[cfg(target_os = "windows")]
pub fn get_autostart_enabled() -> bool {
    use winreg::enums::HKEY_CURRENT_USER;
    use winreg::RegKey;

    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let Ok(run_key) = hkcu.open_subkey(RUN_KEY) else {
        return false;
    };
    run_key.get_value::<String, _>(APP_NAME).is_ok()
}

#[cfg(target_os = "windows")]
pub fn set_autostart_enabled(enabled: bool) -> io::Result<()> {
    use winreg::enums::{HKEY_CURRENT_USER, KEY_WRITE};
    use winreg::RegKey;

    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let run_key = hkcu.open_subkey_with_flags(RUN_KEY, KEY_WRITE)?;

    if enabled {
        let exe_path = std::env::current_exe()?;
        let value = format!("\"{}\" --autostart", exe_path.display());
        run_key.set_value(APP_NAME, &value)?;
    } else {
        let _ = run_key.delete_value(APP_NAME);
    }

    Ok(())
}

// ── macOS ─────────────────────────────────────────────────────────────────────

#[cfg(target_os = "macos")]
const LAUNCH_AGENT_LABEL: &str = "com.djozman.BlurAutoClicker";

#[cfg(target_os = "macos")]
fn launch_agent_path() -> Option<std::path::PathBuf> {
    dirs::home_dir().map(|home| {
        home.join("Library/LaunchAgents")
            .join(format!("{LAUNCH_AGENT_LABEL}.plist"))
    })
}

#[cfg(target_os = "macos")]
pub fn get_autostart_enabled() -> bool {
    launch_agent_path().map(|p| p.exists()).unwrap_or(false)
}

#[cfg(target_os = "macos")]
pub fn set_autostart_enabled(enabled: bool) -> io::Result<()> {
    let path = launch_agent_path().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::NotFound,
            "Could not locate ~/Library/LaunchAgents",
        )
    })?;

    if enabled {
        let exe = std::env::current_exe()?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let plist = format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>{label}</string>
    <key>ProgramArguments</key>
    <array>
        <string>{exe}</string>
        <string>--autostart</string>
    </array>
    <key>RunAtLoad</key>
    <true/>
</dict>
</plist>"#,
            label = LAUNCH_AGENT_LABEL,
            exe = exe.display(),
        );
        std::fs::write(&path, plist)?;
    } else {
        let _ = std::fs::remove_file(&path);
    }

    Ok(())
}
