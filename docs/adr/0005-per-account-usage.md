# ADR 0005: Usage, blocks and limits per Windows account

Date: 2026-10-14
Status: Accepted

## Context

Every Windows account on a PC fed the same counters. A parent who used the
PC for an hour ate the child's total budget, a block raised on the child's
account also covered the same app on the parent's, and Today showed the sum
of everyone. The session helper is already one process per signed-in user,
but `ReportUsage` carried no user, so the agent could not tell them apart.

## Decision

**Identity comes from Windows, not from the wire.** The agent resolves the
user of every pipe connection from the peer process's token (SYSTEM can open
any process): `GetNamedPipeClientProcessId` → `OpenProcessToken` →
`TokenUser` → the account's SID and name (`st_win32::process_user`). A
report cannot claim to be someone else, and nothing in the protocol changes
for the helper.

**Accounts table** (migration 0006): one row per SID the agent has met, with
its display name and whether it is **watched** (limits apply). An account is
added the first time any Tether process of that user connects.

**What is per account:** usage (`usage_daily`, `usage_intervals`), blocks
(`block_state`), extra time (`overrides`) and block-screen answers
(`block_reasons`), each with an `account_id`. **What is shared:** budgets,
schedules, the allowlist, websites, Family DNS and settings: one set of rules
for the household, counted separately for each watched account. Different
budgets per child are left for later.

**Watched by default.** A newly seen account is watched, so a new child
account is covered without anyone remembering to tick it. Unwatching an
account loosens enforcement, so it needs the PIN (a rule in `auth.rs`);
watching is free. Setup lists the accounts and lets the person setting up
untick their own; Settings has the same list.

**Unwatched accounts** are still tracked (Today and Activity work for them)
but are never blocked: the enforcer clears and skips their blocks.

**Existing data** belongs to no account after the migration (`account_id
0`). The first account the agent meets adopts it, which is right on the
common one-user PC and harmless elsewhere.

**Reading another account's data** (the dashboards take an optional
`account_id`) needs the PIN when one is set: a child should not browse the
parent's history, and the parent is the one who knows the PIN. Each
connection defaults to its own account.

**Enforcement** runs per account: the enforcer evaluates the freshest focus
report of every watched account, against that account's usage.

## Consequences

- A household with two children shares one set of budgets, each counted on
  its own; a per-child rule set is a later change to `limits` (an
  `account_id` there), not to this model.
- The dev fallback (`SCREENTIME_SELF_SAMPLE=1`) records under the agent's
  own account.
- The supervisor still watches "a helper reported recently", not each
  account's helper.
- The threat model's "usage is per PC" bypass is closed; an administrator
  can still do anything.
