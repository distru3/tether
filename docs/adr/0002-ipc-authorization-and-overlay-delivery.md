# ADR 0002: Central IPC authorization, peer verification and self-healing block screen

Date: 2026-10-02
Status: Accepted

## Context

The 2026-09 audit found that enforcement could be bypassed by the supervised
user without administrator rights:

- The agent's named pipe admits every authenticated local user (it must: the
  UI and session helper run as that user). PIN checks lived inside individual
  handlers and several loosening requests had none. Most seriously,
  `SetSetting` wrote any key into the `settings` table that also stores
  `pin_hash`, and `Categorize`, schedule edits and the allowlist were
  unguarded. There was no limit on PIN guesses.
- Any local process could send `ReportUsage` (fake usage or focus) or squat
  the overlay bridge pipe.
- The block overlay was shown with one fire-and-forget message; Alt+F4, a UI
  restart or a lost message uncovered the blocked app, and a failed Quit hid
  the overlay anyway.

## Decision

1. **One authorization gate** (`crates/agent/src/ipc_server/auth.rs`) runs
   before dispatch and is the only place that decides whether a request needs
   the PIN. Direction matters: tightening is free, loosening needs the PIN
   (when one is configured). Settings go through `st_core::settings::
   SettingKey` (fixed key list, range validation, `is_loosening`).
2. **Brute-force throttle** (`st_core::pin::PinThrottle`): escalating lockout
   after 5 consecutive wrong credentials; empty credentials never count, so
   the UI can probe without a PIN and prompt only on `bad_pin`. Argon2 runs
   without the database lock.
3. **The UI does not duplicate the rules.** It sends the request without a
   PIN and opens the PIN gate only when the agent answers `bad_pin`
   (`useLedgerActions().guarded`).
4. **Pipe peer verification** by executable identity
   (`GetNamedPipe{Client,Server}ProcessId` → image path →
   `st_win32::peer_is_trusted`: expected file name in our own install
   directory, its parent or `bin/`). Usage reports are accepted only from the
   session helper; the overlay bridge checks both ends. An uninspectable peer
   is *allowed* (and logged): rejecting it would turn a transient lookup
   failure into lost tracking, which fails open.
5. **Self-healing overlay delivery**: the session re-sends `Show` every 2 s
   while a block holds (backoff while unacknowledged) and relaunches the UI if
   needed; `Show` is idempotent on the UI side. Quit hides the overlay only
   after the app actually closed. Quitting a *blocked* app needs no PIN;
   closing any other app does.

## Consequences

- New IPC requests that can loosen enforcement must add a rule to
  `authorize` and a test; handlers must not re-implement PIN checks.
- The PIN throttle is in memory: restarting the agent resets it. Restarting
  the agent needs administrator rights, which already defeats the model.
- Peer verification trusts file location. A user who can write to the
  install directory is an administrator, outside the threat model.
- The block screen can still be uncovered for up to ~2 s (one re-send
  interval) after the overlay is closed or the UI is killed.
- Not addressed: an administrator can stop the agent; the block screen is a
  window, not an input lock (no keyboard hook, no `EnableWindow`).
