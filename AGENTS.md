# AGENTS.md

Guidance and mandatory operational rules for AI agents working in this repository. Updated and verified on **2026-09-12**.

---

## 1. What this is

A high-performance screen-time tracker, limit enforcer, and web filter for Windows (Linux crates are stubs). **Three distinct processes run concurrently at runtime**:

1. `screentime-agent` (`crates/agent`) — privileged daemon (SYSTEM/elevated): SQLite database, limits engine, day rollover, native Windows hosts file enforcer, Cloudflare Family DNS adapter configurator, IPC server on named pipe `\\.\pipe\screentime`.
2. `screentime-session` (`crates/session`) — per-user unprivileged sampling front: tracks foreground window and idle state, reports 1 Hz samples via `ReportUsage` over a persistent pipe connection, and renders the Win32 GDI topmost click-pad block overlay window.
3. `screentime-ui` (`ui/`) — Tauri 2 desktop shell and React 18 dashboard: one-shot pipe commands only, styled as a smoked-glass analytics workspace.

The agent alone does NOT track usage (Session-0 isolation). Dev fallback: `SCREENTIME_SELF_SAMPLE=1` re-enables in-agent sampling.

---

## 2. Mandatory Non-Destructive Workspace Rules (STRICTLY ENFORCED)

Violating these rules causes irreversible loss of user work and is strictly prohibited:

1. **NO BLIND ROLLBACKS OR RESETS**:
   - **NEVER** run `git checkout .`, `git restore .`, `git reset --hard`, or `git clean -fd` on a workspace with uncommitted or untracked changes.
   - If a rollback or cleanup is requested, you **MUST first create an explicit safety branch or backup commit**:
     ```powershell
     git branch safety-backup-$(Get-Date -Format 'yyyyMMdd-HHmmss')
     git add -A
     git commit -m "safety snapshot before revert"
     ```
2. **AUDIT UNTRACKED ASSETS BEFORE TOUCHING GIT**:
   - Always inspect untracked files with `git status -u`. Files like `redesign.css`, `categoryColors.ts`, or new component experiments are often untracked. A standard `git checkout .` will not restore them, and `git clean` will permanently destroy them.
3. **ZERO BLIND REWRITING / NO GUESSING**:
   - If an uncommitted file or diff was altered or lost, **NEVER attempt to hallucinate, reconstruct, or blindly guess its contents from old memory or templates**.
   - Stop immediately, state honestly what is known vs. unknown, and ask the user for guidance or clarification.
4. **NO CASCADING PATCH SCRIPTS**:
   - Avoid executing multi-pass ad-hoc string-replacement scripts on core source files. Verify exact line boundaries and file contents before making edits.
5. **MANDATORY DOWNSTREAM IMPACT AUDIT**:
   - Whenever changing a feature, setting, theme, or user-facing identifier in one part of the app, you **MUST systematically locate and update all downstream references** across the entire workspace:
     - Localization bundles (`ui/src/locales/en/translation.json`, `ui/src/locales/ar/translation.json`, etc.)
     - Component fallbacks and labels (`ui/src/App.tsx`, pages, controls)
     - Native platform renderers and fallback palettes (`crates/session/src/hud.rs`, `crates/session/src/mpo.rs`)
     - Styles and CSS tokens (`ui/src/styles/tokens.css`, `redesign.css`)
     - Documentation (`docs/UI_SPECIFICATION.md`, release notes, user guides)
   - Never leave downstream files stale when introducing or renaming functionality.

---

## 3. Anti-Hallucination Protocol

1. **Inspect Before Asserting**: Always read the target file with `view_file` or check with `grep_search` before claiming a function, component, or IPC route exists or does not exist.
2. **Single Source of Truth**:
   - IPC DTOs: Always verify against `ui/src/types/generated/*.ts` (or `crates/ipc/src/lib.rs`).
   - Tauri Commands: Always check `ui/src-tauri/src/lib.rs` and verify registration in `tauri::generate_handler![]`.
   - UI Design & Classes: Always reference `docs/UI_SPECIFICATION.md` and `ui/src/styles/redesign.css`.
3. **Reference On-Demand Documentation**:
   - Full Architecture & Data Flows: [`docs/SYSTEM_MAP.md`](docs/SYSTEM_MAP.md)
   - UI Redesign & Component Trees: [`docs/UI_SPECIFICATION.md`](docs/UI_SPECIFICATION.md)
   - IPC Contracts & Tauri Bridge: [`docs/IPC_CATALOG.md`](docs/IPC_CATALOG.md)

