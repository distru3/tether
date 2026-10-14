# Threat model

What Tether enforces and what it doesn't, stated plainly. A screen-time app
that overstates its protection is worse than one that says nothing: people
trust it and are then surprised. Only describe protections that exist in the
code; plans belong in an ADR.

## Who uses it

- **Me** (`profile = self`): someone limiting their own use. They are their
  own adversary and want friction, not a wall.
- **Someone I look after** (`profile = guardian`): a parent sets limits for a
  child. Enforcement is only close to "can't be bypassed" when the child uses
  a standard (non-administrator) Windows account.

## What is protected

- The database (usage, limits, settings), block state, the PIN hash
  (Argon2id) and recovery-code hash, the hosts file, network DNS settings and
  the audit log. All live in a SYSTEM-owned directory or OS locations a
  standard user cannot write.

## What Tether does

- **One authorization gate** (`crates/agent/src/ipc_server/auth.rs`) in front
  of every request. With a PIN set, anything that loosens enforcement needs
  it: raising or removing limits, extra time, closing an unblocked app,
  unblocking sites, recategorising apps, editing, disabling or deleting
  schedules, allowlisting, loosening settings, and unwatching an account. `SetSetting` accepts only
  known keys, so the PIN hash cannot be overwritten through it. Changing the
  PIN needs the old PIN or the recovery code.
- **Brute-force throttle**: after 5 wrong PINs or recovery codes in a row,
  checks lock for 30 s, doubling up to 15 min. Rejections are audited.
- **Cooldown**: raising a limit waits `limit_cooldown_hours` (24 by default).
  Lowering, adding, switching off and deleting apply at once.
- **Fail-closed blocks**: a block stores its own expiry, so restarting the
  agent or the PC lifts nothing.
- **The block screen heals itself**: the session helper re-shows it every 2 s
  while a block holds and relaunches the UI if needed, so closing the window
  or killing the UI uncovers the app for a few seconds at most.
- **The helper is restarted**: the service relaunches a session helper that
  stops reporting for 15 s (ADR 0003).
- **Pipe peers are checked**: usage reports are accepted only from Tether's
  own helper, and the block-screen bridge checks both ends are Tether binaries
  from the same install, so a local script can't fake usage or swallow the
  block screen.
- **Clock checks**: wall time is compared with monotonic time; a backward
  jump is logged, audited and not credited.
- **Hosts writes** stay between Tether's markers, so the person's own entries
  survive, and are written atomically.
- **Per-account counting** (ADR 0005): the agent reads each connection's
  Windows account from the process token, so a report cannot claim another
  account. Usage and blocks are per account; stopping limits on an account
  needs the PIN, and so does reading another account's history.
- **Audit log**: PIN set and removed, rejected credentials, overrides, backward
  clock jumps, accounts watched or unwatched.

## Known bypasses

Listed so the UI stays honest and regressions are visible.

- **An administrator** can stop the service, edit the hosts file, delete the
  database or uninstall Tether. Nothing restarts a stopped service. This is
  expected: for a child, use a standard account.
- **Ending the session helper** stops tracking and blocking until the service
  relaunches it (about 15 s).
- **Store apps** all report as one host process, so per-app limits on them
  don't work yet.
- **Hosts-file blocking** is defeated by a browser using its own encrypted DNS.
  Family DNS turns that off through browser policies and blocks DoT on port
  853, but a browser that ignores policies, a VPN or a portable browser with
  its own resolver gets around it.
- **Browser time**: a site cannot have a time budget; time in a browser counts
  as that browser.
- **Moving the clock forward** is not detected (it can bring the day reset
  closer). Sleep is never counted as use: gaps of more than 30 s between
  reports are not bridged.

## Out of scope

- A kernel driver or protected process.
- Cloud sync or a remote parent dashboard.

## What the UI must not do

- Claim a site is blocked while only the hosts file is active and the browser
  uses its own encrypted DNS.
- Claim limits are enforced while tracking is unavailable (the dashboard says
  "Tracking unavailable" when no reports arrive).
- Hide a failure: errors from the agent are shown, not swallowed.
