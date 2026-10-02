# ADR 0003: The service relaunches the session helper

Date: 2026-10-02
Status: Accepted (relaunch). Open: fail-closed fallback.

## Context

Only the per-user session helper (`screentime-session.exe`) can see the
foreground window, so it is the agent's only source of focus reports and
the only thing that drives the block overlay. The agent enforces nothing for
an app it has no fresh (≤30 s) focus report for.

The helper runs unprivileged in the user's session, so the supervised user
can end it in Task Manager. Before this decision nothing restarted it; the
HKCU Run entry starts it only at the next sign-in. Release builds also use
`panic = "abort"`, so any panic in the helper ended it the same way (the HUD
panic paths were removed separately). Either way enforcement silently
stopped: a fail-open hole.

## Decision

When running as the Windows service, the agent relaunches the helper in the
console user's session after reports stop:

- **Liveness signal**: the helper sends `ReportUsage` every second, empty
  batches included, and only a trusted peer (ADR 0002) may send it. No report
  for `SILENCE_SECS` (15 s) means the helper is gone.
- **Policy** (`crates/agent/src/supervisor.rs`, pure and unit-tested): no
  attempt during the first 60 s after agent start (sign-in starts the helper
  itself); then launch, retrying after 10 s, doubling up to 5 min; any report
  resets the schedule.
- **Mechanism** (`st_win32::session_launch`): `WTSGetActiveConsoleSessionId`
  → `WTSQueryUserToken` → `CreateProcessAsUserW` on `winsta0\default` with
  the user's environment block. The executable is looked up in the same
  directories the peer-trust check accepts (agent dir, `bin\`, parent).
  `WTSQueryUserToken` needs `SeTcbPrivilege`, so this only works as
  LocalSystem; console-mode (dev) agents don't supervise.
- The helper is single-instance per user, so a launch racing a helper that is
  still connecting just exits.

## Consequences

- Ending the helper buys at most ~15 s (plus backoff after repeated kills).
- The relaunch ignores `screentime-session --autostart off`: autostart is
  about sign-in, supervision is about enforcement.
- Only the console session is supervised. Remote Desktop sessions are not.
- **Open question for the owner**: should enforcement also fail closed while
  reports are stale (e.g. terminate processes of currently blocked apps with
  the agent's `ProcessController`)? Not implemented; it changes behavior for
  legitimate outages (helper crash loop, sign-out) and needs a decision.
- Needs a manual check on Windows: end `screentime-session.exe` in Task
  Manager and confirm it is back within ~15 s (agent log: "session helper
  was not reporting; relaunched it").
