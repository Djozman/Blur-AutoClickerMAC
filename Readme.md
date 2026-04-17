# Blur AutoClicker — macOS Port

A macOS port of [Blur-AutoClicker](https://github.com/Blur009/Blur-AutoClicker) by Blur009.  
All Windows-specific APIs have been replaced with native macOS equivalents (Core Graphics, Carbon, Core Foundation).

---

## Requirements

- macOS 12 Monterey or later
- [Node.js](https://nodejs.org/) v18+
- [Rust](https://rustup.rs/) (stable toolchain)
- Xcode Command Line Tools — run `xcode-select --install` if not already installed

---

## Build from Source

Run every command **one line at a time** in Terminal. Do not paste blocks with `#` comments — zsh will error on them.

```bash
cd ~/Documents
git clone https://github.com/Djozman/Blur-AutoClickerMAC.git
cd Blur-AutoClickerMAC
npm install
npm install --save-dev @tauri-apps/cli
npm pkg set scripts.tauri="tauri"
```

### Generate app icons

```bash
mkdir -p src-tauri/icons
sips -s format png --resampleWidth 1024 /System/Library/CoreServices/CoreTypes.bundle/Contents/Resources/GenericApplicationIcon.icns --out src-tauri/icons/icon.png
npm run tauri icon src-tauri/icons/icon.png
```

### Fix unused import in overlay.rs

```bash
sed -i '' 's/use crate::engine::mouse::{current_monitor_rects, current_virtual_screen_rect, VirtualScreenRect};/use crate::engine::mouse::{current_monitor_rects, current_virtual_screen_rect};/' src-tauri/src/overlay.rs
```

### Build

```bash
npm run tauri build
```

The compiled app will be at:
```
src-tauri/target/release/bundle/macos/BlurAutoClicker.app
```

Open it:
```bash
open src-tauri/target/release/bundle/macos/
```

Or run the binary directly:
```bash
open src-tauri/target/release/BlurAutoClicker
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

## Errors You May Encounter

### `cd: no such file or directory: /path/to/Blur-AutoClickerMAC`

**Cause:** You copy-pasted the placeholder path literally instead of cloning first.  
**Fix:**
```bash
cd ~/Documents
git clone https://github.com/Djozman/Blur-AutoClickerMAC.git
cd Blur-AutoClickerMAC
```

---

### `npm error Missing script: "tauri"`

**Cause:** The `tauri` script is not registered in `package.json` and/or `@tauri-apps/cli` is not installed.  
**Fix:**
```bash
npm install --save-dev @tauri-apps/cli
npm pkg set scripts.tauri="tauri"
```

---

### `failed to open icon .../src-tauri/icons/icon.png: No such file or directory`

**Cause:** The `icons/` folder doesn't exist in the repo — icons are generated locally and not committed.  
**Fix:**
```bash
mkdir -p src-tauri/icons
sips -s format png --resampleWidth 1024 /System/Library/CoreServices/CoreTypes.bundle/Contents/Resources/GenericApplicationIcon.icns --out src-tauri/icons/icon.png
npm run tauri icon src-tauri/icons/icon.png
```

---

### `use of unresolved module or unlinked crate 'windows_targets'`

**Cause:** You have an old local clone that still has the original Windows-only `worker.rs`. Your local files are outdated.  
**Fix:** Wipe and re-clone so you get the latest fixed files:
```bash
cd ~/Documents
rm -rf Blur-AutoClickerMAC
git clone https://github.com/Djozman/Blur-AutoClickerMAC.git
cd Blur-AutoClickerMAC
```

---

### `warning: unused import: VirtualScreenRect`

**Cause:** A leftover import in `overlay.rs` from the Windows version.  
**Fix:**
```bash
sed -i '' 's/use crate::engine::mouse::{current_monitor_rects, current_virtual_screen_rect, VirtualScreenRect};/use crate::engine::mouse::{current_monitor_rects, current_virtual_screen_rect};/' src-tauri/src/overlay.rs
```

---

### `zsh: number expected` / `zsh: unknown file attribute`

**Cause:** You pasted a multi-line block that included `#` comment lines. Zsh treats `#` differently when pasted inline.  
**Fix:** Run commands **one line at a time**, never paste comment lines.

---

## What Was Changed from the Windows Version

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
