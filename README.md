# Tether

> A private-by-default screen-time tracker and limiter for Windows. See where your
> time actually goes, then put a hard stop on the apps and sites that steal it.

[![CI](https://github.com/distru3/tether/actions/workflows/ci.yml/badge.svg)](https://github.com/distru3/tether/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/rust-1.83%2B-orange.svg)](rust-toolchain.toml)

Tether is a self-control aid, not spyware. Everything is stored in a local SQLite
database on your own machine — nothing is uploaded, nothing is cloud-synced. It is
designed to be friction *you* opt into, and it is honest about what it can and
cannot enforce (see [docs/THREAT_MODEL.md](docs/THREAT_MODEL.md)).

> **Naming:** "Tether" is the product name. The Rust crates and binaries still use
> the historical `screentime-*` prefix (`screentime-agent`, `screentime-session`,
> `screentime-ui`) — those are implementation details, not user-facing names.

## What it does

| Capability | How it works |
| --- | --- |
| **Automatic tracking** | Samples the foreground window + idle state locally, once a second |
| **Smart categorization** | Apps auto-classify into categories (manual override + tags supported) |
| **Daily limits** | Per-app and per-category budgets, with optional per-weekday minutes |
| **Freeze, don't kill** | Limits suspend the process tree instead of killing it, so your work survives |
| **PIN + recovery code** | Limit changes are gated behind a PIN (Argon2id-hashed), with a recovery code |
| **Anti-impulse cooldown** | Loosening a limit waits 24 h; tightening or removing it is instant |
| **Web filtering** | Hosts-based blocking with adult + social-media blocklists and manual domains |
| **Overrides** | A PIN-gated "+15 minutes" escape hatch, with fail-closed expiry |
| **Fail-closed enforcement** | Blocks survive restarts until end-of-day; expired blocks thaw automatically |

## Why Tether

Most "focus" apps are either walled SaaS gardens or blunt website blockers.
Tether is neither: it's a local, single-machine tool that tracks *applications*
(not just browser tabs), enforces limits by freezing processes so no work is
lost, and treats the user as a grown-up — overrides are a feature, and the
threat model says plainly where the limits of enforcement are.

## Getting started

### Prerequisites

- **Windows 10 or 11**
- **Rust 1.83+** (via [rustup](https://rustup.rs); the toolchain is pinned in
  [`rust-toolchain.toml`](rust-toolchain.toml))
- **Visual Studio Build Tools 2022** with the "Desktop development with C++" workload
- **Node.js 20+** and **npm**
- **WebView2 runtime** (preinstalled on Windows 11)

Install the Tauri CLI once:

```powershell
cargo install tauri-cli --version "^2.1" --locked
```

### Build & run

Tether is three processes; start them in this order:

```powershell
# 1. The privileged agent (database, limits engine, enforcement, IPC server)
$env:SCREENTIME_DATA_DIR = "$PWD\local\data"   # else defaults to C:\ProgramData\screentime
cargo run -p st-agent

# 2. The per-user session helper (samples the foreground window, drives the overlay)
cargo run -p st-session

# 3. The dashboard (spawns the Vite dev server)
cd ui
npm install
npm run tauri dev
```

## Installing for daily use

For everyday use, run the agent as a Windows service and start the session helper
at logon. From an **elevated** PowerShell, in the repository root:

```powershell
.\packaging\install.ps1                       # build, register service, enable autostart
```

Or step by step:

```powershell
# Elevated prompt — registers + starts the agent via sc.exe
target\release\screentime-agent.exe --install

# No elevation — writes the HKCU Run key
target\debug\screentime-session.exe --autostart on
```

Reverse with `--uninstall` and `--autostart off`. Logs live in
`C:\ProgramData\screentime\logs\` (agent) and `%LOCALAPPDATA%\screentime\logs`
(session). A bundled NSIS installer story is documented in
[packaging/README.md](packaging/README.md).

## Configuration

| Setting | Where | Description |
| --- | --- | --- |
| `SCREENTIME_DATA_DIR` | env var | Agent data directory; defaults to `C:\ProgramData\screentime` |
| `SCREENTIME_SELF_SAMPLE=1` | env var | Dev fallback: re-enable in-agent sampling (agent alone doesn't track usage) |
| `RUST_LOG` | env var | Log filter, e.g. `st_agent=debug,st_session=debug,info` |

Agent flags:

```text
screentime-agent [FLAG]
  (no args)         run in console mode (foreground)
  --service         run under the Windows service controller (SCM only)
  --install         register the Windows service (elevated)
  --uninstall       remove the Windows service (elevated)
  --reset-network   undo all DNS/hosts/firewall changes and exit (emergency recovery)
```

Session flags:

```text
screentime-session --autostart on|off|status
```

## How it works

Three binaries because Windows forces three privilege levels:

| Binary | Runs as | Owns |
| --- | --- | --- |
| `screentime-agent` | Windows service (SYSTEM) | SQLite, limits engine, enforcement, IPC on `\\.\pipe\screentime` |
| `screentime-session` | Per-user login session | Foreground-window + idle sampling, persistent pipe link, block overlay |
| `screentime-ui` | Interactive user | Tauri dashboard; one-shot pipe commands |

The agent cannot see the interactive desktop (Session 0 isolation), so the
session helper samples it locally and reports usage over a persistent named-pipe
connection. Full details:
- [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) — High-level design and privilege models
- [docs/SYSTEM_MAP.md](docs/SYSTEM_MAP.md) — End-to-end data flows and storage invariants
- [docs/UI_SPECIFICATION.md](docs/UI_SPECIFICATION.md) — Smoked-glass analytics UI and layout hierarchy
- [docs/IPC_CATALOG.md](docs/IPC_CATALOG.md) — Complete IPC named pipe request/response specification
- [docs/THREAT_MODEL.md](docs/THREAT_MODEL.md) — Threat model and bypass resistance

## Repository layout

```text
crates/
  core/            portable domain logic — no OS APIs, no wall clock, no SQL
  storage/         SQLite schema, migrations and queries
  ipc/             wire protocol + generated TypeScript bindings (ts-rs)
  st-win32/        shared Win32 helpers
  tracker-win/     Win32 foreground + idle sampling
  tracker-linux/   X11 stub
  enforce-win/     process freeze + hosts writer
  enforce-linux/   stub
  agent/           screentime-agent (privileged daemon)
  session/         screentime-session (per-user sampling front)
ui/                Tauri 2 + React + Vite dashboard
docs/              architecture, threat model, ADRs
packaging/         installer script + NSIS hooks
```

## Development

The verification gate (CI runs exactly this):

```powershell
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

Frontend typecheck and build (from `ui/`):

```powershell
npx tsc --noEmit
npm run build
```

TypeScript bindings under `ui/src/types/generated/` are generated by
`cargo test -p st-ipc` — never edit them by hand.

## Contributing

Pull requests are welcome. Open an issue to discuss larger changes first. CI
must pass (`fmt`, `clippy` with `-D warnings`, and `cargo test --workspace`) —
see [AGENTS.md](AGENTS.md) for the repo's working conventions and gotchas.

## License

[MIT](LICENSE)
