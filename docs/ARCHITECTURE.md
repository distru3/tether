# Architecture

Cross-platform screen-time tracker and limiter for Windows and Linux.

## Processes

Three binaries because the OS forces three privilege levels.

| Binary                | Runs as             | Owns                                                                                  |
| --------------------- | ------------------- | ------------------------------------------------------------------------------------- |
| `screentime-agent`    | Windows Service / systemd (SYSTEM/root) | Database, `ReportUsage` ingestion and classification, limits engine, day rollover, process freeze, DNS/hosts, PIN vault, audit log |
| `screentime-session`  | Per user login session, unprivileged     | The sampling front: foreground-window + idle sampling, persistent pipe connection to the agent, toast notifications, block overlay |
| `screentime-ui`       | Interactive user process                 | Tauri dashboard: reports, category editor, limit editor                                |

The agent cannot see the interactive desktop (Session 0 isolation on Windows;
Wayland/X11 socket ownership on Linux), and only the agent has the privileges
required to change the hosts file, run WFP/nftables rules, freeze a process
tree or hold the database. Splitting them is not a stylistic choice. It also
means the session helper samples the desktop itself — foreground window and
idle — and ships observations over the pipe; the agent-side sampler survives
only as a dev fallback (`SCREENTIME_SELF_SAMPLE=1`).

## Rust workspace

    crates/
      core/            portable domain: types, limits engine, day rollover, clock guard
      storage/         SQLite schema and queries, module per concern
      ipc/             wire protocol between UI, session helper and agent;
                       owns PIPE_NAME and the ts-rs TypeScript bindings
      st-win32/        shared Win32 helpers (process handles, image paths, wide strings)
      tracker-win/     Win32 foreground + idle (dev fallback sampler)
      tracker-linux/   X11 stub
      enforce-win/     process freeze/terminate + hosts writer
      enforce-linux/   cgroup freezer + hosts writer (stub)
      agent/           the SYSTEM/root binary: IPC server, ingestion, enforcement
      session/         the unprivileged sampling front
    ui/
      src/             React + TypeScript frontend
      src/types/generated/   IPC DTOs, generated from Rust via ts-rs
      src-tauri/       Tauri 2 host process

The four traits in `core::platform` (`WindowTracker`, `IdleMonitor`,
`ProcessController`, `NetworkFilter`) are the entire operating-system surface
that the rest of the code sees. Adding macOS or dropping Linux is confined to
those four implementations.

## Data flow

    session ── active window + idle, batched ──▶ agent ──▶ ingest/classify ──▶ storage
      ▲          (~1s ReportUsage cycles)         │
      ├── Status each cycle: liveness, PIN ◀──────┤
      └── BlockedApps on focus change ◀───────────┤
                                                  ├── limits engine ──▶ freeze/thaw
                                                  └── DNS / hosts filter
    UI ◀── DTOs over IPC ────────────────────────── agent

The session holds one persistent pipe connection, batching observations
through `ReportUsage` (~1 s cycles) with capped-exponential-backoff reconnects.
The same link carries `Status` every cycle — liveness for the agent, the PIN
gate for the overlay — and `BlockedApps` on focus change. One-shot clients,
including every Tauri command today, share the same thread-per-connection
server (cap 32).

Every OS-touching call goes through the four traits above; every SQL statement
lives in `st-storage`; every enforcement decision lives in `st-core::limits`.

## Time

Two clocks, deliberately. Wall time bucketises usage into local days; monotonic
time measures elapsed durations. Comparing the two catches clock rollback and
suspend gaps in the same place. Days are keyed as `YYYYMMDD` local integers,
computed with a configurable day-start offset (default midnight, 04:00 is the
recommended alternative for night owls). Every time-related invariant is
covered by unit tests using a `TestClock`; that is how DST, timezone changes
and clock tampering are tested without a VM.

## Categories

Each application has exactly one **primary category**, used for reports and
budget attribution — this is what keeps a "time by category" chart summing to
100%. It also carries zero or more **tags**, additional categories used *only*
for matching limits. That is how TikTok can count against both a "Social Media"
budget and a "Short-Form Video" budget without being double-counted in the
report. Overlapping limits resolve as *most-restrictive-wins*.

`Development & Tools` and `Utilities & System` are marked `NeverBlock` in the
built-in taxonomy: locking someone out of their terminal, editor or file
manager is worse than any limit is good.

## Enforcement tiers

Documented in `docs/THREAT_MODEL.md`. The short version: this is a self-control
aid, not a bypass-proof lock. Freeze-instead-of-kill preserves the user's work;
the hosts writer is honest about being defeated by DNS-over-HTTPS; the DNS
proxy and browser policies in M3 are what make filtering meaningful; a signed
kernel driver is explicitly out of scope.

Expiry is fail-closed. A block carries its end-of-local-day deadline (`DayKey`,
honouring timezone and `day_start_minutes`), so restarting the agent lifts
nothing; expired blocks thaw on tick and overrides land on the reporting user's
local day. The hosts writer aborts on read failure rather than wipe unmanaged
lines, and writes atomically via temp file + rename.

## Milestones

See `README.md`.
