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
1. **Today** (`overview`, `nav.overview`): the day's usage (Section 3).
2. **Limits** (`limits`, `nav.limits`): every rule in one place: budgets, schedules, websites (Section 4).
3. **Settings** (`settings`): six short groups (Section 4).

There is no separate Web Filtering tab any more; websites live on Limits.

---

## 3. Overview Tab Hierarchy (`overview`)

The dashboard is a vertical stack. The **only** day picker is the one in the top bar; it is hidden (layout-preserving `visibility: hidden`) on every other tab.

```
+-----------------------------------------------------------------------------------------+
| .service-alert (only while the agent is unreachable; role="alert")                      |
|   "Tether's background service isn't responding" + what that means + raw error          |
+-----------------------------------------------------------------------------------------+
| <ExecutiveHeader />                                                                     |
|  +-------------------------------+---------------+----------------+------------------+  |
|  | date eyebrow [Back to today]  | LONGEST       | CLOSEST LIMIT  | PROTECTION       |  |
|  | 6h 11m  (display figure)      | STRETCH  39m  | 23m left       | Enforcing / Not  |  |
|  | +7% vs 7-day average          | in Minecraft  | Games          | running          |  |
|  | [Mostly Games · 37%]          |               | [n reached]    | 1 blocked · DNS  |  |
|  +-------------------------------+---------------+----------------+------------------+  |
+-----------------------------------------------------------------------------------------+
| <BlockedBanner /> (only when something is blocked)                                      |
|   "1 app blocked until the day resets" · Resets in Xh Ym · [Allow 15 more minutes]      |
+-----------------------------------------------------------------------------------------+
| .activity-ledger-container                                                              |
|  +--------------------------------------------+  +-----------------------------------+  |
|  | Timeline (eyebrow = viewed day)            |  | <UsageAside /> Most used (top 5)  |  |
|  | <LedgerRule />: 24h band + legend          |  | [All apps]                        |  |
|  +--------------------------------------------+  +-----------------------------------+  |
| .analytics-trends-container                                                             |
|  +--------------------------------------------+  +-----------------------------------+  |
|  | This week: <WeeklyChart />                 |  | <CategoryMix /> Where time goes   |  |
|  +--------------------------------------------+  +-----------------------------------+  |
+-----------------------------------------------------------------------------------------+
```

The header metrics are derived in `ui/src/dashboardMetrics.ts` (pure functions):
- **Closest limit** (`limitOutlook`): uses the agent's per-row `limit_seconds`, defined so that `limit_seconds - seconds` is the time left on that row's own enabled limit. Weekday overrides are applied, and category rows count tagged apps. The total-screen-time limit is resolved from the catalog for the viewed weekday. Rows inside an active "+15 min" extension are skipped. (This replaced a "Daily allowance" figure that summed every limit's default minutes.)
- **Longest stretch** (`longestStretch`): the longest single interval and its app.
- **vs 7-day average** (`vsWeekAverage`): against the other active days of the week; hidden until there is a baseline.
- **Protection** reflects *agent reachability*: when the poll fails it reads "Not running" in the danger style, never "Enforced".

### Component Details
- **`LedgerRule.tsx`**: Visual 24-hour timeline bar (00 to 24 hours). Slices each usage interval, maps `appId` to `primary_category`, and renders each slice in its distinct category color using `categoryColors.ts`. Displays category legend at the bottom.
- **`UsageAside.tsx`**: Compact right-side card displaying strictly the top 5 ranked applications with visual progress bars colored by their category, live timer countdowns, dedicated category pills (`.usage-category-pill`), and streamlined vertical spacing (`overflow: hidden`) to eliminate scrollability completely within the card bounds.
- **`WeeklyChart.tsx`**: 7-day bar chart showing day-by-day totals with week-over-week deltas.
- **`CategoryMix.tsx`**: Circular conic-gradient donut chart showing time distribution by category using the high-contrast category color taxonomy.
- **`categoryColors.ts`**: Unified high-contrast color mapping across 16+ distinct categories (Games `#8B5CF6`, Social Media `#3B82F6`, Short-Form Video `#EC4899`, Video & Streaming `#EF4444`, Music `#10B981`, News `#F97316`, Shopping `#F59E0B`, Communication `#06B6D4`, Productivity `#0EA5E9`, Creativity `#A855F7`, Education `#14B8A6`, Finance `#84CC16`, AI `#6366F1`, Development `#64748B`, Utilities `#475569`, Adult `#BE123C`, Gambling `#991B1B`, Uncategorized `#94A3B8`), plus a 12-color fallback palette and alpha styling helpers.

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
- **Visual Design**: Solid, high-contrast 92x28 pill (14px corner radius) with zero glassmorphism.
  - Dark Theme: Solid Obsidian `#0B0D13` background, `#202534` 1px border, `#F3F4F6` digits.
  - Light Theme: Solid Titanium `#FFFFFF` background, `#E5E7EB` 1px border, `#111827` digits.
  - Dynamic Status Dot:
    - Normal Active Tracking: Electric Cobalt `#3B82F6` (D2D / GDI).
    - +15m Extension Timer: Amber Gold `#F59E0B`.
    - Warning (≤60 seconds): Rose Coral `#F43F5E`.
  - Digits: Bold monospace ClearType `Consolas` tabular time readout.
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
  - Overlays continuously adapt to the user's active theme by synchronizing with `%LOCALAPPDATA%\screentime\theme.txt`.

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
- **Sizing**: at least 520×680 with the PIN pad (460 tall without), centered on the app and clamped to its monitor. The card scrolls instead of clipping, so "Allow 15 more minutes" and "Quit app" are always reachable. (The old 480px floor cut both off.)
- **Quit is honest**: the overlay hides only after the app actually closed (local close, or the agent's `CloseApps`, which needs no PIN for a *blocked* app). Otherwise it shows "Couldn’t close the app: …" and stays up.
- **Peer verification**: both bridge ends check the other process is a genuine Tether binary from the same install (`st_win32::peer_is_trusted`).
- **Visual Design (Solid Crisp Styling — No Glassmorphism)**:
  - Full-window Backdrop: Solid high-opacity veil (`var(--bg-modal-backdrop, rgba(11, 13, 19, 0.92))`).
  - Centered Solid Card: 480px card (`--bg-card, #131620`) with crisp 1px border (`--border-card, #202534`) and deep elevation shadow (`0 24px 64px rgba(0, 0, 0, 0.6)`).
  - Header: Tether brand lock badge paired with `LIMIT REACHED` status pill.
  - App Label: Bold display typography with category accent dot.
  - Dual Input PIN Support:
    - **Physical Keyboard**: Global listener intercepts digits `0–9`, `Backspace`, `Enter` (to submit), and `Escape` (to quit app).
    - **3x4 Tactile On-Screen Keypad**: Tactile numeric grid with interactive hover, active scaling, and Clear (`C`) / Submit (`OK`) keys.
    - **Masked Indicator Dots**: Glowing indicator bubbles with warm terracotta illumination and dynamic shake keyframe animation on invalid entry.
  - Multi-Lingual & RTL: Full bilingual support in English and Arabic (`dir="rtl"`), ensuring native bidirectional layout and typography.

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
