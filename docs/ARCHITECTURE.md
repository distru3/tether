# Architecture

Cross-platform screen-time tracker and limiter for Windows and Linux.

## Processes

Three binaries because the OS forces three privilege levels.

| Binary                | Runs as             | Owns                                                                                  |
| --------------------- | ------------------- | ------------------------------------------------------------------------------------- |
| `screentime-agent`    | Windows Service / systemd (SYSTEM/root) | Database, limits engine, day rollover, process freeze, DNS/hosts, PIN vault, audit log |
| `screentime-session`  | Per user login session, unprivileged     | Active-window sampling, idle detection, toast notifications, block overlay             |
| `screentime-ui`       | Interactive user process                 | Tauri dashboard: reports, category editor, limit editor                                |

The agent cannot see the interactive desktop (Session 0 isolation on Windows;
Wayland/X11 socket ownership on Linux), and only the agent has the privileges
required to change the hosts file, run WFP/nftables rules, freeze a process
tree or hold the database. Splitting them is not a stylistic choice.

## Rust workspace

    crates/
      core/            portable domain: types, limits engine, day rollover, clock guard
      storage/         SQLite schema and queries
      ipc/             wire protocol between UI, session helper and agent
      tracker-win/     Win32 foreground + idle
      tracker-linux/   X11 stub
      enforce-win/     process freeze/terminate + hosts writer
      enforce-linux/   cgroup freezer + hosts writer (stub)
      agent/           the SYSTEM/root binary
      session/         per-session helper (stub)
    ui/
      src/             React + TypeScript frontend
      src-tauri/       Tauri 2 host process

The four traits in `core::platform` (`WindowTracker`, `IdleMonitor`,
`ProcessController`, `NetworkFilter`) are the entire operating-system surface
that the rest of the code sees. Adding macOS or dropping Linux is confined to
those four implementations.

## Data flow

    session helper ── active window + idle ──▶ agent ──▶ sampler ──▶ storage
                                                 │
                                                 ├── limits engine ──▶ block/warn
                                                 └── DNS / hosts filter
    UI ◀── DTOs over IPC ──────────────────── agent

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

## Milestones

See `README.md`.
