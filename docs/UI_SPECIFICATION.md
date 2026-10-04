# Tether UI Architecture & Redesign Specification

This document defines the frontend layout, design system tokens, and component architecture for the **Tether** dashboard. Verified against the codebase on **2026-10-03**.

---

## 1. Design System & Brand Identity

The visual and interaction contract is **[`docs/DESIGN_SYSTEM.md`](DESIGN_SYSTEM.md)** (v2, approved 2026-10-03): who the app serves (`profile`: self or guardian), principles, the plum-and-orange palette taken from the app icon, Rubik + Unbounded type, shape, components and voice. This file describes how the code implements it.

- **Themes**: `light`, `dark`, `system` ("Like Windows"). Palettes are defined once in `ui/src/styles/redesign.css` (`:root, [data-theme="dark"]` and `[data-theme="light"]`); `tokens.css` holds the shared scales (fonts, spacing, radii, shadows, motion).
- **Budget hues**: `--budget-{games,social,video}` with `-tile` / `-fill` variants and `--budget-other`. Components pick one with a `tt-hue-{games,social,video,total,other}` class from `ui/src/budgetHue.ts` (`hueFor(target, catalog)`: Games → games; Social Media and Short-Form Video → social; Video & Streaming → video; the total limit → total; everything else → other).
- **Components for the v2 screens** live in `ui/src/styles/tether.css` (prefix `tt-`, loaded last): page head (`tt-page`, `tt-head`, `tt-title`, `tt-sub`), buttons (`tt-btn` + `--primary/--accent/--outline/--ghost/--danger/--sm`, `tt-link`), cards and rows (`tt-card`, `tt-row*`, `tt-columns`), budget rows (`tt-budget`, `tt-swatch`, `tt-meter`), notes (`tt-note`, `tt-note--calm`, `tt-banner`), inputs (`tt-input`, `tt-segmented` with `aria-pressed`, `tt-chips`), setup (`tt-setup*`, `tt-steps`, `tt-choice`, `tt-template`, `tt-code`) and settings (`tt-settings-grid`, `tt-range`). Older screens (Today, Downtime, Websites, dialogs) keep their classes in `app.css` / `redesign.css` and are reskinned through the tokens.

---

## 2. Layout Structure & Top Navigation

```
+-----------------------------------------------------------------------------------------+
| [TitleBar] Tether (34px, custom drag region, minimize/maximize/close)                   |
+-----------------------------------------------------------------------------------------+
| [Top bar: .app-sidebar]                                                                 |
|  [squircle mark] Tether [Live] |  ( Today ) ( Limits ) ( Settings )  |  (< Today >)      |
+-----------------------------------------------------------------------------------------+
| [Main content: .app-main-content]   overview | limits | settings                        |
+-----------------------------------------------------------------------------------------+
```

### App Bar (`TitleBar.tsx` & `TitleBar.css`)
- 34px, themed through `--bg-app` / `--border-subtle`. Window title "Tether"; minimize, maximize/restore and close (46px hit width). The close button says what it does ("Close window (Tether keeps running)").

### Navigation Bar (`Sidebar.tsx` & `redesign.css`)
- **Brand**: `.brand-mark` squircle plus the "Tether" wordmark (`.logo-title`, display font) and the status pill (`status-pill--live` / `--offline`).
- **Pill nav** (`.sidebar-nav`, radius 99px on `--bg-card`): the active item (`.nav-item--active`, `aria-current="page"`) is a filled `--color-primary` pill with `--color-on-primary` text. At ≤980px labels hide (each item keeps its `aria-label`); at ≤700px the bar wraps.
- **Day stepper**: `<` / `>` and a "Today"/date label; only shown on Today (`visibility: hidden` elsewhere so the bar keeps its balance).

### Nav Items (`TabKey = "overview" | "limits" | "settings"`)
1. **Today** (`overview`, `nav.overview`): time left, the day strip and budget tiles (Section 3).
2. **Limits** (`limits`, `nav.limits`): every rule in one place: budgets, schedules, websites (Section 4).
3. **Settings** (`settings`): six short groups (Section 4).

There is no separate Web Filtering tab any more; websites live on Limits.

---

## 3. Today (`overview`, `ui/src/pages/TodayPage.tsx`)

Phase 2 of `docs/DESIGN_SYSTEM.md`: time **left** first. The day picker in the top bar is the only date control; past days use the same screen.

```
+-----------------------------------------------------------------------------------------+
| .service-alert (only while the agent is unreachable; role="alert")                      |
+-----------------------------------------------------------------------------------------+
| Hero card                                                                               |
|  ( ring: time left of the total budget )  Headline ("You're on track today")            |
|                                            "2h 55m used so far. Bedtime starts at 10 pm, |
|                                             in 3h 18m."                                 |
|                                            [day strip: day start -> +24 h, now line]    |
|                                            legend: budgets · Everything else · Bedtime  |
+-----------------------------------------------------------------------------------------+
| BlockedBanner (only while something is blocked): name + "Allow 15 more minutes"         |
+-----------------------------------------------------------------------------------------+
| Budget tiles (one per enabled app/category limit): tint fills to the share left,        |
| status pill, "20m left" / "Back at 4 AM", apps that counted, Edit                       |
+-----------------------------------------------------------------------------------------+
| Schedule card (active or next schedule)   |  Websites card (sites blocked, Family DNS)  |
+-----------------------------------------------------------------------------------------+
| Most used (UsageAside)                    |  This week (WeeklyChart)                    |
+-----------------------------------------------------------------------------------------+
```

