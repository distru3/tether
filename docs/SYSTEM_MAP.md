# Screentime System Map & Architecture Reference

This document is the authoritative on-demand reference for AI agents and human engineers working on the Screentime codebase. Verified against the codebase on **2026-09-12**.

---

## 1. Process Model & Privilege Boundaries

The system is partitioned into **three separate processes** due to Windows operating-system privilege and desktop-isolation boundaries:

```
+-------------------------------------------------------------------------------+
|                               Windows OS                                      |
|                                                                               |
|  [Session 0 - High Privilege (SYSTEM / Elevated)]                             |
|  +-------------------------------------------------------------------------+  |
|  |  screentime-agent (crates/agent)                                        |  |
|  |  - SQLite Database (`st-storage`)                                       |  |
|  |  - Limits Engine & Day Rollover (`st-core`)                             |  |
|  |  - Named Pipe IPC Server: `\\.\pipe\screentime` (`st-ipc`)              |  |
|  |  - Local DNS Proxy (`st-dns`) on 127.0.0.1:53                           |  |
|  |  - Windows Hosts & Process Enforcer (`st-enforce-win`)                  |  |
|  +-------------------------------------------------------------------------+  |
|                                   ▲                                           |
|       Named Pipe IPC              │       Named Pipe IPC                      |
|       (Persistent 1 Hz)           │       (One-Shot Commands)                 |
|                                   │                                           |
|  [Interactive User Desktop Session (Medium Integrity)]                        |
|  +-----------------------------+  +----------------------------------------+  |
|  |  screentime-session         |  |  screentime-ui                         |  |
|  |  (crates/session)           |  |  (ui/ + ui/src-tauri)                  |  |
|  |  - 1 Hz Foreground Tracker  |  |  - Tauri 2 Desktop Shell               |  |
|  |  - Idle Detection           |  |  - React 18 / TypeScript Dashboard     |  |
|  |  - Win32 GDI Block Overlay  |  |  - Smoked-Glass Analytics UI           |  |
|  |  - Low-Level Keyboard Hook  |  |  - Settings, Limits & Filtering Admin  |  |
|  +-----------------------------+  +----------------------------------------+  |
+-------------------------------------------------------------------------------+
```

### Why This Split Exists
- **Session 0 Isolation**: On Windows, services running as SYSTEM cannot interact with the user desktop, cannot see the focused window, and cannot display UI. Thus, `screentime-agent` cannot sample the foreground app.
- **Session Helper (`screentime-session`)**: Runs inside the interactive desktop session. It samples `GetForegroundWindow()` and Win32 `GetLastInputInfo()` every second, packaging observations into `ReportUsage` requests sent to the agent over `\\.\pipe\screentime`. It also hosts the hand-painted GDI block overlay window because only a session process can draw topmost windows over games and fullscreen applications.
- **Tauri UI (`screentime-ui`)**: Runs the React frontend dashboard inside WebView2. It never connects to the database directly; it issues one-shot IPC requests to the agent via Tauri commands.

---

## 2. Workspace Crates & Layering Rules

```
crates/
├── core/             Pure domain logic. NO OS APIs. NO wall clock. NO SQL.
│                     Platform traits (`WindowTracker`, `ProcessController`, etc.),
│                     DayKey calculation, LimitEngine, Category, Schedules, FocusSession.
├── storage/          SQLite migrations (0001-0004), query modules, connection handling.
├── ipc/              Named pipe framing, wire serialization, Request/Response enums,
│                     ts-rs TypeScript binding generator.
├── st-win32/         Shared Win32 helpers (process handles, image paths, wide strings).
├── tracker-win/      Win32 active window and idle tracker (used by dev fallback).
├── tracker-linux/    Linux window tracking stub.
├── enforce-win/      Process freeze/terminate and hosts-file atomic writer.
├── enforce-linux/    Linux cgroup and hosts writer stub.
├── dnsproxy/         Local DNS server on 127.0.0.1:53 with ephemeral upstream client sockets.
├── agent/            Privileged daemon: IPC server, report ingestion, enforcement loop.
└── session/          Per-user sampling front and Win32 GDI block overlay.

ui/
├── src-tauri/        Tauri 2 backend: thin adapter translating Tauri invoke -> IPC named pipe.
└── src/              React 18 frontend: Smoked-glass analytics workspace.
```

