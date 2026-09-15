# Tether

> **A high-performance, private-by-default screen-time analytics workspace, limit enforcer, and web filter for Windows.**
> Understand where your attention goes, establish healthy boundaries, and enforce hard limits on distracting apps and websites—with zero cloud telemetry, zero subscription lock-in, and zero gaming performance impact.

[![CI](https://github.com/distru3/tether/actions/workflows/ci.yml/badge.svg)](https://github.com/distru3/tether/actions/workflows/ci.yml)
[![Release](https://img.shields.io/badge/release-v0.2.0--beta-indigo.svg)](https://github.com/distru3/tether/releases)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/rust-1.83%2B-orange.svg)](rust-toolchain.toml)
[![Tauri 2](https://img.shields.io/badge/tauri-v2-cyan.svg)](https://tauri.app/)
[![React 18](https://img.shields.io/badge/react-18.3-61dafb.svg)](https://react.dev/)

---

## Table of Contents

- [Overview & Philosophy](#overview--philosophy)
- [Key Features](#key-features)
- [Architecture & Privilege Model](#architecture--privilege-model)
- [Hardware Multiplane Overlay (MPO) Gaming HUD](#hardware-multiplane-overlay-mpo-gaming-hud)
- [Hardware-Accelerated Block Overlay](#hardware-accelerated-block-overlay)
- [Installation for Testers](#installation-for-testers)
- [Development Guide](#development-guide)
  - [Prerequisites](#prerequisites)
  - [Unified Dev Runner](#unified-dev-runner)
  - [Verification Gate & CI](#verification-gate--ci)
  - [Building the Production Installer](#building-the-production-installer)
- [CLI Reference](#cli-reference)
- [Repository & Crate Layout](#repository--crate-layout)
- [Security & Threat Model](#security--threat-model)
- [License](#license)

---

## Overview & Philosophy

Most commercial focus and screen-time applications fall into one of two traps:
1. **Invasive Cloud SaaS**: They harvest your active window titles, keystrokes, and browsing patterns to monetize your data or charge recurring monthly subscriptions.
2. **Superficial Browser Blockers**: They are trivially bypassed by opening an incognito window, using a secondary browser, or switching to an unmonitored desktop application.

**Tether** takes a radically different engineering approach:
- **100% Local-First**: All usage history, classification rules, and enforcement logs are stored in a local SQLite database on your machine (`%ProgramData%\screentime\screentime.db`). Nothing is uploaded to any remote server.
- **Whole-System Scope**: Tracks executable processes across the entire OS (browsers, IDEs, 3D games, terminal sessions, office tools), aggregating usage into hierarchical categories and timelines.
- **Fail-Closed Architecture**: Active blocks persist end-of-local-day expiries (`DayKey::end_utc`). If the system is rebooted, processes are restarted, or services cycle, enforcement resumes immediately without fail-open loopholes.
- **Process Suspension Over Termination**: When an application limit trips, Tether freezes the process tree via native Win32 suspension, ensuring uncommitted editor files, browser tabs, or documents remain completely intact until the user extends time or authorizes an override.
- **Anti-Impulse Friction**: Tightening a limit or revoking an override takes effect immediately. Relaxing or increasing a limit requires an asynchronous 24-hour cooldown queue, preventing impulsive self-sabotage.

---

## Key Features

### 📊 Real-Time Screen Time Analytics
- **1 Hz Local Sampling**: Continuously monitors the Win32 foreground window and system idle state (`GetLastInputInfo`). Automatically pauses accrual during idle periods or lock screen sessions.
- **Interactive Daily Timeline**: Visual 24-hour timeline rule rendering active intervals, category swatches, active limit markers, and live progress indicators (with future dimming and current-time indicators gated exclusively to today).
- **Categorization Engine**: Hierarchical classification into *Development*, *Productivity*, *Communication*, *Design*, *Entertainment*, *Gaming*, and *Social*. Supports primary classifications plus multi-tag categorization.
- **7-Day Trend Comparison**: Monospace metric readouts comparing current week usage against historical periods with percentage variance indicators.

### 🛡️ Daily Limits & Anti-Impulse Cooldown
- **App & Category Budgets**: Assign daily screen time budgets to specific applications (`Discord.exe`, `Chrome.exe`) or broad categories (*Gaming*, *Social*).
- **Per-Weekday Overrides**: Schedule distinct quotas for weekends vs. weekdays (e.g. 1 hour Monday–Thursday, 3 hours Friday–Sunday).
- **24-Hour Cooldown Asymmetry**: Decreasing a quota or deleting a limit takes effect instantly. Increasing minutes enters a pending state that unlocks only after 24 hours.
- **Argon2id Vault Protection**: Secure PIN gate hashing your master passcode with salted Argon2id. Single-use cryptographic recovery code provided during vault setup in case the PIN is forgotten.

### 🎮 Hardware Multiplane Overlay (MPO) Gaming HUD
- **Zero FPS Drop in 3D Games**: Rendered via DirectComposition and DirectX 11 flip swapchain (`DXGI_SWAP_EFFECT_FLIP_DISCARD`), keeping full-screen DirectX 11/12 and Vulkan games in **Hardware Independent Flip (iFlip / DirectFlip)**.
- **Driver Limiter & VRR Compatibility**: GPU driver-level frame limiters (AMD Radeon Chill, AMD FRTC, Nvidia Max Frame Rate) and Variable Refresh Rate (AMD FreeSync, Nvidia G-Sync) remain 100% active and uncompromised.
- **Hit-Test Transparency (`HTTRANSPARENT`)**: Cursor hover and mouse clicks pass directly through to the underlying game or application. No cursor lag or wait/loading spinner artifacts.
- **Universal On-Demand Peek (`Ctrl+Alt+T`)**: A global keyboard shortcut temporarily reveals your remaining time for 4 seconds on demand across any app or game—even when the continuous HUD is turned off.
- **Milestone Harmonic Audio Alerts**: Subtle, high-fidelity chimes notify you when crossing 15m, 10m, 5m, and 1m thresholds without breaking immersion.

### 🚫 Web Filtering & DNS Interception
- **System-Wide DNS Proxy**: Embedded DNS daemon listening on `127.0.0.1:53` with upstream resolution over ephemeral client sockets.
- **Curated Blocklists**: One-click subscription to community adult and social media blocklists.
- **Encrypted DNS Interception**: Proactively blocks known DoH / DoT endpoints to prevent browser-level DNS bypasses.
- **Custom Domain Rules**: Add custom allow/block rules with automatic wildcard subdomain coverage (`*.example.com`).

### 🎨 Smoked-Glass Analytics UI
- **Tauri 2 Native Desktop Shell**: Low memory footprint (~35 MB RAM), hardware-accelerated rendering, frameless native window with custom title bar.
- **Obsidian & Glass Aesthetics**: Fine-tuned dark theme, classic light theme, and automatic system theme following Windows OS preferences.
- **Durable File Store Persistence**: Synchronous DOM bootstrapping combined with asynchronous Tauri file store (`theme.txt`) guarantees zero flash-of-unstyled-content (FOUC) and persistent theme retention across restarts.
- **Full Internationalization (i18n)**: 100% complete localization in English (`en`) and Arabic (`ar`, with proper RTL layout alignment).

---

## Architecture & Privilege Model

On modern Windows, no single process can securely enforce limits, sample user windows, and display an unprivileged UI. Windows enforces strict **Session 0 Isolation**: services running as `NT AUTHORITY\SYSTEM` cannot interact with user desktops, while unprivileged user processes cannot freeze protected processes or bind port 53.

Tether solves this with **three concurrent cooperating processes**:

```
+-----------------------------------------------------------------------------------+
| 1. screentime-agent (SYSTEM Windows Service)                                      |
|    - Path: %ProgramFiles%\Tether\bin\screentime-agent.exe                         |
|    - Owns: SQLite Database, Limits Engine, Process Freeze/Thaw, DNS Proxy (:53)   |
|    - IPC Server: \\.\pipe\screentime (Named Pipe, Access Controlled)              |
+------------------------------------------^----------------------------------------+
                                           |  (1 Hz ReportUsage & Status IPC)
+------------------------------------------v----------------------------------------+
| 2. screentime-session (Per-User Logon Helper)                                     |
|    - Path: %ProgramFiles%\Tether\bin\screentime-session.exe (HKCU Run Autostart)  |
|    - Owns: 10 Hz Foreground Window & Idle Sampling, Win32 MPO DirectComposition   |
|            Gaming HUD, Low-Level Peek Hotkey Thread (Ctrl+Alt+T)                  |
|    - Bridges: \\.\pipe\screentime_overlay_bridge                                  |
+------------------------------------------^----------------------------------------+
                                           |  (Hardware-Accelerated Block Commands)
+------------------------------------------v----------------------------------------+
| 3. screentime-ui (Tauri 2 Desktop Shell)                                          |
|    - Path: %ProgramFiles%\Tether\Tether.exe                                       |
|    - Owns: React 18 Analytics Dashboard, Settings Panels, Webview Block Overlay   |
+-----------------------------------------------------------------------------------+
```

---

## Hardware Multiplane Overlay (MPO) Gaming HUD

Traditional overlay applications (Discord, RTSS, Steam) inject DLL hooks into game processes or render standard Win32 topmost GDI windows. Topmost GDI windows force the Desktop Window Manager (DWM) to demote full-screen 3D games from **Hardware Independent Flip** to **Composed Flip**, which:
1. Adds 1–2 frames of input latency.
2. Disengages driver-level frame limiters (such as AMD Radeon Chill or Nvidia Max Frame Rate).
3. Causes stutter with Variable Refresh Rate (FreeSync / G-Sync).

Tether's HUD (`crates/session/src/mpo.rs`) uses **DirectComposition Hardware Multiplane Overlays**:
- The window is created with `WS_EX_NOREDIRECTIONBITMAP` (`0x00200000`), completely bypassing GDI CPU redirection surfaces.
- Renders via Direct2D into an independent DirectX 11 swapchain with `DXGI_SWAP_EFFECT_FLIP_DISCARD` and `DXGI_ALPHA_MODE_PREMULTIPLIED`.
- The GPU display controller hardware composites the HUD plane over the game plane at scanout time, keeping the game in 100% uncompromised Independent Flip.
- Handled with `WM_NCHITTEST -> HTTRANSPARENT`: all mouse clicks, drag events, and game cursors pass through seamlessly without interruption.

---

## Hardware-Accelerated Block Overlay

When daily limits or scheduled downtime expires, Tether locks the application:
1. **Native Win32 Input Invalidation**: `screentime-session` immediately calls `EnableWindow(target_hwnd, FALSE)`, rendering the blocked application window inert to mouse, keyboard, and focus events.
2. **React Hardware Overlay**: Dispatches a `Show` message across `\\.\pipe\screentime_overlay_bridge` to the secondary transparent Tauri webview window.
3. **Immersive Obsidian Card**: Renders a blurred backdrop (`backdrop-filter: blur(24px)`) and a centered obsidian card displaying the application name, category accent, and limit reason.
4. **PIN Pad & Extension**: Enter your PIN using the physical keyboard or on-screen keypad to claim a `+15 MIN` extension, or click `Quit App` to cleanly close the process tree.

---

## Installation for Testers

### Automated NSIS Installer (Recommended)

1. Download the latest installer executable:
   ```text
   Tether_0.2.0-beta_x64-setup.exe
   ```
2. Run the installer with administrator privileges (required to register the background Windows service).
3. The installer automatically:
   - Copies binaries to `%ProgramFiles%\Tether\`.
   - Registers and starts the `ScreentimeAgent` Windows Service (`sc.exe start ScreentimeAgent`).
   - Configures `screentime-session.exe` in `HKCU\Software\Microsoft\Windows\CurrentVersion\Run`.
   - Creates a Start Menu shortcut and launches Tether.

### Clean Uninstallation
Run `Uninstall Tether.exe` from Windows Settings (Apps & Features) or the installation folder. The uninstaller cleanly stops and unregisters the `ScreentimeAgent` service, removes autostart entries, restores default network DNS settings, and removes application binaries.

---

## Development Guide

### Prerequisites

- **Operating System**: Windows 10 (Build 19041+) or Windows 11
- **Rust Toolchain**: 1.83+ (pinned in [`rust-toolchain.toml`](rust-toolchain.toml))
- **Build Tools**: Visual Studio 2022 with "Desktop development with C++"
- **Node.js**: Node 20 LTS + npm
- **WebView2 Runtime**: Pre-installed on Windows 11; install Evergreen Bootstrapper on Windows 10 if needed.

### Unified Dev Runner

The repository includes a PowerShell development orchestrator that sets up isolated local data directories, cleans up stale processes, builds backend binaries, starts the agent and session helper, and launches the Tauri Vite dev server:

```powershell
# Standard run (starts Agent -> waits for pipe -> starts Session -> starts Tauri UI)
npm run dev              # or: .\dev.ps1

# Headless backend (hides extra console windows)
.\dev.ps1 -Headless

# Backend only (run Agent and Session without launching the UI)
.\dev.ps1 -BackendOnly

# Fast boot (skip cargo build check and run debug binaries directly)
.\dev.ps1 -NoBuild
```

All local development database records and logs are isolated inside `./local/data/`, keeping your system `%ProgramData%` clean.

### Verification Gate & CI

All pull requests and commits must pass the strict verification gate (zero-warning policy enforced):

```powershell
# 1. Rust formatting check
cargo fmt --all -- --check

# 2. Rust Clippy (must pass with 0 warnings)
cargo clippy --workspace --all-targets -- -D warnings

# 3. Unit & Integration Test Suite (124+ tests)
cargo test --workspace

# 4. Frontend Typecheck & Build
cd ui
npx tsc --noEmit
npm run build
```

> **Note on Generated Types**: TypeScript DTO interfaces in `ui/src/types/generated/*.ts` are generated from Rust structs via `ts-rs`. Never hand-edit them. Run `cargo test -p st-ipc` to update them after altering any IPC structs.

### Building the Production Installer

To produce the standalone release setup executable:

```powershell
# Build release binaries, copy to src-tauri/bin, and build NSIS installer
.\build_installer.ps1
```

The resulting installer is output to:
```text
target\release\bundle\nsis\Tether_0.2.0-beta_x64-setup.exe
```

---

## CLI Reference

### `screentime-agent`
```text
screentime-agent [OPTIONS]

Options:
  (no args)         Run in foreground console mode (development)
  --service         Run under Windows Service Control Manager (SCM)
  --install         Register the ScreentimeAgent Windows Service (Elevated)
  --uninstall       Unregister and remove the Windows Service (Elevated)
  --reset-network   Emergency recovery: reset all DNS and hosts changes
```

### `screentime-session`
```text
screentime-session [OPTIONS]

Options:
  (no args)         Run foreground sampling loop and MPO overlay
  --autostart on    Register in HKCU Run for current user logon
  --autostart off   Remove HKCU Run registration
  --autostart status Check current autostart status
```

---

## Repository & Crate Layout

```text
├── crates/
│   ├── agent/             # screentime-agent: privileged daemon, SQLite, limits, DNS proxy, IPC server
│   ├── session/           # screentime-session: foreground sampler, DirectComposition MPO HUD, input hooks
│   ├── core/              # Pure domain logic: limit evaluation engine, clocks, observation models (no SQL/OS)
│   ├── storage/           # SQLite storage engine, schema migrations, day snapshots, audit log
│   ├── ipc/               # Named-pipe protocol, DTO definitions, ts-rs TypeScript export
│   ├── st-win32/          # Shared Win32 helpers, process paths, window geometry
│   ├── tracker-win/       # Win32 foreground window and GetLastInputInfo tracker
│   ├── tracker-linux/     # Linux X11/Wayland tracker (stub)
│   ├── enforce-win/       # Win32 NtSuspendProcess / ResumeProcess tree enforcement & hosts file writer
│   ├── enforce-linux/     # Linux cgroups/SIGSTOP enforcement (stub)
│   └── dnsproxy/          # UDP DNS proxy on 127.0.0.1:53 with ephemeral upstream sockets
├── ui/
│   ├── src/               # React 18 application (Dashboard, Daily Timeline, Limits, Settings)
│   │   ├── components/    # Smoked-glass components, dialogs, charts, SVG icons
│   │   ├── hooks/         # React hooks (useLedger, useLimits, useTheme, useDowntime)
│   │   ├── locales/       # Internationalization dictionaries (en, ar)
│   │   └── styles/        # CSS design system (tokens.css, app.css, redesign.css)
│   └── src-tauri/         # Tauri 2 Rust desktop wrapper, IPC adapter, named pipe overlay bridge
├── docs/                  # Architectural documentation, ADRs, IPC catalog, Threat Model
├── dev.ps1                # Unified development orchestrator script
├── build_installer.ps1    # Production release packaging script
└── Cargo.toml             # Workspace definition and version manifest
```

---

## Security & Threat Model

Tether is designed as an **honest, high-friction self-control instrument**, not punitive corporate surveillance software. For a full breakdown of protection boundaries and tamper resistance, consult [docs/THREAT_MODEL.md](docs/THREAT_MODEL.md).

- **Standard Users**: Completely blocked from altering limits, tampering with the database, killing the background service, or altering hosts files.
- **Administrators**: If an elevated user chooses to open an Administrator PowerShell and execute `sc stop ScreentimeAgent`, they can do so. Tether intentionally does not load vulnerable kernel-mode filter drivers or rootkit hooks.
- **Fail-Closed Guarantee**: Service restarts or unexpected power loss will never grant unlimited usage; pending quotas wait out their full cooldown, and active blocks resume upon reboot.

---

## License

Tether is distributed under the [MIT License](LICENSE).
Contributions and issue reports are warmly welcomed.
