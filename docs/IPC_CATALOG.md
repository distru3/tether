# IPC catalog

Every request on the local named pipe (`\\.\pipe\screentime`) and the Tauri commands in front of it. Current state only; the source of truth is `crates/ipc/src/lib.rs` (and the generated `ui/src/types/generated/*.ts`).

---

## 1. Protocol Architecture

- **Transport**: Local named pipe on Windows: `\\.\pipe\screentime`. (Unix domain socket reserved for Linux port).
- **Framing**: 4-byte big-endian length prefix followed by UTF-8 JSON payload. Maximum frame size: 8 MB (`MAX_FRAME_BYTES`).
- **Wire Format**: Serialized verbatim via `serde` with `snake_case` tag and field names.
- **Client Topology**:
  - `screentime-session`: One persistent connection looping `ReportUsage` (1 Hz), `Status`, and `BlockedApps`.
  - `screentime-ui` (Tauri): One-shot connections (connect, request, response, close) mapped to Tauri commands. Maximum worker threads on the agent: 32 (`MAX_WORKERS`).

- **Authorization**: The pipe ACL admits every authenticated local user, so it proves nothing about *who* is asking. Every request passes the agent's central gate (`crates/agent/src/ipc_server/auth.rs`) before dispatch. Requests that loosen enforcement need the PIN once one is configured (see the **PIN** column below). The UI first sends such requests without a PIN and opens its PIN prompt only when the agent answers `bad_pin`, so the agent alone decides what counts as loosening.
- **Peer identity**: per connection, the agent resolves the client's executable (`GetNamedPipeClientProcessId` → image path → `st_win32::peer_is_trusted`). `ReportUsage` and `RegisterDiscoveredApps` are refused (`bad_request`) from any program other than `screentime-session.exe` in the agent's own install. An uninspectable peer is allowed, so a lookup failure can never silently stop tracking. The overlay bridge pipe applies the same check in both directions. Both paths are expanded to their long form first (`st_win32::long_path`), because the service is registered under its 8.3 short path (`C:\PROGRA~1\Tether\bin`) while the helper runs from `C:\Program Files\Tether\bin`.
- **Peer account**: the agent also reads the Windows account of every connection from the peer's process token (`st_win32::account::process_user`: SID and display name) and records it (`accounts` table, docs/adr/0005-per-account-usage.md). Usage, blocks, extra time and block-screen answers belong to that account. `ReportUsage` from a peer whose account cannot be read is refused (`bad_request`).
- **Brute-force throttle**: After 5 consecutive wrong PINs or recovery codes, each further failure locks credential checks for 30 s, doubling to a 15 min ceiling (`st_core::pin::PinThrottle`). An empty PIN means "not supplied" and never counts. Rejected credentials are written to `audit_log` as `credential_rejected`.

---

## 2. Request & Response Specification

### Core Reporting & Queries