---

## 4. Development Commands

```powershell
# Unified dev runner (starts Agent -> waits for pipe -> starts Session -> starts Tauri UI)
./dev.ps1               # or: npm run dev
./dev.ps1 -Headless     # run backend hidden with no extra console windows
./dev.ps1 -BackendOnly  # run only Agent + Session without the Tauri UI
./dev.ps1 -NoBuild      # fast-boot: skip cargo build check and run debug binaries directly

# Manual dev run order (if running in 3 separate terminals):
$env:SCREENTIME_DATA_DIR = "$PWD\local\data"   # else defaults to C:\ProgramData\screentime
cargo run -p st-agent
cargo run -p st-session
cd ui && npm run tauri dev
```

# Verification gate (CI runs exactly this order)
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings   # zero-warning policy enforced
cargo test --workspace                                   # ~190 tests

# Single crate / single test
cargo test -p st-agent
cargo test -p st-agent recovery_code        # name substring filter

# Frontend typecheck/build (run from ui/)
npx tsc --noEmit
npm run build
```

CLI extras: `screentime-agent --service|--install|--uninstall` (SCM mode; install/uninstall need elevation) and `screentime-session --autostart on|off|status` (HKCU Run). Logs: agent `{data_dir}\logs`, session `%LOCALAPPDATA%\screentime\logs` — check these before stdout when debugging.

---

## 5. Regenerated Code

`ui/src/types/generated/*.ts` is **generated by a test**: `cargo test -p st-ipc` runs `export_ts_bindings` and rewrites those files from the Rust DTOs (ts-rs). Never hand-edit them; after changing any `st-ipc` DTO, rerun that test and commit the new `.ts` output.

---

## 6. Critical Architectural Invariants

- **`st-core` is pure domain logic**: **No OS APIs, no wall-clock reads, no SQL**. Time comes from an injected `Clock` (`TestClock` in tests); OS surface = four traits in `core::platform`.
- **Enforcement is fail-closed across restarts**: Blocks persist end-of-local-day expiries (`DayKey::end_utc`); startup never wipes blocks; expiry thaw happens per tick. The enforcer evaluates only the currently-focused app (fresh ≤30s session reports).
- **Anti-impulse cooldown asymmetry**: Loosening *minutes* queues into `pending_limits` (+24h default); **removing or disabling a limit applies instantly** by owner decision.
- **`ui/src-tauri` is a pure IPC adapter**: It serializes `st-ipc` DTOs verbatim (**snake_case wire format**) and returns structured `{code, message}` errors — no business logic there.
- **The block overlay is hand-painted GDI in the session crate**: Topmost, borderless window (`WS_EX_TOPMOST | WS_EX_NOACTIVATE`) with a low-level keyboard hook (`WH_KEYBOARD_LL`).
- **Session 1 Hz log quietness**: The session sampling front runs on a 1 Hz cycle. It must **never emit `INFO`-level logs for steady-state periodic flushes** (such as routine usage reports, keepalive ticks, or regular window focus changes). Routine per-second operations belong in `DEBUG`/`TRACE`, reserving `INFO` exclusively for process startup, link disconnects/reconnects, and explicit errors.
- **Web filtering & DNS architecture**: Domain blocking operates via native Windows hosts file enforcement (`st-enforce-win`), mapping blocked domains to `0.0.0.0` without requiring an active proxy listening on port 53. Network-wide adult and security protection is configured via Cloudflare Family DNS (`1.1.1.3` / `1.0.0.3`) at the adapter level with automatic previous DNS restoration on disable/uninstall.

---

## 7. Mandatory Documentation Updates (Context Preservation)

- **Continuous Documentation**: Each new feature, architectural adjustment, styling overhaul, or bugfix **must be recorded in the relevant document in `docs/`** (e.g. `docs/UI_SPECIFICATION.md`, `docs/SYSTEM_MAP.md`, `docs/ARCHITECTURE.md`, or dedicated architecture decision records in `docs/adr/`).
- **Context Preservation**: Agents must never leave the repository's documentation stale after modifying code or UI structures. Every session must leave behind an updated, truthful record of changes made, ensuring zero loss of operational context across agent turns.

