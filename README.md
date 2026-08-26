# screentime

Cross-platform screen-time tracker and limiter for Windows and (secondarily)
Linux/X11. Rust workspace, three binaries, Tauri 2 dashboard.

> Status: **M2 in progress**. M1 insight alpha is complete: auto-classification,
> real named-pipe IPC, and the live dashboard (see the checklist below). M2
> adds limits and app blocking: PIN vault, limit CRUD over IPC, and the
> enforcer that freezes exhausted apps. The limit editor, PIN setup and
> "+15 minutes" override are live in the UI.

## Repository layout

    crates/
      core/            portable domain logic, no OS or I/O
      storage/         SQLite schema + queries
      ipc/             wire protocol
      st-win32/        shared Win32 helpers (handles, image paths, wide strings)
      tracker-win/     Win32 foreground + idle (dev fallback sampler)
      tracker-linux/   X11 stub (M0 spike target)
      enforce-win/     process freeze + hosts writer
      enforce-linux/   stub
      agent/           screentime-agent (privileged)
      session/         screentime-session (per-user sampling front)
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

Development runs as two processes before the dashboard has anything to
show. The database lands in `.\local\data\`; set this once per shell:

    set SCREENTIME_DATA_DIR=%CD%\local\data
    set RUST_LOG=st_agent=debug,st_session=debug,st_core=debug,info

Terminal 1 — the privileged agent (database, limits engine, enforcement,
IPC server):

    cargo run -p st-agent

Terminal 2 — the unprivileged session helper. Session 0 isolation means the
agent cannot see the desktop; the session helper samples the foreground
window and idle time and feeds them to the agent over the named pipe:

    cargo run -p st-session

Then the Tauri dashboard (spawns `npm run dev` and rebuilds the Rust host as
needed):

    cd ui
    npm install
    npm run tauri dev

## Installing (service + autostart)

For daily use, run the agent as a Windows service and start the session
helper at logon:

    # Elevated prompt:
    packaging\install.ps1                    # end-to-end: build, service, autostart

    # Or step by step:
    target\release\screentime-agent.exe --install     # registers + starts via sc.exe
    target\debug\screentime-session.exe --autostart on  # HKCU Run key, no elevation

`--uninstall` / `--autostart off` reverse each. Logs live in
`C:\ProgramData\screentime\logs\` (agent) and `%LOCALAPPDATA%\screentime\logs`
(session). See `packaging/README.md` for the bundled-installer story.

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
- [x] Manual smoke: verify `NtSuspendProcess` freezes and thaws a process
      (validated 2026-08-21: a plain single-process Win32 window app froze —
      heartbeat stopped — and resumed on thaw)
- [x] Manual smoke: verify hosts writer round-trips a rule without disturbing
      an existing `127.0.0.1 localhost` line
      (validated 2026-08-21 against a throwaway temp hosts file)

## Verification checklist (M1)

- [x] Apps auto-classify into categories on first sight (curated signature
      table; never overwrites a user decision)
- [x] Named-pipe transport: UI talks to the agent over `\\.\pipe\screentime`;
      `agent_connected` reflects reality
- [x] Dashboard shows per-app and per-category usage for today, polling every 3s
- [x] Live `DaySummary` round trip validated against a seeded database

## Verification checklist (M2)

- [x] PIN vault: Argon2id hashing; limit changes allowed before a PIN is set,
      PIN required once configured
- [x] Limit CRUD over IPC: `Catalog`, `SetLimit`, `DeleteLimit`, `GrantOverride`
- [x] Anti-impulse cooldown: loosening applies after `limit_cooldown_hours`
      (default 24h), tightening immediately; pending changes promote on time
- [x] Enforcer: exhausted limits freeze the process tree; overrides thaw live;
      day rollover thaws everything; NeverBlock categories never freeze
      (covered by unit tests with a fake `ProcessController`)
- [x] UI: limit editor, PIN setup, "+15 minutes" override, blocked banner

## Refactor 2026-08

A structural pass after M2; the checklists above remain accurate as written.

- `screentime-session` is now the sampling front: it reads foreground window +
  idle locally and reports them to the agent over a persistent pipe connection
  (`ReportUsage`). In-agent sampling remains only as a dev fallback
  (`SCREENTIME_SELF_SAMPLE=1`).
- Enforcement and hosts writes are fail-closed: blocks persist their
  end-of-day expiry across agent restarts, and the hosts writer aborts on
  read failure instead of wiping unmanaged lines.
- `crates/storage` split into modules (migrations, taxonomy, usage, limits,
  enforcement, settings, reporting); new shared `crates/st-win32`.
- IPC DTOs generate TypeScript bindings into `ui/src/types/generated/`;
  errors cross the wire as structured `{code, message}`.
- Dashboard redesigned as a ledger-style light dashboard.

`cargo test --workspace` stands at 179 tests.

## License

MIT — see [LICENSE](LICENSE).