| Request Variant | Parameters | Response Variant | Description |
| :--- | :--- | :--- | :--- |
| `Ping` | None | `Pong` | Liveness probe. |
| `Status` | None | `Status(StatusDto)` | Agent version, backends, tracking availability, PIN configuration, strict mode, `profile` (`self`/`guardian`; `#[serde(default)]`, so an older agent reads as `self`), and `account` (the caller's `AccountDto`, `None` when it could not be read). |
| `DaySummary` | `day: DayKey`, `account_id?: i64`, `pin?: String` | `DaySummary(DaySummaryDto)` | Per-app and per-category usage rollups and chronological intervals for one day of one account (the caller's unless `account_id` names another; another account's needs the PIN). |
| `WeeklySummary`| `end_day: DayKey`, `account_id?`, `pin?` | `WeeklySummary(WeeklySummaryDto)` | 7-day usage array ending at `end_day`, plus previous week's total seconds, for one account (same rule as `DaySummary`). |
| `ListAccounts` | None | `Accounts(AccountsDto { accounts: [AccountDto { id, name, supervised, is_caller, last_seen_utc }] })` | Every Windows account Tether has met, by name. Free. |
| `Catalog` | None | `Catalog(CatalogDto)` | Full list of apps, categories, current limits, and pending cooldown limits. |
| `BlockedApps` | None | `BlockedApps(BlockedAppsDto)` | The caller's account's currently blocked applications, polled by the session helper for the block screen. |

### Mutations & Controls

| Request Variant | Parameters | Response Variant | Invariants & Cooldown |
| :--- | :--- | :--- | :--- |
| `SetLimit` | `target: LimitTargetDto`, `default_minutes: u32`, `weekday_minutes: [Option<u32>; 7]`, `enabled: bool`, `pin: String` | `Accepted { effective_utc, hud }` | Tightening applies immediately. Loosening queues into `pending_limits` (+24h). |
| `DeleteLimit` | `target: LimitTargetDto`, `pin: String` | `Accepted { effective_utc, hud }` | Deleting is instant by owner decision. |
| `CancelPendingLimit` | `target: LimitTargetDto`, `pin: String` | `Accepted { effective_utc, hud }` | Aborts a queued loosening change before it becomes active. |
| `GrantOverride` | `target: LimitTargetDto`, `seconds: i64`, `pin: String`, `account_id?: i64` | `Accepted { effective_utc, hud }` | Grants +15m override to an account (the caller's by default). Rejected outright in strict mode or with invalid PIN. |
| `SetAccountSupervised` | `account_id: i64`, `supervised: bool`, `pin?: String` | `Accepted` | Watch an account (free) or stop watching it (PIN). Stopping lifts the account's blocks at once. Audited as `account_supervision`. |
| `Categorize` | `app_id: i64`, `primary: Option<i64>`, `tags: Vec<i64>`, `pin?: String` | `Accepted { effective_utc, hud }` | Reclassifies app. Primary drives reporting; tags affect limit matching. **PIN always** (moving an app to an unlimited category is a bypass). |
| `CloseApps` | `app_id: i64`, `pin: String` | `Accepted { ... }` | User-initiated app termination from the block overlay. **No PIN when the app is currently blocked** (quitting only tightens); closing any other app needs the PIN. |
| `SetSetting` | `key: String`, `value: String`, `pin?: String` | `Accepted { ... }` | Only keys in `st_core::settings::SettingKey` are accepted (unknown keys, including `pin_hash`, are `bad_request`); values are range-checked and stored in canonical form, then applied to the live policy. **PIN when loosening**: `strict_mode`/`family_dns` → `false`, lowering `limit_cooldown_hours`, any change to `day_start_minutes`, `idle_threshold_secs` or `profile`. HUD, volume, hotkey and `capture_window_titles` never need it. Ranges: cooldown 0–168 h, day start 0–1439 min, idle 5–3600 s, volume 0–100, hotkey ≤ 32 chars, `profile` ∈ {`self`, `guardian`} (default `self`; who Tether is for, chosen in setup — it changes copy and whether the PIN step can be skipped, not enforcement). |

### PIN Vault

`VerifyPin { pin }` → `Accepted` or `bad_pin`. It changes nothing and is used for UI-only reveals (the hidden adult-domain list). It counts against the same brute-force throttle as every other credential check.

| Request Variant | Parameters | Response Variant | Invariants |
| :--- | :--- | :--- | :--- |
| `SetPin` | `new_pin: String`, `current_pin: Option<String>` | `PinVault { recovery_code }` | Plaintext recovery code is returned ONCE on creation/rotation. Stored only as Argon2 hash. |
| `RecoverPin` | `recovery_code: String`, `new_pin: String` | `PinVault { recovery_code }` | Consumes old recovery code and issues a fresh one. |
| `RemovePin` | `credential: String` | `Accepted { ... }` | Completely dismantles vault using current PIN or recovery code. |

### Web Filtering & Manual Blocks

| Request Variant | Parameters | Response Variant | Description |
| :--- | :--- | :--- | :--- |
| `ListManualBlocks` | None | `ManualBlocks { domains }` | Returns all manually blocked domains. |
| `AddManualBlock` | `domain: String` | `Accepted { ... }` | Adds a domain to the manual block list. The domain is normalised; adding an already-blocked domain is a no-op; invalid input is `bad_request`. Tightening, so no PIN. |
| `RemoveManualBlock`| `domain: String`, `pin: String` | `Accepted { ... }` | Removes every manual rule for the normalised domain; PIN-gated if a PIN is configured. `not_found` when the domain isn't blocked, `bad_request` when it isn't a valid domain. |

### Schedules & Allowlist

| Request Variant | Parameters | Response Variant | Description |
| :--- | :--- | :--- | :--- |
| `ListSchedules` | None | `Schedules(SchedulesDto)` | Lists all downtime schedules. |
| `CreateSchedule` | `name`, `weekday_mask`, `start_minute`, `end_minute` | `ScheduleCreated(ScheduleDto)` | Inserts new downtime window. |
| `UpdateSchedule` | `id`, `name`, `weekday_mask`, `start_minute`, `end_minute`, `pin?` | `Accepted { ... }` | Updates downtime window times/days. **PIN always.** |
| `SetScheduleEnabled` | `id`, `enabled`, `pin?` | `Accepted { ... }` | Toggles schedule active state. **PIN to disable**; enabling is free. |
| `DeleteSchedule` | `id`, `pin?` | `Accepted { ... }` | Deletes schedule. **PIN always.** |
| `ListAllowlist` | None | `Allowlist(AllowlistDto)` | Lists subjects exempt from downtime. |
| `SetAllowlist` | `subject_type`, `subject_id`, `allowed`, `pin?` | `Accepted { ... }` | Adds or removes subject from allowlist. **PIN to add**; removing is free. |

---

### Block-screen reasons

| Request | Arguments | Response | Notes |
|---|---|---|---|
| `RecordBlockReason` | `app_id: i64`, `reason: "finish" \| "bored" \| "habit"` | `Accepted { ... }` | The block screen's "What were you about to do?". No PIN (never changes enforcement). Accepted only while the app is blocked (`not_found` otherwise); unknown reasons are `bad_request`. Filed under the agent's own current day; one answer per app per day and account, a second answer replaces the first (table `block_reasons`, migrations 0005 and 0006). |
| `BlockReasons` | `from_day: DayKey`, `to_day: DayKey`, `account_id?`, `pin?` | `BlockReasons(BlockReasonsDto { counts: [{ reason, count }] })` | Most common first, for one account (same rule as `DaySummary`); range 1-366 days, oldest first, else `bad_request`. Used by Activity's "When time ran out". |

Tauri commands: `record_block_reason(app_id, reason)` and `block_reasons(from_day, to_day, account_id?, pin?)`. Accounts: `list_accounts()`, `set_account_supervised(account_id, supervised, pin?)`; `get_day_summary` and `get_weekly_summary` take `account_id?` and `pin?`, `grant_override` takes `account_id?`.

## 3. Error Codes (`ErrorCode`)

When a request fails, the agent responds with `Response::Error { code, message }`. Tauri normalizes this into `{ code: String, message: String }` for the UI:

- `bad_pin`: PIN verification failed.
- `cooldown_active`: Loosening change cannot take effect until anti-impulse cooldown expires.
- `strict_mode`: Requested operation (e.g. override) is forbidden while strict mode is active.
- `not_limitable`: Subject category is marked `never_block` (e.g. Developer tools, System utilities).
- `not_found`: Requested entity id does not exist.
- `bad_request`: Malformed payload, out-of-bounds minute/day, or invalid argument.
- `internal`: Storage failure or unhandled agent error.
- `rate_limited`: Too many wrong PINs or recovery codes in a row; the message says how many seconds remain.
- `unreachable` (Host-owned): Named pipe connection failed or agent service stopped.

---

## 4. Host-only Tauri commands

Commands in `ui/src-tauri` that do more than forward one `Request`.

| Command | Arguments | What it does |
|---|---|---|
| `run_tray_action` | `action: "reset_network" \| "stop_all"`, `pin?: String` | The tray menu's "Reset network settings" and "Stop Tether and its service". The menu does not run them: it shows the dashboard and emits `tray_action_requested`; the dashboard tries without a PIN, prompts on `bad_pin`, and calls this command, which sends `VerifyPin` to the agent first (allowed when no PIN is set, rate limited like every PIN check) and only then runs the action. Fails closed with `unreachable` when the agent is down; an administrator can still run `screentime-agent --reset-network`. |
| `open_dashboard` | none | "Open Tether" in the tray panel: hides the panel, shows and focuses the dashboard. |
