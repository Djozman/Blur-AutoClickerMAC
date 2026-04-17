# Blur AutoClicker — macOS Port

A macOS port of [Blur-AutoClicker](https://github.com/Blur009/Blur-AutoClicker) by Blur009.  
All Windows-specific APIs have been replaced with native macOS equivalents (Core Graphics, Carbon, Core Foundation).

> **Apple Silicon (arm64) only.** Built and tested on macOS 12 Monterey and later.

---

## macOS vs Windows — Feature Comparison

| Feature | macOS | Windows |
|---|---|---|
| Simple Mode (toggle/hold, hotkeys, mouse button) | ✅ | ✅ |
| Advanced Mode | ✅ | ✅ |
| Duty Cycle | ✅ | ✅ |
| Speed Variation (randomized CPS) | ✅ | ✅ |
| Double Click | ✅ | ✅ |
| Click Limit | ✅ | ✅ |
| Time Limit | ✅ | ✅ |
| Corner Stop | ✅ | ✅ |
| Edge Stop | ✅ | ✅ |
| Position Clicking | ✅ | ✅ |
| Clicks per Second / Minute / Hour / Day | ✅ | ✅ |
| Click stats overlay | ✅ | ✅ |
| Light / Dark theme | ✅ | ✅ |
| **Max sustained 1:1 CPS** | **✅ ~2000 CPS** | ⚠️ ~500 CPS |

> **What "1:1 CPS" means:** the ratio of clicks shown in the UI vs. clicks actually registered by the OS. Windows enforces a ~1ms minimum timer resolution system-wide, which hard-caps consistent input delivery at around 500 CPS. macOS does not have this limitation — the kernel timer resolution is fine-grained enough to sustain far higher rates.
>
> That said, macOS is not unlimited. In practice, the highest tested cap where clicks remain consistently 1:1 is **~2000 CPS**. Above that, the OS begins dropping inputs and the ratio degrades.
>
> **For the best 1:1 accuracy at high CPS, disable both Duty Cycle and Speed Variation.** Both features introduce timing jitter that compounds input drop-off at high rates. The comparison below shows the difference:
>
> | Duty Cycle OFF · Speed Variation OFF | Duty Cycle ON · Speed Variation ON |
> |:---:|:---:|
> | <!-- Add screenshot: assets/cps-both-off.png --> | <!-- Add screenshot: assets/cps-both-on.png --> |
> | `![Both off](assets/cps-both-off.png)` | `![Both on](assets/cps-both-on.png)` |
>
> *With both options disabled, the UI CPS and actual registered CPS stay 1:1. With them enabled, timing jitter causes measurable input drop at high rates.*

---

## Requirements

- macOS 12 Monterey or later
- [Node.js](https://nodejs.org/) v18+
- [Rust](https://rustup.rs/) (stable toolchain)
- Xcode Command Line Tools

---

## Install Dependencies

### Xcode Command Line Tools
```bash
xcode-select --install
```

### Node.js (via [nvm](https://github.com/nvm-sh/nvm))
```bash
curl -o- https://raw.githubusercontent.com/nvm-sh/nvm/v0.39.7/install.sh | bash
source ~/.zshrc
nvm install --lts
node -v
```

### Rust
```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source "$HOME/.cargo/env"
rustc --version
```

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

### Install to Applications

```bash
rm -rf /Applications/BlurAutoClicker.app
cp -r src-tauri/target/release/bundle/macos/BlurAutoClicker.app /Applications/
```

Or run the binary directly (no permissions popup, useful for testing):
```bash
open src-tauri/target/release/BlurAutoClicker
```

---

## Required Permissions (first launch)

macOS blocks synthetic input by default.  
After launching the app for the first time, grant the following permission:

1. **System Settings → Privacy & Security → Accessibility** — add `BlurAutoClicker`

Without **Accessibility**, simulated mouse clicks will silently fail.

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

### App launches but shows a white screen / blank overlay

**Cause:** The `.app` bundle was built with incorrect macOS bundle configuration.  
**Fix:** Make sure you are on the latest commit and do a clean rebuild:
```bash
git pull
npm run tauri build
rm -rf /Applications/BlurAutoClicker.app
cp -r src-tauri/target/release/bundle/macos/BlurAutoClicker.app /Applications/
```
If the `.app` still fails, run the raw binary directly to confirm the build itself works:
```bash
open src-tauri/target/release/BlurAutoClicker
```

---

### `use of unresolved module or unlinked crate 'windows_targets'`

**Cause:** You have an old local clone that still has the original Windows-only `worker.rs`.  
**Fix:** Wipe and re-clone:
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

### `Error: A public key has been found, but no private key`

**Cause:** The updater plugin requires a signing key to produce update artifacts. This does not affect the app itself.  
**Fix:** Safe to ignore. The `.app` bundle is fully built and functional despite this error.

---

### `zsh: number expected` / `zsh: unknown file attribute`

**Cause:** You pasted a multi-line block that included `#` comment lines. Zsh treats `#` differently when pasted inline.  
**Fix:** Run commands **one line at a time**, never paste comment lines.

---

## What Was Changed from the Windows Version

| File | Change |
|---|---|
| `src-tauri/Cargo.toml` | Removed `windows-sys`, `winreg`, `windows-targets` |
| `src-tauri/tauri.conf.json` | Changed bundle target from `nsis` to `app` (macOS bundle), added macOS icon formats and minimum system version |
| `src-tauri/src/engine/mouse.rs` | Rewrote using `CGEventCreateMouseEvent`, `CGWarpMouseCursorPosition` (Core Graphics) |
| `src-tauri/src/engine/worker.rs` | Removed `NtSetTimerResolution`, `QueryThreadCycleTime`, `windows_targets::link!` |
| `src-tauri/src/engine/mod.rs` | Removed `NtSetTimerResolution` extern block — macOS kernel timer resolution is sufficient |
| `src-tauri/src/hotkeys.rs` | Replaced Win32 keyboard hooks with Carbon `GetKeys` + `CGEventTap` for scroll wheel |
| `src-tauri/src/overlay.rs` | Replaced all Win32 window calls with Tauri's cross-platform window API |

---

## Credits

- Original Windows app: [Blur009/Blur-AutoClicker](https://github.com/Blur009/Blur-AutoClicker)
- macOS port: [Djozman](https://github.com/Djozman)
