# Blur AutoClicker — macOS Port

A macOS port of [Blur-AutoClicker](https://github.com/Blur009/Blur-AutoClicker) by Blur009.  
All Windows-specific APIs have been replaced with native macOS equivalents (Core Graphics, Carbon, Core Foundation).

---

## Requirements

- macOS 12 Monterey or later
- [Node.js](https://nodejs.org/) v18+
- [Rust](https://rustup.rs/) (stable)
- Xcode Command Line Tools — run `xcode-select --install` if not already installed

---

## Build from Source

```bash
# 1. Clone the repo
git clone https://github.com/Djozman/Blur-AutoClickerMAC.git
cd Blur-AutoClickerMAC

# 2. Install JS dependencies and register the Tauri build script
npm install
npm pkg set scripts.tauri="tauri"

# 3. Generate app icons (uses macOS built-in sips — no extra tools needed)
mkdir -p src-tauri/icons
sips -s format png --resampleWidth 1024 \
  /System/Library/CoreServices/CoreTypes.bundle/Contents/Resources/GenericApplicationIcon.icns \
  --out src-tauri/icons/icon.png
npm run tauri icon src-tauri/icons/icon.png

# 4. Build
npm run tauri build
```

The compiled app will be at:
```
src-tauri/target/release/bundle/macos/BlurAutoClicker.app
```

Open it with:
```bash
open src-tauri/target/release/bundle/macos/
```

---

## Required Permissions (first launch)

macOS blocks synthetic input and event monitoring by default.  
After launching the app for the first time, grant both permissions:

1. **System Settings → Privacy & Security → Accessibility** — add `BlurAutoClicker`
2. **System Settings → Privacy & Security → Input Monitoring** — add `BlurAutoClicker`

Without **Accessibility**, simulated mouse clicks will silently fail.  
Without **Input Monitoring**, scroll wheel hotkeys won't fire.

Restart the app after granting permissions.

---

## What was changed from the Windows version

| File | Change |
|---|---|
| `src-tauri/Cargo.toml` | Removed `windows-sys`, `winreg`, `windows-targets` |
| `src-tauri/src/engine/mouse.rs` | Rewrote using `CGEventCreateMouseEvent`, `CGWarpMouseCursorPosition` (Core Graphics) |
| `src-tauri/src/engine/worker.rs` | Removed `NtSetTimerResolution`, `QueryThreadCycleTime`, `windows_targets::link!` |
| `src-tauri/src/engine/mod.rs` | Removed `NtSetTimerResolution` extern block — macOS kernel timer resolution is sufficient |
| `src-tauri/src/hotkeys.rs` | Replaced Win32 keyboard hooks with Carbon `GetKeys` + `CGEventTap` for scroll wheel |
| `src-tauri/src/overlay.rs` | Replaced all Win32 window calls with Tauri's cross-platform window API |

---

## Credits

- Original Windows app: [Blur009/Blur-AutoClicker](https://github.com/Blur009/Blur-AutoClicker)
- macOS port: [Djozman](https://github.com/Djozman)
