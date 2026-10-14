# Architecture

How Tether works today. This file describes the current code only; history
lives in git and in the decision records (`docs/adr/`). When code and this file
disagree, fix one of them in the same change.

Related: `docs/IPC_CATALOG.md` (every request), `docs/THREAT_MODEL.md` (what
is and isn't enforced), `docs/UI_SPECIFICATION.md` (screens),
`docs/DESIGN_SYSTEM.md` (look and voice), `packaging/README.md` (installer).

## 1. Three processes

Windows forces three privilege levels, so Tether is three binaries.

| Binary | Runs as | Owns |
|---|---|---|
| `screentime-agent` (`crates/agent`) | Windows service, SYSTEM | SQLite database, limits engine, day rollover, block state, PIN vault, hosts file, Family DNS, IPC server on `\\.\pipe\screentime` |
| `screentime-session` (`crates/session`) | One per signed-in user, unprivileged | Foreground-window and idle sampling, usage reports, the timer HUD, telling the UI when to show the block screen |
| `screentime-ui` (`ui/`, shown as Tether) | The signed-in user | Tauri 2 shell and React dashboard, the block screen window, the tray panel |

The agent runs in Session 0 and cannot see the desktop, so it cannot tell
which app is in front; only the session helper can. The UI never touches the
database: every action is a one-shot request to the agent. A dev fallback,
`SCREENTIME_SELF_SAMPLE=1`, lets the agent sample by itself.

```
 screentime-session ── ReportUsage (1 Hz, persistent pipe) ──▶ screentime-agent ──▶ SQLite
   │  ▲                                                          │
   │  └── Status, BlockedApps, HUD state ◀────────────────────────┤
   │                                                              ├── hosts file
   └── Show / Hide ──▶ screentime-ui (overlay window)             └── Family DNS
                         ▲
                         └── one-shot requests (Tauri commands) ──▶ agent
```

## 2. Workspace

```
crates/
  core/           Pure domain logic: models, day keys, limits engine, schedules,
                  categories, settings keys, PIN throttle, game heuristics.
                  No OS calls, no wall clock, no SQL (time comes from `Clock`).
  storage/        SQLite migrations and queries, one module per concern.
  ipc/            Wire protocol (Request/Response), pipe transport, ts-rs
                  export of the DTOs to ui/src/types/generated.
  st-win32/       Shared Win32 helpers: process paths, peer trust, long/short
                  paths, single-instance mutex, audio, launching in a user session.
  tracker-win/    Foreground window, idle (with audio-playback detection), app discovery.
  enforce-win/    Hosts-file writer; closing an app's processes on request.
  family-dns/     Family DNS adapter configuration, backup/restore, DoH/DoT lockdown.
  tracker-linux/, enforce-linux/   Stubs; see docs/adr/0004-linux-port.md.
  agent/          The service: IPC server, report ingestion, enforcer, policy,
                  Family DNS orchestration, session-helper supervision.
  session/        The per-user helper: sampling loop, HUD, block-screen driver.
ui/
  src/            React 18 + TypeScript dashboard.
  src-tauri/      Tauri 2 host: thin IPC adapter, overlay bridge, tray, tray panel.
```

`core::platform` defines the OS boundary: `WindowTracker`, `IdleMonitor`,
`ProcessController` (find and close an app's processes) and `NetworkFilter`.
Every SQL statement lives in `st-storage`; every enforcement decision in
`st-core::limits` and the agent's `enforcer.rs`.

## 3. Tracking

1. The session helper polls the foreground window about 10 times a second and
   idle state from `GetLastInputInfo`. Input-idle time does not count while an
   audio session is playing (films are not idle).
2. It resolves the window to an `AppKey` (executable path today; Store apps
   all report as `ApplicationFrameHost.exe`, a known gap) and batches
   observations.
3. About once a second it sends `ReportUsage` over its persistent pipe
   connection, with the currently focused app (`focused_key`).
4. The agent accepts reports only from Tether's own helper
   (`st_win32::peer_is_trusted`, comparing long-form paths), checks the clock
   (a backward jump is logged, audited and not credited), deduplicates,
   writes `usage_intervals` and the `usage_daily` rollup, and answers with the
   HUD state for the focused app.
5. **Per account** (ADR 0005): every connection's Windows account is read
   from the peer process's token (`st_win32::account::process_user`) and
   kept in `accounts`. Usage, blocks, extra time and block-screen answers
   belong to that account; budgets, schedules, websites and settings are
   shared. An account is *watched* (limits apply) unless someone with the
   PIN turned that off; new accounts are watched. The first account met
   adopted the usage recorded before accounts existed.
6. Days are local `YYYYMMDD` keys shifted by `day_start_minutes`
   (`DayKey`). The UI mirrors the same setting so "today" matches.

App discovery runs once per helper start: Start menu shortcuts, the uninstall
registry and Windows' GameConfigStore. The agent classifies new apps
(`classify.rs`: exact names, paths, publishers, name keywords) only while the
person hasn't categorised them.

Each app has one primary category (reports add up to 100 %) plus optional tag
categories that only matter for matching limits. Categories marked
`never_block` (development tools, system utilities, Tether itself) are never
blocked.

## 4. Limits and blocking

- **Limits** target an app, a category or total screen time, with a daily
  minute value and optional per-weekday values.
- **The enforcer** (`crates/agent/src/enforcer.rs`) runs every second on
  each account's focused app (focus reports older than 30 s are ignored): it
  lifts expired blocks, clears all blocks on a day change it observed itself,
  then asks the limits engine about every watched account, against that
  account's own usage. An unwatched account is never blocked. Order of checks: a downtime schedule in force
  (unless the app is always allowed), the app's budget, its categories, total
  screen time. Overlapping limits: the most restrictive wins.
- **A block** is a `block_state` row with an expiry (end of the local day, or
  the schedule's end). Restarting the agent never lifts it.
- **The block screen** is what enforces it. The session polls `BlockedApps`;
  when the focused app is blocked it sends `Show` over
  `\\.\pipe\screentime_overlay_bridge` and the UI covers the app with its
  always-on-top `overlay` window (`BlockOverlay.tsx`). The app keeps running
  behind it. The session re-sends `Show` every 2 s while the block holds and
  relaunches the UI if its pipe is gone, so closing the window or the UI
  heals within seconds. Both ends of the bridge verify the other is a genuine
  Tether binary. There is no keyboard hook and nothing is frozen.
- **Close** on the block screen ends the app (locally, then through the
  agent's `CloseApps`). **Extra time** (+15 minutes) is a PIN-approved
  override; never offered during a schedule or in strict mode.
- **Cooldown** (ADR 0006): in "Me" mode, raising a limit (or any weekday's
  value) queues in `pending_limits` for `limit_cooldown_hours` (24 by
  default). For someone looked after with a PIN set, the raise needed the
  PIN and applies at once. Lowering, adding, switching off and deleting
  always apply at once.
- **The timer HUD** (`crates/session/src/mpo.rs`, DirectComposition, with a
  GDI fallback in `hud.rs`) shows time left over apps with a budget. It hides
  over full-screen games unless `show_hud_in_fullscreen` is on; the peek
  shortcut and the 15/10/5/1-minute chimes show it for 4 s.

## 5. Authorization

The pipe admits every local user, so it proves nothing about who is asking.
Every request passes one gate, `crates/agent/src/ipc_server/auth.rs`, before
dispatch. Anything that loosens enforcement needs the PIN when one is set;
`SetSetting` accepts only `st_core::settings::SettingKey` keys with checked
values. Wrong PINs and recovery codes are throttled (5 free, then 30 s
doubling to 15 min). Details per request: `docs/IPC_CATALOG.md`.

## 6. Websites

- Blocked sites are written to the Windows hosts file between Tether's own
  markers (both IPv4 and IPv6 loopback), atomically; unmanaged lines are kept.
  Sites on the built-in adult list are hidden in the UI until the PIN is
  entered.
- **Family DNS** points every active adapter at Cloudflare's family resolvers
  (`1.1.1.3`, `1.0.0.3` and IPv6), after saving the previous DNS to the
  database and `{data_dir}/original_dns_backup.json`. It also sets browser
  policies that switch off their own encrypted DNS and blocks outbound DoT
  (port 853). Turning it off restores the saved DNS. One implementation,
  `crates/agent/src/family_dns.rs`, serves the app, the CLI and the
  uninstaller. `--reset-network` is the emergency undo.

## 7. Keeping the helper alive

Only the helper reports focus, so while it is gone nothing is enforced. The
service relaunches it in the console user's session after 15 s without
reports (`supervisor.rs`, with a startup grace period and backoff; ADR 0003).
The helper uses a per-session single-instance mutex. Release builds abort on
panic, so the helper must not panic.

## 8. Storage

SQLite in `C:\ProgramData\screentime` (or `SCREENTIME_DATA_DIR`), migrations
in `crates/storage/migrations/` tracked by `PRAGMA user_version`; never edit a
shipped migration, add a new one.

| Migration | Adds |
|---|---|
| 0001 | Base schema: categories, apps, tags, usage, limits, schedules, allowlist, block state, overrides, audit log, settings (and unused blocklist tables) |
| 0002 | `pending_limits` (cooldown queue) |
| 0003 | One total-screen-time limit at most |
| 0004 | Wall-clock expiry on overrides |
| 0005 | `block_reasons` (answers on the block screen) |
| 0006 | `accounts`; `account_id` on usage, blocks, overrides and block reasons |

The audit log records PIN set/removed, rejected credentials, overrides,
backward clock jumps and accounts being watched or unwatched.

Logs: agent in `{data_dir}\logs`, helper in `%LOCALAPPDATA%\screentime\logs`.
The helper never logs routine per-second work at INFO.

## 9. Packaging

A styled NSIS installer built by Tauri (`packaging/README.md`): installs to
`Program Files\Tether`, registers the service (re-pointing an existing
registration), adds one HKLM Run entry for the helper, starts tracking as the
signed-in user, and on uninstall offers to delete data and restore DNS.
