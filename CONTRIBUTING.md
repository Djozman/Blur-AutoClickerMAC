# Contributing to Blur Auto Clicker

Thanks for helping improve Blur Auto Clicker.

## Project scope

- Blur Auto Clicker is a Windows-first desktop app built with Tauri 2, Rust, React, and TypeScript.
- Keep changes focused. Avoid unrelated refactors in the same pull request.
- If your change affects the UI, include screenshots or a short recording in the pull request.

## Prerequisites

- Node.js 20 or newer
- Rust via `rustup`
- Microsoft C++ Build Tools / Visual Studio Build Tools
- Windows with the Rust `x86_64-pc-windows-msvc` toolchain installed

## Setup

```powershell
git clone https://github.com/Blur009/Blur-AutoClicker.git
cd Blur-AutoClicker
npm install
rustup default stable-x86_64-pc-windows-msvc
```

## Local development

Run the app in development:

```powershell
npm run dev
```

Build the frontend only:

```powershell
npm run frontend:build
```

Build the desktop app bundle:

```powershell
npm run build
```

## Validation

Run all checks before opening a pull request:

```powershell
npm run check:all
```

This runs all these automatically — cargo test, npm test, ESLint, Prettier, frontend type check and build, cargo check, cargo clippy, cargo fmt, and npm audit.

## Branches and pull requests

- Open feature and fix pull requests against `dev`.
- Keep pull requests small enough to review comfortably.
- Link the related issue when there is one, or write `N/A`.
- Use the issue forms before opening a new issue.

## Questions

If you have questions or get stuck, join the [Discord server](https://discord.gg/jhWEW747x5).

## Generated files

- Some files are generated, including schema output under `src-tauri/gen/`.
- Only commit generated files when they were intentionally updated as part of the change.
- If generated files changed unexpectedly, review them before committing.
