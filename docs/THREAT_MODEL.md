# Threat model

Written up front because a screen-time app that overstates what it enforces is
worse than one that says nothing: users will trust it and then be surprised.

## Users and adversaries

* **Self-controller.** The user *is* the adversary and knows it. Wants friction,
  not a wall.
* **Guardian mode.** The user runs an admin account, the person being managed
  runs a standard account. This is the only configuration in which "cannot be
  bypassed" is close to true.
* **Casual bypass on a shared machine.** Admin credentials not available, so
  service tampering and hosts edits are out of reach.

## Assets

* The SQLite database (usage history and configured limits).
* Active limits and current day-key state.
* The PIN hash (Argon2id).
* The hosts file / DNS proxy configuration.
* The audit log.

## In scope

* Reversible enforcement: freeze rather than kill. Grace countdown before any
  destructive action.
* Anti-impulse **cooldown** on loosening limits (24 h by default). Tightening
  takes effect immediately.
* Clock-tamper detection: wall time cross-checked against monotonic time; a
  detected rollback is logged and does not credit the interval.
* Sleep-gap detection: same mechanism, opposite direction. Time under
  `Suspend`/`Hibernate` is not screen time.
* Marker-delimited hosts writes so user entries survive.
* DB and config in a SYSTEM/root-owned directory with restrictive ACLs.
* PIN gate on grants, PIN change requires the old PIN.
* Audit log for overrides, limit edits, service stops, hosts/DNS tampering,
  clock jumps.

## Out of scope

* Kernel driver or Protected Process Light (would need EV signing, WHQL and a
  much larger threat model).
* Making enforcement bulletproof against a determined administrator on their
  own machine. This is a self-control aid; the docs say so, the onboarding says
  so, and the UI says so when running in a configuration where blocking is
  advisory.
* Wayland support in v1.
* Cloud sync, remote guardian dashboards.

## Known bypasses (v1)

Documented here so the UI can be honest about them and so that regressions can
be spotted:

* **Hosts-only mode** is defeated by any browser with DNS-over-HTTPS enabled
  (Chrome, Edge, Firefox default in many regions). This is why the M3 DNS
  proxy also (a) installs enterprise policies disabling DoH, (b) firewalls TCP
  and UDP 853 and 53 to non-local resolvers, and (c) blocks known DoH endpoint
  domains.
* **VPNs and portable browsers** bypass everything short of a kernel filter.
  Detect a new tunnel interface and surface a warning; do not claim the site
  is blocked when it isn't.
* **Deleting the database.** Prevented by ACLs *only* when the user is not an
  admin. On a self-controller's own machine, this is possible. The audit log
  records it, and reinstalling starts from a clean slate — no silent recovery
  that would make it look like the database survived.
* **Killing the agent process.** Watchdog pair restarts each other; a stop is
  logged. A determined admin can still stop both. On non-admin accounts,
  service permissions prevent it.
* **Uninstalling the app.** The correct answer, and always available. Not
  something we try to prevent.

## What the UI must not do

* Claim adult content is blocked when only the hosts backend is active and DoH
  is on in the browser.
* Claim a limit is enforced when the tracker backend has reported "unsupported"
  for the current session (e.g. Wayland).
* Silently swallow a `Backward`/`Forward` clock verdict — log and surface.
