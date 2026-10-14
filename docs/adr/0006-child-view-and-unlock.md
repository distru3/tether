# ADR 0006: Child view, one grown-up unlock, safeguards by profile

Date: 2026-10-15
Status: Accepted

## Context

- A child on their own account saw the same app a parent did: Limits and
  Settings were one click away, guarded only by PIN prompts on each change.
- A parent changing several things typed the PIN again for every change.
- Raising a limit waited 24 hours even when the parent made the change with
  the PIN. That wait exists to stop someone limiting *themselves* from giving
  in on impulse; for a parent the PIN is already the safeguard.

## Decision

**Safeguards follow the profile** (agent, `ipc_server/limits.rs`):

- **"Someone I look after"** with a PIN set: a raise needs the PIN, so it
  applies at once.
- **"Me"**, or a guardian who has not set a PIN yet: raises wait
  `limit_cooldown_hours` as before.

Lowering, adding, switching off and deleting were already instant.

**One grown-up unlock** (UI, `useLedgerActions`). The first time the PIN is
entered, the window stays unlocked for 10 minutes:

- While unlocked, actions that need the PIN reuse it without asking again.
- The window locks again on "Lock", after the 10 minutes, or as soon as it
  is hidden or closed.
- The PIN is held only in the main window's memory, never stored.
- The agent still checks it on every request; the unlock only saves typing.

**Child view.** The window is in child view when all of these hold:

- the profile is "Someone I look after";
- a PIN is set;
- the account on screen is watched (a child's account);
- the window is locked.

In child view:

- Today and Activity work as usual, without edit buttons or the budget
  suggestion.
- Limits and Settings show what they are for and an Unlock button.
- Viewing another account's screen time takes the unlock like any other PIN
  use.

## Consequences

- A parent on their own account sees no child view: their account is not
  watched, and "Me" mode never locks.
- An unlock on the child's account lasts 10 minutes at most and ends when
  the window closes, so it cannot be left open by accident for long.
- The "Wait before raising a limit" setting only matters in "Me" mode;
  Settings says so.
