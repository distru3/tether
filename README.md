# screentime

Cross-platform screen-time tracker and limiter for Windows and (secondarily)
Linux/X11. Rust workspace, three binaries, Tauri 2 dashboard.

> Status: **M1 in progress**. The M0 spike is complete on Windows: live
> tracking validated end-to-end (see the checklist below). Next up is the M1
> insight alpha — auto-classification, IPC, and the real dashboard.

## Repository layout

    crates/
      core/            portable domain logic, no OS or I/O
      storage/         SQLite schema + queries
      ipc/             wire protocol
      tracker-win/     Win32 foreground + idle
      tracker-linux/   X11 stub (M0 spike target)
      enforce-win/     process freeze + hosts writer
      enforce-linux/   stub
      agent/           screentime-agent (privileged)
      session/         screentime-session (per-user)
    ui/                Tauri 2 + React + Vite dashboard
    docs/              architecture and threat model
    packaging/         installers (empty for now)

See `docs/ARCHITECTURE.md` and `docs/THREAT_MODEL.md`.

## Prerequisites

* **Windows 10 or 11.**
* **Rust 1.83+** via [rustup](https://rustup.rs/). The toolchain is pinned in
  `rust-toolchain.toml`, so `cargo` picks the right version automatically.
* **Microsoft Visual Studio Build Tools 2022** with the "Desktop development
  with C++" workload (needed by `rustc` on Windows and by
  [Tauri prerequisites](https://tauri.app/start/prerequisites/#windows)).
* **Node.js 20+** and **npm** (you already have Node 24, that's fine).
* **WebView2 runtime** (pre-installed on Windows 11; use the evergreen
  installer on Windows 10).

Install the Tauri CLI once you have Rust:

    cargo install tauri-cli --version "^2.1" --locked

## Building

Everything Rust:

    cargo build --workspace

Run the tests without an OS:

    cargo test --workspace

Run the agent in the foreground (development mode, database in
`.\local\data\`):

    set SCREENTIME_DATA_DIR=%CD%\local\data
    set RUST_LOG=st_agent=debug,st_core=debug,info
    cargo run -p st-agent

Run the Tauri dashboard (spawns `npm run dev` and rebuilds the Rust host as
needed):

    cd ui
    npm install
    npm run tauri dev

## Milestones

| M   | Scope                                                                  |
| --- | ---------------------------------------------------------------------- |
| M0  | Spikes: Win32 tracking + freeze + overlay, X11 tracking, DNS proxy     |
| M1  | Insight alpha: tracking, dashboard, categories, dogfood                |
| M2  | Limits & app blocking: agent service, IPC, PIN, freeze + overlay       |
| M3  | Web filtering: DNS proxy, DoH lockdown, adult-content blocklists       |
| M4  | Polish: downtime, focus sessions, weekly report, guardian mode, i18n   |
| M5  | Packaging, signing, release                                            |

Deliberately cut from v1: browser extension, multi-device sync, Wayland,
eBPF/fanotify, kernel driver.

## Verification checklist (M0)

- [x] `cargo build --workspace` on Windows with MSVC Build Tools installed
- [x] `cargo test --workspace` all green (67 tests)
- [x] `cargo run -p st-agent` writes intervals to `.\local\data\screentime.db`
      (validated 2026-08-21: Brave / Telegram / VS Code tracked, correct
      `day_key`, clean app-switch boundaries)
- [x] `ui: npm install && npm run tauri dev` opens the dashboard and shows
      "tracker_backend: win32"
- [ ] Manual smoke: verify `NtSuspendProcess` freezes and thaws a Notepad PID
      taken from Task Manager (throwaway script, not committed)
- [ ] Manual smoke: verify hosts writer round-trips a rule without disturbing
      an existing `127.0.0.1 localhost` line

## License

MIT — see [LICENSE](LICENSE).
