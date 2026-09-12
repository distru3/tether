# Screentime IPC & Command Catalog

This document is the exhaustive catalog of the local named pipe IPC interface (`\\.\pipe\screentime`) and its Tauri adapter layer. Verified against the codebase on **2026-09-12**.

---

## 1. Protocol Architecture

- **Transport**: Local named pipe on Windows: `\\.\pipe\screentime`. (Unix domain socket reserved for Linux port).
- **Framing**: 4-byte big-endian length prefix followed by UTF-8 JSON payload. Maximum frame size: 8 MB (`MAX_FRAME_BYTES`).
- **Wire Format**: Serialized verbatim via `serde` with `snake_case` tag and field names.
- **Client Topology**:
  - `screentime-session`: One persistent connection looping `ReportUsage` (1 Hz), `Status`, and `BlockedApps`.
  - `screentime-ui` (Tauri): One-shot connections (connect, request, response, close) mapped to Tauri commands. Maximum worker threads on the agent: 32 (`MAX_WORKERS`).

---

## 2. Request & Response Specification

### Core Reporting & Queries

| Request Variant | Parameters | Response Variant | Description |
| :--- | :--- | :--- | :--- |
| `Ping` | None | `Pong` | Liveness probe. |
| `Status` | None | `Status(StatusDto)` | Agent version, backends, tracking availability, PIN configuration, strict mode. |
| `DaySummary` | `day: DayKey` | `DaySummary(DaySummaryDto)` | Per-app and per-category usage rollups and chronological intervals for one day. |
| `WeeklySummary`| `end_day: DayKey` | `WeeklySummary(WeeklySummaryDto)` | 7-day usage array ending at `end_day`, plus previous week's total seconds. |
| `Catalog` | None | `Catalog(CatalogDto)` | Full list of apps, categories, current limits, and pending cooldown limits. |
| `BlockedApps` | None | `BlockedApps(BlockedAppsDto)` | List of currently blocked applications, polled by session helper for overlay trigger. |

### Mutations & Controls

| Request Variant | Parameters | Response Variant | Invariants & Cooldown |
| :--- | :--- | :--- | :--- |
| `SetLimit` | `target: LimitTargetDto`, `default_minutes: u32`, `weekday_minutes: [Option<u32>; 7]`, `enabled: bool`, `pin: String` | `Accepted { effective_utc, hud }` | Tightening applies immediately. Loosening queues into `pending_limits` (+24h). |
| `DeleteLimit` | `target: LimitTargetDto`, `pin: String` | `Accepted { effective_utc, hud }` | Deleting is instant by owner decision. |
| `CancelPendingLimit` | `target: LimitTargetDto`, `pin: String` | `Accepted { effective_utc, hud }` | Aborts a queued loosening change before it becomes active. |
| `GrantOverride` | `target: LimitTargetDto`, `seconds: i64`, `pin: String` | `Accepted { effective_utc, hud }` | Grants +15m override. Rejected outright in strict mode or with invalid PIN. |
| `Categorize` | `app_id: i64`, `primary: Option<i64>`, `tags: Vec<i64>` | `Accepted { effective_utc, hud }` | Reclassifies app. Primary drives reporting; tags affect limit matching. |
| `CloseApps` | `app_id: i64`, `pin: String` | `Accepted { ... }` | User-initiated app termination from the block overlay. |
| `SetSetting` | `key: String`, `value: String` | `Accepted { ... }` | Updates settings table and reloads runtime policy. |

### PIN Vault

| Request Variant | Parameters | Response Variant | Invariants |
| :--- | :--- | :--- | :--- |
| `SetPin` | `new_pin: String`, `current_pin: Option<String>` | `PinVault { recovery_code }` | Plaintext recovery code is returned ONCE on creation/rotation. Stored only as Argon2 hash. |
| `RecoverPin` | `recovery_code: String`, `new_pin: String` | `PinVault { recovery_code }` | Consumes old recovery code and issues a fresh one. |
| `RemovePin` | `credential: String` | `Accepted { ... }` | Completely dismantles vault using current PIN or recovery code. |

### Web Filtering & Manual Blocks

| Request Variant | Parameters | Response Variant | Description |
| :--- | :--- | :--- | :--- |
| `ListManualBlocks` | None | `ManualBlocks { domains }` | Returns all manually blocked domains. |
| `AddManualBlock` | `domain: String` | `Accepted { ... }` | Adds a domain to manual block list. |
| `RemoveManualBlock`| `domain: String`, `pin: String` | `Accepted { ... }` | Removes domain; PIN-gated if PIN is configured. |

### Schedules, Allowlist & Focus Sessions

| Request Variant | Parameters | Response Variant | Description |
| :--- | :--- | :--- | :--- |
| `ListSchedules` | None | `Schedules(SchedulesDto)` | Lists all downtime schedules. |
| `CreateSchedule` | `name`, `weekday_mask`, `start_minute`, `end_minute` | `ScheduleCreated(ScheduleDto)` | Inserts new downtime window. |
| `UpdateSchedule` | `id`, `name`, `weekday_mask`, `start_minute`, `end_minute` | `Accepted { ... }` | Updates downtime window times/days. |
| `SetScheduleEnabled` | `id`, `enabled` | `Accepted { ... }` | Toggles schedule active state. |
| `DeleteSchedule` | `id` | `Accepted { ... }` | Deletes schedule. |
| `ListAllowlist` | None | `Allowlist(AllowlistDto)` | Lists subjects exempt from downtime. |
| `SetAllowlist` | `subject_type`, `subject_id`, `allowed` | `Accepted { ... }` | Adds or removes subject from allowlist. |
| `GetFocusSession` | None | `FocusSession { session }` | Queries active focus session if any. |
| `StartFocusSession`| `duration_minutes: u32`, `name: Option<String>` | `Accepted { ... }` | Starts strict focus session. |
| `EndFocusSession` | `pin: Option<String>` | `Accepted { ... }` | Ends focus session early (PIN required if strict). |

---

## 3. Error Codes (`ErrorCode`)

When a request fails, the agent responds with `Response::Error { code, message }`. Tauri normalizes this into `{ code: String, message: String }` for the UI:

- `bad_pin`: PIN verification failed.
- `cooldown_active`: Loosening change cannot take effect until anti-impulse cooldown expires.
- `strict_mode`: Requested operation (e.g. override) is forbidden while strict mode is active.
- `not_limitable`: Subject category is marked `never_block` (e.g. Developer tools, System utilities).
- `not_found`: Requested entity id does not exist.
- `bad_request`: Malformed payload, out-of-bounds minute/day, or invalid argument.
- `internal`: Storage failure or unhandled agent error.
- `unreachable` (Host-owned): Named pipe connection failed or agent service stopped.