### Invariant Rules
1. **`st-core` is pure domain logic**: No Windows/Linux headers, no `chrono::Utc::now()`, no SQLite. Time is always injected via `Clock` trait (`TestClock` in unit tests).
2. **Fail-closed enforcement**: Blocks persist end-of-local-day expiries (`DayKey::end_utc`). Restarting the agent never clears an active block. Blocks only lift when the day rolls over or via authorized PIN override.
3. **Session 1 Hz log quietness**: The session loop runs every second. Steady-state sampling and flushes must **never emit `INFO`-level logs**. `INFO` is reserved for process startup, connection status changes, and actual errors.

---

## 3. Database Schema & Persistence (`st-storage`)

SQLite database managed via append-only migrations tracked by `PRAGMA user_version`. **Never edit shipped migrations.**

### Migrations
- `0001_init.sql`: Base schema (`categories`, `apps`, `app_tags`, `sites`, `site_tags`, `usage_intervals`, `usage_daily`, `limits`, `schedules`, `allowlist`, `block_state`, `overrides`, `blocklists`, `block_rules`, `audit_log`, `settings`).
- `0002_pending_limits.sql`: Anti-impulse cooldown table (`pending_limits`) for queued loosening of limits (+24h).
- `0003_total_target_uniqueness.sql`: Expression index `ON limits (target_type, IFNULL(target_id, -1))` to enforce uniqueness for total-screen-time limits where `target_id IS NULL`.
- `0004_wall_clock_timers.sql`: Added `expires_utc` column to `overrides` for wall-clock expiry tracking.

### Core Tables & Invariants
- **`apps` & `categories`**: Every app has exactly one `primary_category_id` (ensuring reporting sums to 100%). Additional categories for limit matching are stored in `app_tags`.
- **`usage_intervals`**: Contiguous foreground spans. Polled at ~1 Hz by `session`, collapsed on identical consecutive samples by `agent`.
- **`usage_daily`**: Materialized daily rollup `(day_key, subject_type, subject_id, seconds)`. All dashboard queries read this directly.
- **`limits`**: Budget limits for `app`, `category`, or `total`. Contains `default_minutes` and a JSON array `weekday_minutes` (`[null, ...]`).
- **`pending_limits`**: Queued limit changes subject to the anti-impulse cooldown (+24 hours). Tightening applies immediately; loosening queues here.
- **`schedules` & `allowlist`**: Recurring downtime windows (e.g. Bedtime, School hours) with `weekday_mask` and `start_minute`/`end_minute`. Outside allowed times, non-allowlisted apps are blocked.

---

## 4. End-to-End Data Flows

### A. Usage Tracking Loop (1 Hz)
1. `screentime-session` calls `GetForegroundWindow()`, resolves `AppKey`, and measures idle time via `GetLastInputInfo()`.
2. Session sends `Request::ReportUsage { report }` over the persistent named pipe to `screentime-agent`.
3. Agent checks for clock skew, deduplicates recent observations, updates `usage_intervals`, and updates the `usage_daily` rollup table.
4. Agent's enforcement loop checks active limits against `usage_daily`.

### B. Limit Reached & Enforcement
1. If an app or category budget is exhausted, `st-core::LimitEngine` issues a `Decision::Block`.
2. Agent records the block in `block_state` with `expires_utc` set to the end of the local day.
3. On its 1 Hz tick, `screentime-session` queries `Request::BlockedApps`.
4. If the active foreground window matches a blocked app, `screentime-session` spawns the topmost, borderless Win32 GDI block overlay window (`WS_EX_TOPMOST | WS_EX_NOACTIVATE`) and installs a low-level keyboard hook (`WH_KEYBOARD_LL`) to swallow inputs.
5. User can click **Quit App** (sends `Request::CloseApps`) or enter their PIN on the click-pad for **+15 MIN EXTEND** (sends `Request::GrantOverride`).

### C. Web Filtering & DNS Proxy
1. `st-dns` runs a local DNS proxy server on `127.0.0.1:53`.
2. Incoming UDP queries are matched against `block_rules` and `sites`.
3. Blocked domains return `0.0.0.0`.
4. Non-blocked queries are forwarded to upstream resolvers via **ephemeral client sockets (`0.0.0.0:0`)**, NEVER reusing the `127.0.0.1:53` listener socket.

