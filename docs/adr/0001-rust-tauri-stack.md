# ADR 0001: Rust + Tauri as the primary stack

Date: 2026-08-20
Status: Accepted

## Context

Cross-platform desktop app with three privilege levels: a privileged agent, a
per-session helper and a UI dashboard. Needs direct access to Win32 (foreground
window, `NtSuspendProcess`, WFP), X11, cgroups, DNS/nftables, and a first-class
SQLite integration. Ships to end users who install once and update.

## Options considered

1. **Rust + Tauri 2, React frontend.** One Rust workspace shared by the agent,
   session helper and UI backend. Tauri gives a small (~10 MB) native shell
   using the system WebView. Direct OS access through `windows`, `x11rb`,
   `nix`, no FFI shims.
2. **Electron + Node.** Best UI DX. But a Node runtime is a poor host for
   suspending processes, editing hosts, or running as a Windows Service. Would
   still need a native sidecar for the agent, i.e. two languages anyway.
   Bundle size ~150 MB per app.
3. **.NET + Avalonia.** Strong on Windows, viable on Linux. P/Invoke for Win32
   is fine; cgroup and nftables integration less so. Adds a large runtime for
   Linux users.
4. **Python + PySide6.** Fastest for a prototype. Worst enforcement story,
   worst packaging, worst startup time. Fine for spikes only.

## Decision

Option 1: Rust + Tauri 2 with a React + TypeScript frontend.

## Consequences

* **Positive.** One language for all three binaries. Compile-time guarantees at
  the OS boundary. Small installers. `windows`, `x11rb`, `nix`, `rusqlite` are
  best-in-class. `tauri-build` handles the bundling story for both platforms.
* **Negative.** Rust learning curve for a developer new to it; Tauri 2's
  capability model is different from Tauri 1 and Electron and needs a
  half-day's worth of reading before it clicks.
* **Mitigations.** The four `st-core::platform` traits confine the risky OS
  code. `#[tauri::command]` handlers stay thin: any command longer than a
  handful of lines is a signal that the logic belongs in the agent instead.