All numbers come from `ui/src/todayModel.ts` (pure functions, `now` passed in):
- **`budgetStates`**: one entry per enabled app or category limit. Time left = the usage row's `limit_seconds - seconds` (the agent's own arithmetic, weekday overrides and tagged apps included); no row means nothing used. Status: `done` (nothing left), `low` (≤ 15 min or ≤ 20 % left), `extra` (a "+15 min" extension is running, until `timer_expires_utc`), else `plenty`. The apps listed are the day's apps that count toward it.
- **`totalState`**: the total-screen-time limit for the viewed weekday; drives the ring. Without one the ring shows time used, with no arc.
- **Headline** priority: total used up, a budget done, total low, a budget low, else "You're on track today" ("Here's your day so far" with no budgets). Past days show the date.
- **`stripSegments`**: usage intervals placed on the agent's day (`dayWindowStart` = midnight + `day_start_minutes`), coloured by the budget the app counts toward (`hueForApp`: its own app limit, else a category limit on its primary category or a tag, else "Everything else"), merged when the same colour is less than 90 s apart.
- **`scheduleBands` / `scheduleOutlook`**: enabled downtime schedules that overlap the day, hatched on the strip. An occurrence belongs to the weekday it starts on, as in `st_core::schedules` (Friday's 22:00-07:00 runs into Saturday morning). The subtitle names the schedule in force or the next one to start.
- Strip labels sit at their real positions (start, +6 h, +12 h, +18 h, end, "now"); labels near "now" are hidden. In Arabic the strip runs right to left with the page.

Tiles use the budget hue tokens (`tt-hue-*`: `--hue`, `--hue-tile`, `--hue-fill`, `--hue-ink`). "Most used" bars use the same hue as the strip; the category is a neutral button that opens the categorize dialog (the old palette-coloured pills, an accessibility gap, are gone). "This week" bars are buttons that open that day; the viewed day is orange.

Removed with this change: `ExecutiveHeader` (metric cards), `CategoryMix` (donut), `LedgerRule` (old 00-24 h timeline that ignored the day start), `dashboardMetrics.ts`, and ~180 translation keys and ~680 CSS lines they used.

---

## 4. Other Panels

### Limits (`ui/src/pages/LimitsPage.tsx`)
One page for every rule. `View = "main" | "schedules" | "websites"`; the two subviews show a back button and their own level-1 heading.

- **Header**: "Limits" plus a subtitle naming the cooldown ("Lowering a limit works right away. Raising one waits {{count}} hours…", plural keys; `subtitleNoWait` when the cooldown is 0), and "+ New budget" (`actions.startNewOrder`, opens `LimitEditorDialog`).
- **Waiting banner**: one `tt-banner` row per `catalog.pending_limits` entry ("{{name}} changes to {{amount}} a day, {{when}}." / "{{name}} is removed {{when}}.", `formatWhen`), each with **Cancel** (`actions.cancelPendingLimit`).
- **Budgets card**: one `tt-budget` row per limit: hue swatch, name, the rule (`limitRule`: same every day / weekday overrides), today's use against the budget (`role="progressbar"` meter, over-budget text), an on/off switch (`actions.toggleLimit`) and **Edit** (`actions.openEditor`). Paused rows dim only the swatch and meter so text keeps its contrast. Rows stack under 760px.
- **Schedules card** (side column): each downtime schedule with its clock range (`Intl` time format), days ("Every day" / "Weekdays" / "Weekends" / list from the bitmask), and a switch; **Manage** opens the Schedules subview (`DowntimeSection`: editor, delete, always-allowed apps).
- **Websites card** (side column): add a site inline (`addManualBlock`), up to three blocked sites as chips (hidden adult-list domains excluded via `domains.ts` `isHiddenDomain`), "+N more", and the **Family DNS** switch (`family_dns` setting). **Manage** opens the Websites subview (`WebFilteringPanel`: bulk import, search, paging, hidden-domain reveal).

### Budget editor (`LimitEditorDialog.tsx`)
Target picker (app / category / total), duration slider + h/m inputs + presets, weekday overrides. **Before the save button it says when the change applies**:
- `limitRules.ts` `waitsForCooldown(existing, minutes, weekdayMinutes, enabled)` mirrors the agent's `is_loosening` (`crates/agent/src/ipc_server/limits.rs`): raising the daily limit or any day's effective limit waits; lowering, a new limit and switching a limit off apply now (removing or disabling applies instantly by owner decision).
- If it waits (and `cooldownHours > 0`): an orange `tt-note` "Applies {{when}}" with the reason, and the button reads "Save, applies later" (`limitEditor.saveLater`). Otherwise a calm note "Applies right away."
- `cooldownHours` comes from `statusInfo.limit_cooldown_hours` (24 by default). The agent stays the authority; the note is a preview.

### Settings (`ui/src/pages/SettingsPage.tsx`)
Six groups in a `tt-settings-grid` (two columns when there is room):
1. **Protection**: who it's for (segmented Me / Someone I look after → `profile`), PIN (set or change, `actions.openPinSetup`), strict mode, cooldown hours.
2. **Timer on screen**: show timer (`show_hud_overlay`), in full-screen games (`show_hud_in_fullscreen`), peek shortcut recorder + reset (`hud_peek_hotkey`), chime volume (`alert_volume`) with Play (`previewAlertSound`).
3. **Day and tracking**: day starts at (`<input type="time">` → `day_start_minutes`), idle after (`idle_threshold_secs`).
4. **Language and look**: language segmented control, theme Light / Dark / Like Windows.
5. **Apps**: opens the app directory.
6. **Help**: collapsible tutorial and the version.

Every switch and input shows a spinner while saving (`settingPending`). Changes go through `actions.setSetting`; the agent's authorization gate decides which need the PIN, and the UI prompts for it when asked.

### Application Directory & Categorization
- **`AppDirectoryDialog.tsx`**: search, All / Categorized / Uncategorized chips, rows with category swatch, name, path, `[Custom]` badge and **Change**. Opened from Settings → Apps.
- **`CategorizeDialog.tsx`**: multi-tag category checkboxes and "Auto-detect" reset. Also opened from the Today "Most used" category pills and from the budget editor's category preview.

### Setup (`ui/src/components/setup/SetupFlow.tsx`)
Shown on first run (`localStorage` `screentime_first_run_completed`). Four steps with a step list (`aria-current="step"`) and a language switch in the top `<header>`:
1. **Who is this for?** Me / Someone I look after → saves `profile`.
2. **Starting budgets**: templates (there is no usage history yet): Games 1h (2h on weekends), Social 30m, Video 1.5h, Bedtime 10 pm–7 am (a schedule), Total 4h (off by default). Each picked template is created with `set_limit` / `create_schedule`; failures are counted and the user can continue.
3. **PIN**: at least 4 digits plus confirmation → `set_pin`, then the recovery code (`tt-code`) and "I wrote it down". Skipping is offered only for "Me". If a PIN already exists the step says so.
4. **Websites**: Family DNS switch; **Finish** marks setup done.

### App-Wide Loading Animations & Shimmer Skeletons
- **`LoadingSpinner.tsx`**: SVG spinner (`xs`, `sm`, `md`, `lg`) used in form submissions, PIN buttons (`PinGate`, `PinSetupDialog`), the budget editor and settings controls.
- **Websites subview**: skeleton rows (`.skeleton-shimmer`) while domain rules load; bulk import shows a spinner while it parses.

---

## 5. Overlays: Win32 Draggable HUD & Hardware-Accelerated Block Overlay

### A. Timer HUD Overlay (`crates/session/src/hud.rs` & `crates/session/src/mpo.rs`)
- **Visual Design** (Phase 2): 92x28 pill, colours from one pure module, `crates/session/src/hud_palette.rs` (`hud_colors`, unit-tested), shared by the Direct2D (`mpo.rs`) and GDI (`hud.rs`) renderers so they cannot drift.
  - Dark: plum `#1C1229`, border `#3A2752`, digits `#F4EEFB`, lilac dot `#C9B6F2`.
  - Light: white, border `#E7E0F0`, digits `#23163A`, violet dot `#8F6CE6`.
  - A running "+15 min" extension shows an orange dot `#F08A3C`.
  - Last minute (≤ 60 s): the whole pill turns orange with plum digits.
  - Digits: Segoe UI bold (tabular figures; Consolas before).
- **Direct Dragging & Multi-Monitor Window Clamping**:
  - Registered with `WS_EX_NOACTIVATE` so mouse dragging never steals keyboard focus or activates the overlay over fullscreen games or typing apps.
  - Dragging uses Win32 `SetCapture` / `ReleaseCapture` upon `WM_LBUTTONDOWN`, `WM_MOUSEMOVE`, and `WM_LBUTTONUP`.
  - Moving cursor shows `IDC_SIZEALL` 4-way move cursor.
  - Position is clamped strictly inside target application window bounds (`wr.left + 4` to `wr.right - HUD_W - 4`).
  - Position persistence: Saved to `%LOCALAPPDATA%\screentime\hud_pos.json` (relative offset and anchor corner: `from_right` / `from_bottom`). Survives window moves, resizing, and app restarts.
- **Windows Volume Flyout-Style Entrance Animation**:
  - Smooth 240 ms entrance animation powered by a 16 ms high-precision timer (`ANIM_TIMER_ID = 2`).
  - Cubic deceleration easing: `ease = 1.0 - (1.0 - t)^3`.
  - Adaptive trajectory: If positioned in the top half of the window, gracefully slides **down** from `target_y - 24` to `target_y`. If positioned in the bottom half, gracefully slides **up** from `target_y + 24` to `target_y`.
  - Drag interruption: If the user begins dragging while the animation is playing, the animation cancels cleanly and mouse drag takes immediate precedence.
- **Theme Synchronization**:
  - Overlays continuously adapt to the user's active theme by synchronizing with `%LOCALAPPDATA%\screentime\theme.txt` (`light`, `dark` or `system`; `theme_is_light`). `system` follows Windows' app mode (`HKCU\...\Themes\Personalize\AppsUseLightTheme`); it used to be read as dark.

### B. Hardware-Accelerated Tauri 2 React Block Overlay (`BlockOverlay.tsx` & `overlay_bridge.rs`)
- **Architecture**: Hardware-accelerated transparent secondary webview window in Tauri 2 commanded over named pipe `\\.\pipe\screentime_overlay_bridge`.
- **Two-Tier Sampling & Reconciliation Cadence**:
  - **100 ms Overlay & Focus Loop**: The session helper polls `take_sample()` at 10 Hz (100 ms interval). Focus transitions to and away from blocked apps react within ≤100 ms, making the block screen snap into place and dismiss with near-instant responsiveness.
  - **1 Hz Ingest Grid Preservation**: Normal usage accumulation (`ObsAccumulator::offer`) and persistent pipe communication (`run_frames`) are bounded to ~1 Hz (plus immediate focus boundary flushes), preserving the agent's database write contract and zero-noise logging policy.
- **Window Title Discrimination**:
  - `is_overlay_window_focused()` inspects the Win32 foreground window via `GetWindowTextW`.
  - When the user focuses the secondary block screen (`"Tether Overlay"`), input is preserved for PIN entry and extension.
  - When the user clicks the primary Tether Dashboard (`"Tether"`), the helper correctly treats this as moving focus away from the blocked app, instantly dismissing the block card and preventing the overlay from locking out the main application.
- **Immediate Dismissal & Ghost Click Elimination**:
  - On `OverlayBridgeRequest::Hide`, Tauri immediately calls `window.hide()` in 0 ms.
  - Eliminates ghost click stealing and prevents focus oscillation loops where mouse clicks near the overlay area would reactivate `screentime-ui.exe`.
- **No input hooks**: the block screen is an always-on-top window covering the app; there is no keyboard hook and the blocked window is not disabled (`EnableWindow` was documented here but never implemented). What keeps the block in place is the session re-asserting it:
- **Self-healing delivery (2026-10)**: while the block holds and the app is focused, the session re-sends `Show` every 2 s (backoff 1 s → 10 s while unacknowledged) and relaunches the UI (`screentime-ui.exe` / `Tether.exe`) at most every 15 s if the bridge pipe is missing. `Show` is idempotent. `overlay_update` is re-emitted only when the target changes or the window was hidden, so a PIN being typed survives.
- **Sizing**: at least 520×820 logical pixels with a PIN (560 tall without), scaled by the monitor's scale factor (the floors used to be physical pixels, so at 150 % the window was two-thirds of the intended size), centered on the app and clamped to its monitor. The page scrolls instead of clipping and drops its badge below 640px, so "Close" is always reachable.
- **Quit is honest**: the overlay hides only after the app actually closed (local close, or the agent's `CloseApps`, which needs no PIN for a *blocked* app). Otherwise it shows "Couldn’t close the app: …" and stays up.
- **Peer verification**: both bridge ends check the other process is a genuine Tether binary from the same install (`st_win32::peer_is_trusted`).
- **"Pause and choose" design (Phase 2, `docs/DESIGN_SYSTEM.md`)**: always the dark palette (the root sets `data-theme="dark"`), an opaque plum backdrop, no card:
  - **Why it is blocked**, re-derived by `ui/src/blockModel.ts` in the agent's own order (`enforcer.rs`): a downtime schedule in force (unless the app is always allowed), then the app's budget, then total screen time. The overlay loads status, catalog, today's summary, schedules and the allowlist itself; the bridge DTO only names the app.
  - Badge (empty budget ring in the budget's hue, or a moon for downtime), eyebrow ("TikTok · 20m every day, all used" / "Downtime · 6:50 PM – 8:50 PM"), title ("Time's up for TikTok today" / "Homework time is on"), and "TikTok is waiting behind this screen, so nothing is lost."
  - Strip of the agent's day, hatched orange from now until the app comes back ("back at 12 AM · in 4h 39m"): the day reset for budgets, the schedule's end for downtime.
  - **Close {app}** (orange, primary). Extra time is offered only when it can work: never during downtime (the agent re-blocks every tick, an override does not lift a schedule) and not in strict mode. With a PIN it is a link ("A grown-up can add 15 minutes with the PIN" for the guardian profile, "Add 15 minutes with the PIN" for self) that opens the PIN pad; without one, a secondary "Allow 15 more minutes" button. During downtime a note says the schedule is changed on Limits.
  - Keyboard: digits, Backspace and Enter drive the PIN pad once it is open; **Escape only closes the pad**. (It used to quit the blocked app, which a reflex press in a game would do by accident.)
  - Not yet (needs agent support, Phase 4): asking why ("What were you about to do?") and borrowing from tomorrow.

---

## 6. Theme System & Component Standardization

### A. Light, Dark and Like Windows
- **Hook (`useTheme.ts`)**: `ThemePreference = "light" | "dark" | "system"`, `EffectiveTheme = "light" | "dark"`. The choice is persisted in `localStorage` (`tether_theme`) and through the Tauri `set_theme` command; `applyTheme` sets both `data-theme` and `data-theme-mode` on `<html>` to the effective theme. `system` follows `prefers-color-scheme` live.
- **Migration**: retired names map to the new ones in three places that must agree: `useTheme.ts` `normalizePref`, the `index.html` bootstrap script, and `ui/src-tauri/src/lib.rs` `normalize_theme` (`THEMES = ["light", "dark", "system"]`). Dark: `midnight-cobalt`, `slate-charcoal`, `cyber-emerald`, `horizon-dark`, `classic-dark`. Light: `clean-titanium`, `nordic-frost`, `horizon-light`, `classic-light`.
- **Zero FOUC**: the `index.html` script stamps the theme before CSS or React load.
- **Native HUD**: `crates/session/src/hud.rs` reads `theme.txt` and treats any value containing "light" as light, so it needs no change for the new names.

### B. Switches
- One definition (`.toggle-switch` in `redesign.css`): 42×24, off track `--border-strong`, on track `--color-primary` with a `--color-accent` knob; the knob moves toward the inline end (RTL aware); `:focus-visible` ring. Always rendered through `ToggleSwitch` with an accessible `label`.

### C. Top Date Stepper
- Fixed width so the nav never shifts between "Today" and a past date. Shown only on Today; elsewhere `visibility: hidden` and out of the tab order.

---

## 7. Scheduled Downtime & Bedtime Mode (Phase A)

### A. Schedules on the Limits page
- **Entry point**: the Schedules card on Limits lists each schedule with a switch; its **Manage** link opens the Schedules subview (back button + level-1 "Schedules" heading) that hosts `DowntimeSection`.
- **Dedicated Viewport (`DowntimeSection.tsx`)**:
  - **Metric Cluster**: Surfaces 3 live status cards:
    - *Active Schedules*: Ratio of enabled schedules (e.g. `1 / 2`).
    - *Downtime Status*: Real-time enforcement indicator (`Active now` with amber badge or `Inactive`).
    - *Always Allowed*: Counter of exempt applications.
  - **Recurring Schedule Rows**:
    - Displays schedule name (e.g. "Bedtime", "Deep Work"), time interval badge (`10:00 PM – 7:00 AM`), active weekday pills (`M T W T F S S`), overnight indicator badge, quick toggle switch, edit button, and delete action.
  - **Schedule Editor Modal (`ScheduleEditorModal`)**:
    - Backed by accessible `Dialog` modal.
    - Configures schedule name, HTML5 start and end time pickers (converted to 0–1439 minute integers), overnight calculation badge, and 7 interactive day bitmask pills with quick presets ("Every day", "Weekdays", "Weekends").
  - **Always-Allowed Apps (Downtime Allowlist)**:
    - Searchable application dropdown powered by detected apps from `catalog.apps`.
    - Allows user to exempt critical tools (e.g. Calculator, Phone, Notes) that remain fully accessible during scheduled downtime.
    - Visual tag list with remove buttons for quick exemption management.

### B. End-to-End Pipeline
- **Backend IPC**: Full round-trip support in `st-agent::ipc_server` for `ListSchedules`, `CreateSchedule`, `UpdateSchedule`, `SetScheduleEnabled`, `DeleteSchedule`, `ListAllowlist`, and `SetAllowlist`.
- **Enforcement Integration**: `st-agent::enforcer` checks active minute against weekday bitmasks (supporting overnight rollover) on each tick; non-allowlisted apps are blocked with `OverlayMode::DowntimeActive`.

---

## 8. Timer Overlay (HUD Pill) Focus Reconciliation & Low-Latency Dismissal

### A. Problem Statement
Previously, switching focus away from an application with an active time limit (such as Notepad) caused the timer overlay (floating HUD pill) to linger for up to 1 second over unrelated desktop areas or subsequent applications before being dismissed.

### B. Architectural Root Cause
1. **Outbox Batch Lag in Ingest**: `ObsAccumulator` emits closed interval observations into `outbox`. When focus changes, `outbox` contained the *previous* application's observation slice. The agent server previously evaluated HUD state based on `report.observations.last()`, which falsely reported the prior app as active for that report cycle.
2. **Missing `hud_app` Association**: `SessionState` cached `sess.hud: Option<HudStateDto>` without storing which `AppKey` it belonged to. When the user switched focus to another application before the next 1 Hz report, the session's 100 ms tick mistakenly considered `sess.hud` valid for whatever window was in the foreground.

### C. Solution & Invariants
1. **Explicit Instantaneous Focus Field**: `ReportUsageDto` now carries an explicit `pub focused_key: Option<AppKey>` field (deserialized with `#[serde(default)]` for wire backward compatibility). The agent's `handle_report_usage` evaluates limits and generates HUD recommendations strictly against `report.focused_key`.
2. **Focus-Keyed Session Cache (`hud_app`)**: `SessionState` in `screentime-session` tracks `sess.hud_app: Option<AppKey>`. Whenever a HUD state is acknowledged by the agent, `sess.hud_app` is set to the current focused key.
3. **Immediate ≤100 ms Focus-Loss Dismissal**: In Step 7 of the session loop:
   ```rust
   let hud_matches_focus = sess.hud_app.as_ref() == current_key;
   if active.is_some() || current_key.is_none() || !hud_matches_focus {
       if let Some((_, run)) = active_hud.take() {
           run.dismiss();
       }
   }
   ```
   If focus moves to desktop/taskbar (`current_key.is_none()`) or another app (`!hud_matches_focus`), the timer HUD is destroyed immediately in the 100 ms loop cycle without waiting for the 1 Hz agent round-trip.
4. **Immediate Frame Dispatch on Focus Change**: Focus transitions immediately flag `effective_focused_changed = true`, triggering a `ReportUsage` frame dispatch on the current tick rather than waiting for the 1 Hz cadence.

---

## 9. HUD Overlay Graphics & Game Composition Interoperability

### A. Problem Statement
When running full-screen or borderless 3D games with driver-level frame rate limits (such as AMD Software: Adrenalin Edition Radeon Chill or Max Frame Rate / FRTC capped at 30 FPS), having the timer HUD overlay active over the game could cause the game to exceed the 30 FPS cap and render unthrottled.

### B. Root Causes
1. **DWM Presentation Demotion (Independent Flip to Composed Flip)**:
   - Modern Windows games present using **Independent Flip (iFlip)**, bypassing DWM composition and flipping buffers directly to hardware display planes. Driver limiters (AMD Chill / FRTC) hook directly into this hardware flip presentation queue.
   - Drawing a classic Win32 layered GDI window (`WS_EX_LAYERED | WS_EX_TOPMOST`) over the game's surface forces Windows DWM to fall back to **Composed Flip** (standard desktop composition).
   - In Composed Flip, AMD Adrenalin's driver hook detects that the swapchain is composited by DWM rather than hardware-exclusive, causing Radeon Chill or FRTC to disengage and leave frame pacing to DWM or unthrottled rendering (if in-game VSync is off).
2. **60 Hz `WM_TIMER` Wakeups & Redundant `ShowWindow` Calls**:
   - `TRACK_STEP_MS` was set to `16` ms (~60 Hz), and called `ShowWindow(hwnd, SW_SHOWNOACTIVATE)` 60 times a second on every tick. This continuously marked the DWM composition tree as dirty at 60 Hz, thrashing presentation timing.
3. **Missing `WS_EX_TOOLWINDOW`**:
   - The HUD lacked `WS_EX_TOOLWINDOW`, causing Windows and GPU driver hooks to treat it as an unowned top-level application window, disrupting driver active-surface detection.

### C. Refinements & True Architectural Solution
1. **Direct3D Fullscreen & Game Geometry Detection (`is_game_or_fullscreen`)**:
   - In `screentime-session`, before spawning or updating the timer HUD overlay, the session evaluates whether the focused window is a full-screen 3D game using two native signals:
     - **Windows Shell Notification Query**: Calls `SHQueryUserNotificationState()`. If it returns `QUNS_RUNNING_D3D_FULL_SCREEN`, Windows has detected an active full-screen DirectX/Vulkan game.
     - **Monitor Span & Taskbar Coverage**: Compares the window bounding rect against `MONITORINFO.rcMonitor` and `rcWork`. Borderless/exclusive fullscreen games span the entire monitor across the taskbar area without `WS_MAXIMIZE`.
   - When a full-screen game is detected, **the floating timer HUD is automatically suppressed and dismissed**:
     ```rust
     if is_game_or_fullscreen(snap.hwnd, snap.rect) {
         if let Some((_, run)) = active_hud.take() {
             run.dismiss();
         }
     }
     ```
2. **Preservation of Hardware Independent Flip & Driver Limiters**:
   - Because no external Win32 window is placed over the full-screen game, Windows DWM maintains pure **Independent Flip (iFlip) / DirectFlip**.
   - AMD Software: Adrenalin Edition driver limiters (**Radeon Chill**, **FRTC**, Max Frame Rate) remain 100% engaged at the configured 30 FPS cap.
   - Variable Refresh Rate (**AMD FreeSync** / **Nvidia G-Sync**) and HDR tonemapping operate without composition degradation or stutter.
3. **Continuous Tracking & Fail-Closed Enforcement**:
   - App usage sampling continues at 1 Hz in the background.
   - When the configured limit is reached, the Fail-Closed Block Screen engages to block the application as designed.
   - As soon as the user switches to a windowed application or the desktop, the timer HUD seamlessly returns.
4. **Window State Optimizations**:
   - Added `WS_EX_TOOLWINDOW` and removed redundant 60 Hz `ShowWindow` calls.
   - Tracking frequency relaxed to 100 ms (10 Hz) for windowed mode.

---

## 10. Semantic Token Alignment

Superseded by the v2 palette (Section 1 and `docs/DESIGN_SYSTEM.md`). What still holds:
- Component rules read semantic tokens (`--bg-card`, `--border-card`, `--text-primary`, `--color-primary`, …); light-only adjustments use `[data-theme-mode="light"]` and tokens, not hardcoded colours.
- Text uses the text-safe tokens (`--color-*-text`, `--color-on-primary`); fills use the fill tokens.
- `WebFilteringPanel.css`, the weekly chart, dialogs, PIN inputs and skeletons have no per-theme blocks; they follow the tokens.

---

## 11. Alert Sound & Milestone Chime Volume Control Pipeline

### A. End-to-End Audio Pipeline
- **Settings UI**:
  - Interactive slider in Settings -> "Alert Sounds & Milestone Chimes" ranging from `0%` (Muted) to `100%` in `5%` increments.
  - Interactive numerical percentage readout (`0%` - `100%`).
  - Integrated with `previewAlertSound(volume)` for real-time auditory feedback at the exact selected volume level.
- **Database & Agent Persistence**:
  - Stored in SQLite `settings` table as `alert_volume` (`0..100`, default `80`).
  - Reflected in `StatusDto.alert_volume` and broadcast to connected session clients.
- **In-Memory 16-Bit PCM WAV Volume Scaling**:
  - Both `st-session` (milestone chimes and block alerts) and `st-tauri` (settings preview) dynamically scale 16-bit PCM little-endian audio samples after the 44-byte standard RIFF header in memory:
    ```rust
    let new_sample = ((sample as i32 * volume_pct as i32) / 100).clamp(i16::MIN as i32, i16::MAX as i32) as i16;
    ```
  - At `volume_pct == 0`, audio playback is completely muted.
  - Zero disk I/O, zero external audio crate dependencies, zero audio latency.

---

## 12. HUD Timer Overlay Exit Animation & Hotkey Interruption

### A. 120 FPS Exit Animation Pipeline
- **Problem**: Previously, when the 4-second peek duration expired or HUD was dismissed, `run.dismiss()` called `WM_CLOSE` immediately, causing the overlay to vanish instantly with an abrupt cutout.
- **Solution**:
  - In `crates/session/src/hud.rs`, the HUD lifecycle is managed by an atomic phase controller (`HudAnimShared`): `Entering` (0), `Settled` (1), `Exiting` (2), `Closed` (3).
  - When peek duration runs out, `trigger_exit()` is invoked instead of immediate window destruction.
  - The exit animation runs at 120 FPS (~8.33 ms frame interval) over a 240 ms window using a cubic ease-in curve (`t^3`):
    ```rust
    let ease = t * t * t;
    let y = (start_y as f32 + (target_y - start_y) as f32 * ease).round() as i32;
    ```
  - The overlay glides smoothly off-screen back in the exact vertical direction it arrived from (e.g. retreating +24px off bottom or -24px off top).
  - Upon completion of the exit curve, the phase transitions to `Closed` and `WM_CLOSE` is dispatched cleanly.

### B. Hotkey Interruption & Dynamic Reversal
- If the user re-triggers the hotkey (`Ctrl+Alt+T`) while the HUD is in mid-exit:
  - `reverse_to_enter()` dynamically snapshots the current `cur_y` coordinate and reverses the trajectory towards `final_y` without resetting or jumping frames.
  - In `crates/session/src/main.rs`, during `is_exiting()` the main loop sleep relaxes from 1000 ms to 50 ms polling intervals, ensuring near-instant (<50 ms) responsiveness to hotkey presses during departure.

---

## 13. UI De-Cardenisation Architecture

### A. Unified Telemetry Bar (`MetricCards.tsx` & `MetricCards.css`)
- Replaces disjointed floating card boxes with a cohesive horizontal telemetry bar:
  - Enclosed in a single unified panel (`.metric-cards-grid`) with subtle 1px border (`var(--border-subtle)`).
  - Individual metric segments (`.metric-card-link`) are separated by vertical hairline dividers (`border-right: 1px solid var(--border-subtle)`).
  - Hover states apply clean surface tinting (`var(--bg-card-hover)`) without shifting surrounding geometry.

### B. Linear-Style Unified Settings Sections (`redesign.css`)
- Replaces 8 detached floating settings cards with unified grouped panels:
  - Settings controls are housed within cohesive panels with clean uppercase category headers and subtle hairline dividers between options.
  - Eliminates visual clutter while retaining tactile contrast and legible spacing.

---

## 14. Modernized Limit Creation & Block Overlay Experience

### A. Redesigned Limit Creation Dialog (`LimitEditorDialog.tsx`)
- Segmented target picker (`[ App ]` | `[ Category ]` | `[ Total Device ]`) with iconography.
- Direct-access duration slider coupled with dual hour/minute numerical inputs and rapid preset pills (`15m`, `30m`, `45m`, `1h`, `1.5h`, `2h`, `3h`, `4h`).
- Compact single-row weekday pill selector (`[M] [T] [W] [T] [F] [S] [S]`) replacing previous multi-row card stacks.

### B. Limit Block Overlay Animation & Cross-Window Theme Sync (`BlockOverlay.tsx`, `overlay_bridge.rs`)
- **Graceful Dismissal**: When limits thaw or are granted overrides, `overlay_bridge.rs` invokes `hide_overlay_gracefully` to trigger CSS exit animations (`overlay-exit-scale` & `backdrop-exit-fade`) before calling native `window.hide()`.
- **Theme Synchronization**: Listens for the `theme_changed` Tauri event to synchronize theme tokens across all windows in real time, eliminating hardcoded dark fallbacks and ensuring seamless visual consistency across light and dark modes.

---

## 15. UX Consistency Pass (2026-10)

Rules established by the October 2026 pass. Follow them in new UI:

- **Fonts are bundled** (now `@fontsource-variable/rubik`, `@fontsource-variable/unbounded`, `@fontsource-variable/jetbrains-mono`, imported in `main.tsx`; Hanken Grotesk until Phase 1). The Tauri CSP (`default-src 'self'`) blocks Google Fonts, so the designed typography previously never rendered in the shipped app, and never offline. Do not add remote font or stylesheet links.
- **Durations are amounts, not clocks.** `formatDuration` renders `6h 11m`, `44m`, `35s` with localized units (`time.short.*`). Only live countdowns (`LiveTimer`) use clock form. The dashboard hero renders the same parts as a large display figure with small muted units.
- **Honest states.** Unreachable agent → `.service-alert` banner and "Protection: Not running". An unknown catalog shows "—", never "No limits".
- **Progress carries state.** Limit cards use `rich-limit-progress-fill--ok | --warn (≥80%) | --over`. Never force a color with `!important` over a state class.
- **Budgets come from the agent.** A card's budget is the row's `limit_seconds` (weekday-aware). Disabled limits and the total limit fall back to today's weekday-resolved minutes.
- **Every user-visible string goes through i18n**, including `aria-label`, `title`, placeholders and toasts. Weekday names come from `Intl.DateTimeFormat` in the UI language (`weekdayShortNames()`, DowntimeSection `weekdays()`). Arabic plural forms (`_zero/_one/_two/_few/_many/_other`) are provided where counts appear. Numbers embedded in Arabic sentences are wrapped in Unicode isolates (U+2068/U+2069) so they don't reorder.
- **Filter tabs** keep stable keys and a separate labels map, so translation can't break filtering (`FilterTabs` itself was removed in Phase 1; segmented controls now use `aria-pressed`).
- **Copy**: plain verbs, sentence case, and the same name for an action through the whole flow ("Allow 15 more minutes"). The window's close button says what it does ("Close window (Tether keeps running)").
- **Removed dead components**: `Hero`, `LedgerSection`, `FrictionBanner`, `Masthead`, `Section`, `TetherLogo`. `noUnusedLocals` is on in `tsconfig.json`.
- **Accessibility**: icon-only states (narrow window nav, title bar, pagination, steppers) carry `aria-label`; the active nav item has `aria-current="page"`; limit progress bars are `role="progressbar"` with values; the offline alert is `role="alert"`.
- **"Today" follows the agent's day boundary.** `format.ts` mirrors `day_start_minutes` from every status poll (`setDayStartMinutes`), so `todayKey()` and the dashboard's rollover timer agree with how the agent buckets usage (a 04:00 day start keeps 02:00 on yesterday).
- **Dates and clock times use the UI language.** `formatDayLabel`, `formatClock` and the limit-change toast (`effectClause`: "Applied." / "Takes effect {{when}}.") format through `Intl` with `i18n.language`; there are no hardcoded English month or weekday tables in `format.ts`.
- **Limit editor copy speaks in limits, not ledger "orders".** "New limit" / "Save limit" / "Limit is on" / "Daily limit" / "Different limit on some days". Presets, the h/m inputs and the slider ticks format through `formatDuration`, so they localize. Slider tick labels sit at their true position on the 5–480 min linear scale (`--at`, `inset-inline-start`, RTL-safe); they used to be evenly spaced, which put "1h" at the 2h mark.
- **App directory** has its own description (`categorize.directoryDesc`), shows the executable path without the internal `exe:` key prefix (full key on hover), and its row action says "Change". The web filter's rule count is a plural key (`webFilter.ruleCount_*`).
- **Arabic typography**: no letter-spacing on Arabic (`:lang(ar) *`), and `--font-mono` falls through to a proportional face for Arabic glyphs while keeping JetBrains Mono for digits. Both tracking and monospace fallback were pulling connected letters apart.
- **One toggle switch definition** (`.toggle-switch` in `redesign.css`; the duplicate `app.css` block and every `!important` on it are gone, verified pixel-identical in all LTR scenarios). The knob travels toward the inline end (left = on in Arabic), and keyboard focus shows a `:focus-visible` ring (it was suppressed with `outline: none !important`).
- **Top bar CSS is defined once** (`redesign.css`). The old side-rail rules in `app.css` (from when the nav was a vertical sidebar) are gone, along with ~230 `!important`s on the top bar, status pill, day stepper and their light-theme overrides; 980px media declarations that could never apply were dropped instead of revived. Pixel-identical except two stray lines that the legacy rules drew: a hairline across the top bar beside the day stepper (`.sidebar-footer` `border-top`) and a vertical edge line in Arabic (`[dir="rtl"] .app-sidebar` `border-left`).
- Limit cards say "Same limit every day" / "Varies by day" for their schedule line (was a bare "/ day").
- **No ledger vocabulary in copy.** PIN prompts, toasts and errors talk about limits ("Pause the limit for TikTok", "Limit paused."), not "standing orders" or "the books". Some i18n keys keep their historical names (`actions.orderRecorded`, `pinGate.applyOrder`); only the values changed.
- **Accessibility audit (axe-core, every screenshot scenario).** All critical/serious findings except data-driven category pills are fixed:
  - **Text-safe color tokens.** Fill colors are not text colors. Each theme defines `--color-primary-text`, `--color-accent-text`, `--color-danger-text`, `--color-success-text`, `--color-warning-text` (≥4.5:1 on that theme's surfaces and tints) and `--color-on-primary` (text on a primary fill; dark on slate's light cyan). Any `color:` uses the `*-text` token; `background`/`border`/`accent-color` keep the fill token. Dark `--text-muted` was raised to `#7C8CA2` (midnight) / `#848D97` (slate).
  - **Names.** Every switch, number input, slider and select has an accessible name (`ToggleSwitch` takes `label`; settings controls reuse their visible label). The overlay keypad's icon keys are "Clear PIN" / "Confirm PIN".
  - **Structure.** The title bar is a `<header>`; onboarding and the overlay are `role="main"`. Each page has one level-1 heading (`aria-level={1}` on the visible page title, a `.sr-only` `h1` on the dashboard) and section headings are level 2.
  - Known gap: category pills draw text in the category's own palette color (some under 4.5:1). (The Nordic Frost theme with a 4.1:1 primary button was retired in Phase 1.)
- **Theme colors live in `redesign.css`.** `tokens.css` used to define all four palettes too, but `redesign.css` loads after it and overrode them. The 83 dead duplicates were removed by simulating the custom-property cascade for every theme (including Nordic Frost, which also matches `[data-theme-mode="light"]`) and keeping only declarations that change an effective value; screenshots are unchanged. `tokens.css` now holds the shared scales (fonts, spacing, radii, shadows, motion) and the few theme tokens `redesign.css` doesn't set.
- **Built-in category names are translated** once in `useDashboard` (`categoryNames.<slug>`). `categoryColors.ts` registers every localized name against its English palette entry, so colors never change with the language.
- **CSS hygiene**: about 2,600 lines of dead rules were removed (classes no source references, plus selectors for retired theme names that `useTheme` and the `index.html` bootstrap always migrate away from). Every screenshot scenario was verified pixel-identical before and after. The accent tokens are named `--redesign-accent` / `--redesign-accent-deep` (they were `--redesign-orange*` while holding cobalt). The Tauri `get_theme` / `set_theme` share one `THEMES` list and migrate retired names instead of accepting them.

---

## 16. Redesign Phase 1 (2026-10-03)

Implements Phase 1 of `docs/DESIGN_SYSTEM.md`:
- **New palette and type**: plum/orange light and dark themes replace the four previous palettes; Rubik (body, Arabic) and Unbounded (display) are bundled with fontsource (`@fontsource-variable/rubik`, `@fontsource-variable/unbounded`); JetBrains Mono stays for code. Hanken Grotesk was removed.
- **Information architecture**: Today · Limits · Settings. Web filtering moved into Limits.
- **Setup flow** replaces the onboarding slider and asks who Tether is for (new `profile` setting, see `docs/IPC_CATALOG.md`).
- **"Applies when"** note in the budget editor and a waiting banner with Cancel on Limits.
- **Removed**: `OnboardingSlider`, `LimitsPanel`, `FilterTabs` (and their CSS), about 470 lines of CSS that no longer matched any element.
- **Accessibility** (axe-core over every screenshot scenario, light and dark, plus Arabic and narrow): no new findings. Light success text darkened to `#176A52`. The known gap (category pills drawn in their palette colour on Today) remains until Phase 2.
- **Not yet**: Today restyle (ring, day strip, budget tiles), the "pause and choose" block screen, timer/tray restyle (Phase 2); budgets for custom app groups, borrowing time, reasons and suggestions from last week (need agent work, Phase 4); Activity.

---

## 17. Redesign Phase 2 (2026-10-04)

- **Today** (Section 3): ring, day strip, budget tiles, schedule and website cards; `todayModel.ts`.
- **Block screen** (Section 5B): "pause and choose", reason re-derived by `blockModel.ts`, extra time only when it can work, Escape no longer quits the app, DPI-scaled window floors.
- **Timer HUD** (Section 5A): plum palette shared by both renderers, orange last minute, `system` theme resolved.
- **Tray**: plain labels ("Reset network settings", "Stop Tether and its service", "Close the Tether app"), tooltip "Tether".
- **Installer**: styled NSIS installer, see `packaging/README.md`.
- The category-pill contrast gap from Phase 1 is gone (neutral category buttons).
- **Not yet**: a tray panel (the mockup's popup with the ring and budgets needs a new window), Activity, and Phase 4 agent work (custom budget groups, borrowing, reasons, suggestions).

