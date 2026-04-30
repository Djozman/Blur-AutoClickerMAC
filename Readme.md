<div align="center">

# 🖱️ Blur AutoClicker for macOS

**A native macOS port of [Blur AutoClicker](https://github.com/Blur009/Blur-AutoClicker) — every feature, fully ported.**

![Platform](https://img.shields.io/badge/platform-macOS-lightgrey?style=flat&logo=apple)
![Version](https://img.shields.io/github/v/release/Djozman/Blur-AutoClickerMAC?style=flat&label=version)
![Downloads](https://img.shields.io/github/downloads/Djozman/Blur-AutoClickerMAC/total?style=flat&label=downloads&color=blue)
![License](https://img.shields.io/github/license/Djozman/Blur-AutoClickerMAC?style=flat)

</div>

---

## ✨ Features

- 🎯 **Precise click targeting** — click at cursor position or set custom coordinates
- 🔁 **Sequence mode** — define multi-point click sequences with per-point click counts
- ⚡ **High-speed clicking** — sub-millisecond precision using CoreGraphics on macOS
- 🖱️ **Left, Right & Middle click** support
- 🔄 **Double-click mode** with configurable delay
- 🎲 **Speed variation** — randomize intervals with Gaussian distribution for human-like behavior
- ⏱️ **Time limit & click limit** — auto-stop after a set duration or number of clicks
- 🛑 **Failsafe stops** — corner stop, edge stop, and custom stop zones
- 📊 **Usage stats** — tracks total clicks, sessions, time, and CPU usage across all runs
- ⌨️ **Global hotkeys** — start/stop without switching windows
- 🚀 **Launch at login** — optional autostart via macOS LaunchAgent
- 🔔 **Auto-updater** — get notified and update in-app

---

## 🍎 Installation

1. Go to the [**latest release**](https://github.com/Djozman/Blur-AutoClickerMAC/releases/latest)
2. Download the `.dmg` for your Mac:
   - **Apple Silicon (M1/M2/M3/M4):** `aarch64` build
   - **Intel Mac:** `x86_64` build
3. Open the `.dmg` and drag **BlurAutoClicker** to your Applications folder
4. Grant **Accessibility access** when prompted:
   **System Settings → Privacy & Security → Accessibility → enable BlurAutoClicker**

> ⚠️ If macOS blocks the app on first launch, go to **System Settings → Privacy & Security** and click **Open Anyway**

---

## 🔧 'App is Damaged' Fix

If you see *"BlurAutoClicker is damaged and can't be opened"* after installing:

![Damaged warning](https://raw.githubusercontent.com/Djozman/Blur-AutoClickerMAC/main/assets/Damaged.png)

Open **Terminal** and run this command **once**:

```bash
xattr -cr /Applications/BlurAutoClicker.app
```

Then launch the app normally. You will **never need to do this again**.

---

## 🛠️ Building from Source

**Prerequisites:** Rust, Node.js 20+, Xcode Command Line Tools

```bash
git clone https://github.com/Djozman/Blur-AutoClickerMAC.git
cd Blur-AutoClickerMAC
npm install
npm run build
```

The built app and `.dmg` will be in `src-tauri/target/release/bundle/`.

---

## 📄 Credits

Original Windows version by [Blur009](https://github.com/Blur009/Blur-AutoClicker). macOS port by [Djozman](https://github.com/Djozman).