### D. Family DNS Protection & Automatic Original DNS Restoration
1. **Preservation on Enable**: When Family DNS is enabled (via UI Settings, Onboarding, or CLI `--enable-family-dns`), `st_dns::dns_config::capture()` reads all active network interfaces (`IfaceDns`), captures their exact DNS server IP lists (or DHCP state), and serializes them to both SQLite `settings` (`original_dns_config`) and `{data_dir}/original_dns_backup.json`.
2. **Registry Tracking**: `HKLM\Software\Screentime\FamilyDnsApplied` DWORD is set to `1`. Active interfaces are configured to use Cloudflare Family DNS (`1.1.1.3`, `1.0.0.3`, `2606:4700:4700::1113`, `2606:4700:4700::1003`).
3. **Restoration on Disable**: When Family DNS is disabled (via UI Settings or CLI `--disable-family-dns`), the agent reads the serialized backup from SQLite or the JSON backup file, restores each interface back to its exact prior configuration (static IPs or DHCP), flushes the Windows DNS resolver cache (`ipconfig /flushdns`), removes the backup records, and clears the registry flag.
4. **Silent Uninstaller Restoration**: The NSIS uninstaller (`installer_hooks.nsh`) inspects `FamilyDnsApplied`. If set to `1`, it silently executes `screentime-agent.exe --disable-family-dns` without interactive popup prompts, ensuring the user's internet is cleanly restored before service removal.

### E. Application Discovery & Auto-Classification
1. **Multi-Source Discovery (`st-tracker-win::discovery`)**:
   - **Start Menu**: Recursively inspects `.lnk` shortcuts in All Users and Current User directories, extracting target executables and localized shortcut display names.
   - **Uninstall Registry**: Scans 64-bit and 32-bit Windows uninstall registries (`HKLM` & `HKCU`) to extract application names, installation directories, display icons, and publishers.
   - **GameConfigStore Registry**: Scans `HKCU\System\GameConfigStore\Children` where Windows DirectX and Game Bar register games (`Type: 1`), automatically detecting standalone, packaged, and non-Steam games.
2. **Multi-Signal Classification (`st-agent::classify`)**:
   - **Layer 1 (Exact Signatures)**: High-confidence overrides for known standard binaries.
   - **Layer 2 (Expanded Paths)**: Matches paths against game launchers, `\Games\`, `\Game\`, `\SteamLibrary\`, `\Riot Games\`, `\Ubisoft\`, and `\Electronic Arts\`.
   - **Layer 3 (Publisher Heuristics)**: Matches developer and publisher strings (e.g., *Studios*, *Entertainment*, *Ubisoft*, *Bethesda*, *Adobe*, *JetBrains*) to corresponding categories.
   - **Layer 4 (Display Name Keywords)**: Evaluates application display names against targeted domain keywords (development, creativity, communication, music).
   - **Layer 5 (Name Patterns)**: Evaluates engine and shipping suffixes (`_win64-shipping.exe`, etc.).
   - **Human Decision Immutability**: Auto-classification runs strictly when `!user_classified && primary == default_category`. User manual categorizations are never overwritten.
### F. Game & Fullscreen Overlay Suppression (FPS & DWM Flip Preservation)
1. **The DWM Flip Degradation Problem**:
   - DirectX 11/12 and Vulkan 3D swapchains rely on **Hardware Independent Flip (iFlip)** for direct display scanout, enabling low-latency frame pacing, Variable Refresh Rate (FreeSync/G-Sync), and frame rate capping.
   - When an external Win32 layered topmost window (`WS_EX_LAYERED | WS_EX_TOPMOST`) intersects any pixel of a 3D swapchain, Windows DWM degrades presentation from Independent Flip to **Composed Flip**.
   - Under Composed Flip, DWM decouples `IDXGISwapChain::Present` from physical VBlank into a multi-buffered queue. As a result, `Present(1, 0)` does not block, in-game frame rate caps and DXGI waitable latency objects are bypassed, and GPU driver limiters (e.g. AMD Radeon Chill / FRTC, Nvidia Max Frame Rate) disengage, causing the game to render at uncapped FPS.
2. **Two-Tier Fail-Safe Suppression**:
   - **Tier 1 (Agent Policy Filter - `st-agent::ipc_server`)**: In `report_usage`, when evaluating whether to return `HudStateDto`, the agent checks the database category of the focused app. If categorized under `"games"` and `!policy.show_hud_in_fullscreen`, the HUD payload is suppressed (`hud = None`).
   - **Tier 2 (Session Front Guard - `st-session::main::is_game_or_fullscreen`)**: Even before a newly discovered app is classified by the agent, the session front inspects the focused window snapshot (`FocusedSnapshot`) every 1 Hz tick:
     - **Launcher & Library Paths**: Checks for path signatures like `\steamapps\common\`, `\epic games\`, `\riot games\`, `\xboxgames\`, `\battle.net\`, `\gog galaxy\games\`, `\games\`, etc.
     - **Executable Signatures**: Checks known game binaries (`cs2.exe`, `valorant.exe`, `gta5.exe`, etc.) and engine shipping suffixes (`-shipping.exe`, `*game*.exe`).
     - **Window Class Names**: Queries `GetClassNameW` for engine window classes (`UnrealWindow`, `UnityWndClass`, `Valve001`, `GLFW30`, `SDL_app`, `Godot_Engine`, `Direct3D`, `RenderWindow`).
     - **Shell Notification State**: Queries `SHQueryUserNotificationState` for `QUNS_RUNNING_D3D_FULL_SCREEN` and `QUNS_PRESENTATION_MODE`.
     - **Tolerant Geometry Coverage**: Compares window `RECT` against `MONITORINFO` with a 48px tolerance margin to reliably detect borderless windowed mode across multi-monitor and DPI-scaled configurations.
   - **User Toggle**: Configured under Settings -> Timer HUD & Overlay (`show_hud_in_fullscreen`, defaults to `false`), with localized warnings in English and Arabic detailing the DWM composition impact.
3. **Transient Peek Mode & Remapable Hotkey**:
   - **Universal On-Demand Peek**: Even when continuous HUD overlay is toggled off (`!show_hud_overlay`) or when in full-screen 3D games (`!show_hud_in_fullscreen`), pressing the peek shortcut (`Ctrl+Alt+T`) or crossing a milestone alert immediately reveals the HUD for 4 seconds over the active application or game before automatically dismissing.
   - **Remapable Hotkey Peek (`Ctrl + Alt + T` default)**:
     - Configured interactively in **Settings → Timer HUD & Overlay** with an interactive keyboard recorder.
     - Persisted in SQLite `settings` (`hud_peek_hotkey`) and transmitted over `StatusDto`.
     - `st-session` runs a dedicated background hotkey thread that dynamically registers/unregisters hotkeys via `RegisterHotKey` / `UnregisterHotKey`.
     - When pressed, the HUD surfaces for 4 seconds over the active app/game and automatically vanishes.
   - **Milestone Alert System (15m, 10m, 5m, 1m)**:
     - `st-session` monitors budget transitions across 900s, 600s, 300s, and 60s.
     - On crossing or when the block overlay spawns, plays a custom high-fidelity, non-intrusive harmonic chime (`PlaySoundW` with `SND_MEMORY | SND_ASYNC`, embedded in `st-session` binary) and surfaces the HUD for 4 seconds before auto-dismissing.
4. **Hardware Multiplane Overlay (MPO) DirectComposition Architecture (`crates/session/src/mpo.rs` & `hud.rs`)**:
   - **DirectComposition & Hardware Overlay Pipeline**:
     - The HUD window is created with `WS_EX_TOPMOST | WS_EX_TRANSPARENT | WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW` and `WS_EX_NOREDIRECTIONBITMAP` (`0x00200000`), ensuring it floats topmost over applications without allocating a GDI CPU redirection buffer.
     - A DirectComposition visual target (`CreateTargetForHwnd`) hosts an independent DirectX 11 DXGI flip-discard swapchain (`DXGI_SWAP_EFFECT_FLIP_DISCARD`) with `DXGI_SCALING_NONE` (1:1 pixel mapping) and `DXGI_ALPHA_MODE_PREMULTIPLIED`, rendered with Direct2D/DirectWrite.
     - Position tracking operates at 10 Hz (100ms), invoking `SetWindowPos(HWND_TOPMOST)` strictly when window bounds actually change to eliminate unnecessary DWM compositor wakeups during static gameplay.
     - Note on Fullscreen 3D Games: Any external topmost window presented over a fullscreen 3D application requires DWM to composite the topmost layer over the game. For users requiring absolute zero DWM composition overhead (pure Hardware Independent Flip), the continuous overlay in games can be kept disabled via the `show_hud_in_fullscreen` setting while retaining on-demand 4-second peek via hotkey.
   - **Graceful GDI Layered Fallback**:
     - If DirectX 11, DirectComposition, or DXGI swapchain initialization fails (e.g. basic display adapter, remote desktop session, or legacy VM), Tether automatically demotes the HWND to a layered window (`WS_EX_LAYERED`) and routes drawing through GDI ClearType rendering, guaranteeing zero crashes.
