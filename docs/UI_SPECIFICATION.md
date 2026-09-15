# Tether UI Architecture & Redesign Specification

This document defines the frontend layout, design system tokens, and component architecture for the **Tether** dashboard. Verified against the codebase on **2026-09-13**.

---

## 1. Design System & Brand Identity: Tether Smoked-Glass Workspace

The application is branded as **Tether**, featuring an official brand mark depicting an architectural "T" encircled by an orbital tether connecting to a clock/timer orb in warm terracotta and caramel hues.

The UI uses a dark obsidian purple and warm amber smoked-glass aesthetic defined in `ui/src/styles/tokens.css`, `ui/src/styles/redesign.css`, and `ui/src/styles/app.css`:

### Core Palette (Color Hunt #210F374F1C51A55B4BDCA06D)
- **Obsidian Purple Base**: `#210F37` (`--bg-canvas`, `--bg-app`, `--redesign-bg`) — Deep background canvas.
- **Rich Purple Glass Surfaces**: `#4F1C51` (`--bg-card`, `--bg-surface`, `--redesign-panel`) — Floating glass cards with subtle border illumination.
- **Warm Terracotta Accents**: `#A55B4B` (`--accent-indigo`, `--color-primary`) — Primary action buttons, progress bars, category highlights.
- **Amber Gold Highlights**: `#DCA06D` (`--color-accent`, `--accent-amber`, `--redesign-orange`) — Badges, timer readouts, glowing PIN dots, active tab icons.
- **Danger / Urgent Warning**: `#DF5E4E` (`--color-danger`, `--accent-rose`) — Limit reached, destructive actions, error prompts.
- **Borders**: Translucent amber/terracotta rules (`rgba(220, 160, 109, 0.14)`).
- **Typography**: System sans-serif for headings/copy, monospace for tabular numbers and time codes.
- **Elevation & Blur**: `backdrop-filter: blur(24px); box-shadow: 0 16px 40px rgba(15, 5, 25, 0.45); border-radius: 14px;`.

---

## 2. Layout Structure & Top Navigation

The top chrome consists of a streamlined App Bar (TitleBar) and a horizontal Top Navigation Bar:

```
+-----------------------------------------------------------------------------------------+
| [TitleBar] Tether (34px, custom drag region, minimal title, minimize/maximize/close)    |
+-----------------------------------------------------------------------------------------+
| [Top Nav Bar: .app-sidebar] (58px sticky smoked glass)                                  |
|  [Tether Logo + Live Pill]  |  [Dashboard] [Web Filter] [App Limits] [Settings]  | (< Today >)  |
|                             |               (Horizontal Capsule)                 |             |
+-----------------------------------------------------------------------------------------+
| [Main Content Area: .app-main-content]                                                  |
|                                                                                         |
|  (Active Tab View: overview | limits | web-filtering | settings)                        |
|                                                                                         |
+-----------------------------------------------------------------------------------------+
```

### App Bar (`TitleBar.tsx` & `TitleBar.css`)
- **Height**: 34px, `background: rgba(22, 10, 36, 0.98)` matching deep horizon obsidian purple.
- **Left Cluster**: Clean "Tether" window title (`12px`, font-weight 650) without repeating the app logo.
- **Window Controls**: Minimize, Maximize/Restore, Close caption buttons (46px hit width, hover states, `#e81123` close hover, with `transform: none !important` to prevent active-state distortion).

### Navigation Bar (`Sidebar.tsx` & `redesign.css`)
- **Container**: 58px min-height, `rgba(26, 11, 42, 0.94)` smoked glass, `backdrop-filter: blur(20px) saturate(135%)`.
- **Brand Cluster**: Renders the 30px Tether squircle logo cleanly without any outer border/background wrapper, paired with "Tether" logotype (`1.05rem`, font-weight 750) and an inline **Live Status Pill** (`status-pill--live`).
- **Center Nav Segmented Capsule**: Horizontal row (`flex-direction: row !important`) hosting the 4 section tabs side-by-side in a sleek floating pill track. Active tab is highlighted with an elevated warm terracotta gradient (`rgba(165, 91, 75, 0.35)` to `rgba(122, 62, 48, 0.28)`) and amber icon accent.
- **Date Stepper Capsule**: Matching floating pill track on the right with responsive `<` and `>` buttons and an interactive "Today" / date label with calendar icon for returning to Today when viewing past ledger records.

### Nav Items
1. **Dashboard** (`overview`): Daily timeline, top applications, weekly chart, category distribution (includes blocked app count badge).
2. **Web Filter** (`web-filtering`): Domain block rules, bulk uploading, NSFW protection.
3. **App Limits** (`limits`): Target limits (apps and categories), status filters, limit creator.
4. **Settings** (`settings`): Language, Appearance, Advanced Parameters, Security & PIN, About.

---

## 3. Overview Tab Hierarchy (`overview`)

The dashboard overview is structured as an interactive grid:

```
+-----------------------------------------------------------------------------------------+
| .overview-stack                                                                         |
|                                                                                         |
|  [Row 1: .overview-activity-grid] (1.38fr : 0.72fr)                                     |
|  +--------------------------------------------+  +-----------------------------------+  |
|  | .timeline-panel (card)                     |  | <UsageAside />                    |  |
|  | - Header: "Today at a glance"              |  | - "Your activity / Most used"     |  |
|  | - Date Stepper: [<] Today [>]              |  | - Ranked list: 01, 02, 03...      |  |
|  | - <LedgerRule />: 24h timeline band        |  | - Duration bars & percentage      |  |
|  | - Category Color Legend (from catalog)     |  |                                   |  |
|  +--------------------------------------------+  +-----------------------------------+  |
|                                                                                         |
|  [Row 2: .overview-chart-grid] (1fr : 0.8fr)                                            |
|  +--------------------------------------------+  +-----------------------------------+  |
|  | .card (Weekly History)                     |  | <CategoryMix />                   |  |
|  | - 7-day bar chart (<WeeklyChart />)        |  | - "Where time goes"               |  |
|  | - Clickable day bars with hover tooltips   |  | - Conic-gradient donut breakdown  |  |
|  |                                            |  | - Legend with % of total          |  |
|  +--------------------------------------------+  +-----------------------------------+  |
+-----------------------------------------------------------------------------------------+
```

### Component Details
- **`LedgerRule.tsx`**: Visual 24-hour timeline bar (00 to 24 hours). Slices each usage interval, maps `appId` to `primary_category`, and renders each slice in its distinct category color using `categoryColors.ts`. Displays category legend at the bottom.
- **`UsageAside.tsx`**: Compact right-side card displaying strictly the top 5 ranked applications with visual progress bars colored by their category, live timer countdowns, dedicated category pills (`.usage-category-pill`), and streamlined vertical spacing (`overflow: hidden`) to eliminate scrollability completely within the card bounds.
- **`WeeklyChart.tsx`**: 7-day bar chart showing day-by-day totals with week-over-week deltas.
- **`CategoryMix.tsx`**: Circular conic-gradient donut chart showing time distribution by category using the high-contrast category color taxonomy.
- **`categoryColors.ts`**: Unified high-contrast color mapping across 16+ distinct categories (Games `#8B5CF6`, Social Media `#3B82F6`, Short-Form Video `#EC4899`, Video & Streaming `#EF4444`, Music `#10B981`, News `#F97316`, Shopping `#F59E0B`, Communication `#06B6D4`, Productivity `#0EA5E9`, Creativity `#A855F7`, Education `#14B8A6`, Finance `#84CC16`, AI `#6366F1`, Development `#64748B`, Utilities `#475569`, Adult `#BE123C`, Gambling `#991B1B`, Uncategorized `#94A3B8`), plus a 12-color fallback palette and alpha styling helpers.

---

## 4. Other Panels

### App Limits (`LimitsPanel.tsx`)
- Shell class: `.limits-page-shell`.
- Intro header + `+ Add New Limit` action button.
- Top metrics: Total Limits, Active Limits, Limits Reached.
- `FilterTabs`: "All", "Active", "Disabled".
- Limit cards grid displaying budget progress rings, weekday override badges, and quick toggles.
- **Active +15m Extension Live Timer**:
  - When an override is active (`activeTimerExpiresUtc`), the card receives an elevated amber border and glow (`.limit-card--extended`).
  - Card Header Badge: Displays `<LiveTimer />` with an animated pulsing dot and countdown indicator.
  - Card Body Banner (`.limit-card-extension-banner`): Dedicated frosted amber strip displaying `+15m Extension Active` with an animated pulsing dot and real-time second-by-second countdown.
  - Dashboard Integration: Also surfaces the live countdown pill next to the app name in `UsageAside` on the Overview tab.

### Web Filtering (`WebFilteringPanel.tsx`)
- Shell class: `.web-filter-page-shell`.
- Command Panel (`.web-filter-command-panel`):
  - Manual domain entry form input with real-time validation.
  - Bulk import via text/CSV file with 100-domain safety limit.
  - AI Prompt generator card with one-click clipboard copy.
- Domain Rules Table Panel (`.web-filter-domain-panel`):
  - Edge-to-edge full width table card (`padding: 0; overflow: hidden;`).
  - Table Header Bar (`.web-filter-table-header`): Title "Active Domain Rules", amber counter pill badge (`{count} rules`), and real-time search input with clear trigger.
  - Column Distribution:
    - Domain (45%): Rounded amber icon box with globe icon, ellipsis-clipped monospace domain text.
    - Category (23%): Pill badges distinguishing "Custom Block" from "Adult Content".
    - Status (16%): Monospace "BLOCKED" pill badge with glowing red status indicator.
    - Action (16%): Right-aligned "Remove" action button with danger-glow hover transition.
  - Table Footer (`.web-filter-table-footer`):
    - Left: Hidden NSFW domains unlock button (`EyeOff`).
    - Right: Pagination controls (`PAGE_SIZE = 10`) with count indicator ("1–10 of 14") and chevron stepper buttons.
  - Empty & Filter states: Dedicated empty views for zero domains and no search matches with a "Clear search" action.

### Settings (`App.tsx` Settings Tab)
- Shell class: `.settings-page`.
- Cards:
  - **Column 1**:
    - `.settings-card--language`: Language and color theme (Dark, Light, System).
    - `.settings-card--hud`: **Timer HUD & Gaming Overlay**:
      - Continuous timer HUD toggle (`show_hud_overlay`): Enables continuous floating timer over limited apps. When disabled, the shortcut keybind still functions on demand.
      - Continuous timer in full-screen games toggle (`show_hud_in_fullscreen`, defaults to disabled to preserve native display refresh rates, DWM Independent Flip, and driver frame limiters; recommend using in-game peek shortcut).
      - Timer peek shortcut (`hud_peek_hotkey`, default `Ctrl+Alt+T`) with interactive keyboard recorder and reset button. Operates universally across all apps and games, even when continuous HUD is disabled.
    - `.settings-card--alerts`: **Alert Sounds & Milestone Chimes**:
      - Visual countdown milestone pills (`15 min`, `10 min`, `5 min`, `1 min`, and `Limit Reached`).
      - Explanatory copy detailing the gentle, non-intrusive harmonic chime played when milestones are reached.
      - "Preview Chime Sound" button with live audio playback via Win32 `PlaySoundW` and speaker icon (`VolumeIcon`).
  - **Column 2**:
    - `.settings-card--security`: Admin PIN, Strict Mode toggle, Family DNS Protection toggle with status indicator dot.
    - `.settings-card--advanced`: Anti-impulse cooldown (hrs), idle threshold (sec), day rollover offset (min).
    - `.settings-card--categories`: Application Directory launcher with detected app count.
  - **Full Width**:
    - `.settings-card--tutorial`: Collapsible onboarding tutorial walkthrough.
    - `.settings-card--about`: Version and mode telemetry.
- **Loading Animations Across Settings**: Every toggle (`show_hud_overlay`, `show_hud_in_fullscreen`, `strict_mode`, `family_dns`) and advanced parameter input displays a dedicated `<LoadingSpinner />` while persisting changes and locks input to prevent race conditions.

### Application Directory & Categorization
- **Interactive Category Pills**:
  - `UsageAside.tsx`: In the "Most used" activity list, `.usage-category-pill` has `.usage-category-pill--clickable` with hover illumination and click-to-categorize trigger, opening `<CategorizeDialog />`.
  - `LimitsPanel.tsx`: App limit cards feature clickable `.target-badge--category-tag.target-badge--clickable` badges to re-categorize apps directly from their limits. A "Manage Apps" header button (`<FolderTree />`) provides quick entry to the full directory.
  - `LimitEditorDialog.tsx`: When an app target is selected, displays an inline category preview badge and a "Change" button.
- **`<AppDirectoryDialog.tsx>`**:
  - Full-screen modal for managing discovered applications on the machine.
  - Live search input with instant filtering across app display names, keys, and category names.
  - Status filter chips: All, Categorized, and Uncategorized.
  - App row layout with category color swatches, app names, keys, `[Custom]` badges for user-classified entries, and direct category change triggers.
- **`<CategorizeDialog.tsx>`**:
  - Theme-aware selection dropdown (`CategorySelect`) styled with CSS tokens (`var(--bg-card-elevated)`, `var(--border-subtle)`, `var(--bg-surface-hover)`) for dark and light modes.
  - Multi-tag category checkboxes and "Auto-detect" reset button.

### Onboarding Flow (`OnboardingSlider.tsx`)
- Multi-step first-run wizard:
  1. Welcome to Tether
  2. Language Selection (English / العربية)
  3. Track Your Time
  4. Set Limits
  5. **Family DNS Protection**: Interactive card allowing the user to opt-in to system-wide adult content filtering with automatic preservation of existing DNS, backed by `<ToggleSwitch />` and `<LoadingSpinner />`.
  6. Stay Focused

### App-Wide Loading Animations & Shimmer Skeletons
- **`LoadingSpinner.tsx`**: Reusable SVG vector spinner with animated dashed stroke and smooth rotation in sizes `xs`, `sm`, `md`, and `lg`. Integrated into form submissions, PIN verification buttons (`PinGate`, `PinSetupDialog`), and order updates (`LimitEditorDialog`).
- **Web Filtering Shimmer Skeleton**: When querying domain rules, the table renders animated multi-column skeleton rows (`.skeleton-shimmer`) with staggered widths in Color Hunt obsidian/terracotta gradients instead of static loading text.
- **Async Action Buttons**: Bulk upload in `WebFilteringPanel` displays `<LoadingSpinner size="xs" />` during multi-domain parsing and importation.

---

## 5. Overlays: Win32 HUD & Hardware-Accelerated Block Overlay

### A. Timer HUD Overlay (`crates/session/src/hud.rs`)
- **Visual Design**: High-contrast 92x28 pill (14px corner radius) with smoked-glass translucency (alpha 235/255).
  - Background: Obsidian purple `#210F37` (`rgb(0x21, 0x0F, 0x37)`).
  - Border: Amber gold `#DCA06D` (`rgb(0xDC, 0xA0, 0x6D)`) when active timer, or warm terracotta `#A55B4B` (`rgb(0xA5, 0x5B, 0x4B)`) in normal budget tracking.
  - Indicator: 4px glowing indicator dot on the left.
  - Digits: Bold monospace ClearType `Consolas` tabular time readout in amber gold or ivory white.
- **Hit-Testing Transparency & Native Cursor Passthrough**:
  - Registered with `hCursor = LoadCursorW(None, IDC_ARROW)` so Windows never falls back to an uninitialized or `IDC_APPSTARTING` (loading spinner) cursor.
  - Intercepts `WM_NCHITTEST` and returns `HTTRANSPARENT`, making the window completely transparent to mouse clicks and cursor hover so underlying game or desktop interactions and custom cursors remain 100% active and uninhibited.
  - Handles `WM_SETCURSOR` defensively to immediately set standard arrow without wait-cursor artifacts.
- **Real-Time Window Clamping**:
  - Attached via `SetTimer(hwnd, TRACK_TIMER_ID, TRACK_STEP_MS, None)` (100 ms / 10 Hz position reconciliation).
  - Follows `target_hwnd` position in real-time, clamping to the top-center of the application frame: `wr.top + 10`.
  - Auto-hides on app minimize (`IsIconic`) and restores smoothly when reopened.
  - Session loop preserves the window and only calls `run.update(...)`, respawning only when application focus changes.

### B. Hardware-Accelerated Tauri 2 React Block Overlay (`BlockOverlay.tsx` & `overlay_bridge.rs`)
- **Architecture**: Replaces legacy hand-drawn GDI and the global `WH_KEYBOARD_LL` hook with a hardware-accelerated transparent secondary webview window in Tauri 2 commanded over named pipe `\\.\pipe\screentime_overlay_bridge`.
- **Two-Tier Sampling & Reconciliation Cadence**:
  - **100 ms Overlay & Focus Loop**: The session helper polls `take_sample()` at 10 Hz (100 ms interval). Focus transitions to and away from blocked apps react within ≤100 ms (down from legacy 1,000 ms lag), making the block screen snap into place and dismiss with near-instant responsiveness.
  - **1 Hz Ingest Grid Preservation**: Normal usage accumulation (`ObsAccumulator::offer`) and persistent pipe communication (`run_frames`) are bounded to ~1 Hz (plus immediate focus boundary flushes), preserving the agent's database write contract and zero-noise logging policy.
- **Window Title Discrimination**:
  - `is_overlay_window_focused()` inspects the Win32 foreground window via `GetWindowTextW`.
  - When the user focuses the secondary block screen (`"Tether Overlay"`), input is preserved for PIN entry and extension.
  - When the user clicks the primary Tether Dashboard (`"Tether"`), the helper correctly treats this as moving focus away from the blocked app, instantly dismissing the block card and preventing the overlay from locking out the main application.
- **Immediate Dismissal & Ghost Click Elimination**:
  - On `OverlayBridgeRequest::Hide`, Tauri immediately calls `window.hide()` in 0 ms (rather than keeping an invisible/semi-transparent topmost window floating over the desktop for 240 ms).
  - This eliminates ghost click stealing and prevents focus oscillation loops where mouse clicks near the overlay area would reactivate `screentime-ui.exe` and cause jitter or glitched lingering.
- **Native OS Input Blocking Without Hooks**:
  - Upon limit trip, `screentime-session` calls `EnableWindow(target_hwnd, FALSE)` to make the blocked application completely inert to mouse, keyboard, and drag events natively at the Win32 OS level.
  - This eliminates machine-wide key swallowing, allows uninhibited `Alt+Tab` and Windows key usage, and removes antivirus/EDR false positives.
  - Upon unlock or focus dismissal, `screentime-session` calls `EnableWindow(target_hwnd, TRUE)` to instantly re-enable normal app interaction.
- **Visual Design**:
  - Full-window Backdrop: `rgba(12, 14, 20, 0.78)` smoked glass with hardware `backdrop-filter: blur(24px)`.
  - Centered Obsidian Card: 480px floating card (`--bg-card-elevated`) with warm border illumination (`--border-strong`), matching `redesign.css`.
  - Header: Tether brand lock badge paired with `LIMIT REACHED` or `DOWNTIME ACTIVE` status pill.
  - App Label: Bold display typography with category accent dot.
  - Dual Input PIN Support:
    - **Physical Keyboard**: Global listener intercepts digits `0–9`, `Backspace`, `Enter` (to submit), and `Escape` (to quit app).
    - **3x4 Tactile On-Screen Keypad**: Tactile numeric grid with interactive hover, active scaling, and Clear (`C`) / Submit (`OK`) keys.
    - **Masked Indicator Dots**: Glowing indicator bubbles with warm terracotta illumination and dynamic shake keyframe animation on invalid entry.
  - Multi-Lingual & RTL: Full bilingual support in English and Arabic (`dir="rtl"`), ensuring native bidirectional layout and typography.

---

## 6. Theme System & Component Standardization (Horizon Dark, Horizon Light, Classic Dark, Classic Light, System)

### A. Quad-Theme Engine & Zero-Emoji Architecture
- **Hook (`useTheme.ts`)**: Manages theme state (`horizon-dark`, `horizon-light`, `classic-dark`, `classic-light`, `system`), persists choice in `localStorage` under `tether_theme`, and sets `<html data-theme="..." data-theme-mode="dark|light">`.
- **Zero FOUC Startup Script (`index.html`)**: An early synchronous script in `<head>` immediately reads `localStorage.getItem("tether_theme")`, resolves system preference, and stamps `data-theme` before CSS or React mounts, ensuring zero visual flash on app restarts.
- **Backwards Compatibility**: Automatically migrates legacy `"dark"` to `"horizon-dark"` and `"light"` to `"horizon-light"`.
- **Zero-Emoji Theme Selector (`App.tsx`)**: Replaced raw emojis (`🌙`, `☀️`, `⊙`) with clean, professional localized buttons featuring live color swatch indicators:
  1. **Horizon Dark**: Signature warm obsidian floor (`#210F37`), elevated cards (`rgba(79, 28, 81, 0.80)`), warm terracotta (`#A55B4B`) and amber gold (`#DCA06D`) accents.
  2. **Horizon Light**: Signature warm cream canvas (`#F5EDE8`), parchment cards (`rgba(255, 250, 247, 0.95)`), soft terracotta lines (`rgba(165, 91, 75, 0.20)`), and amber highlights (`#8B6B1A`).
  3. **Classic Dark**: Clean, neutral dark slate canvas (`#0B0F17`), deep slate cards (`#141A26`), modern indigo accents (`#6366F1`), and cyan telemetry (`#38BDF8`) with 0% purple tint.
  4. **Classic Light**: Clean, neutral cool slate canvas (`#F8FAFC`), pure white cards (`#FFFFFF`), slate dividers (`#CBD5E1`, `#E2E8F0`), and royal blue accents (`#2563EB`) with 0% warm terracotta tint.
  5. **System**: Automatically evaluates OS `prefers-color-scheme: dark` in real time with reactive media query change listeners.

### B. Standardized Switch Toggles
- **Implementation**: Standardized across Settings, App Limits, and Onboarding via semantic `<input type="checkbox" className="toggle-switch" role="switch">`.
- **Geometry**: Precision 40x22px capsule with 16px thumb, 2px padding, and smooth cubic-bezier transitions (`translateX(18px)`).
- **Light Theme Cohesion**: Soft terracotta track when unchecked (`rgba(165, 91, 75, 0.18)`), vibrant terracotta when checked (`#A55B4B`), and clean white thumb (`#FFFFFF`) with zero overflow or double-rendering artifacts.

### C. Stabilized Top Date Stepper
- Fixed `196px` width with `justify-content: space-between` and flex `min-width: 0` ensures the date pill and central navigation tabs never shift or resize when toggling between "Today" and past date ranges.

---

## 7. Scheduled Downtime & Bedtime Mode (Phase A)

### A. Sub-Navigation Architecture in App Limits
- **Segment Control (`FilterTabs`)**: Integrates seamlessly at the top of `LimitsPanel.tsx`, offering instant switching between **"Daily Limits"** and **"Scheduled Downtime"**.
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

## 10. Multi-Theme Architecture & Semantic Token Alignment

The application supports four distinct, beautifully tuned visual themes plus OS automatic matching:
1. **Horizon Dark** (`horizon-dark`): Signature warm obsidian base (`#210F37`), deep purple cards (`#4F1C51`), terracotta primary buttons (`#A55B4B`), and amber gold accents (`#DCA06D`).
2. **Horizon Light** (`horizon-light`): Signature warm cream parchment base (`#F5EDE8`), elevated paper cards (`#FFFAF7`), dark espresso ink (`#1E0A05`), and warm terracotta accents (`#A55B4B`).
3. **Classic Dark** (`classic-dark`): Neutral slate canvas (`#0B0F17`), deep charcoal cards (`rgba(20, 26, 38, 0.75)`), crisp slate ink (`#F1F5F9`), and modern indigo & cyan accents (`#6366F1`, `#38BDF8`).
4. **Classic Light** (`classic-light`): Neutral slate light canvas (`#F8FAFC`), pure white cards (`#FFFFFF`), dark slate ink (`#0F172A`), and vibrant royal blue & emerald accents (`#2563EB`, `#059669`).

### A. Semantic Surface Hierarchy & Theme Selectors
- **Unified Dual-Selector Syntax**:
  - To maintain backward compatibility with legacy `"light"` while supporting explicit `"horizon-light"`, all light theme component rules in `redesign.css` utilize `:is([data-theme="light"], [data-theme="horizon-light"])`.
  - Classic Dark uses `[data-theme="classic-dark"]` and Classic Light uses `[data-theme="classic-light"]`.
- **Top Navigation Bar & App Bar Theming**:
  - The top App Bar (`.titlebar`) dynamically inherits `var(--bg-app)` and `var(--border-subtle)` with high-contrast text and control buttons across all 4 themes.
  - The sticky Navigation Bar (`.app-sidebar`), segmented button capsule (`.sidebar-nav`, `.nav-item`), and day stepper (`.day-stepper`, `.stepper-btn`) adapt seamlessly with warm parchment surfaces and espresso ink in Horizon Light, crisp slate in Classic Dark, and clean neutral slate in Classic Light.

### B. Component Visual Overhauls
- **Site Block List (`WebFilteringPanel.css`)**:
  - Completely decoupled from hardcoded dark purple (`#210F37` / `#4F1C51`).
  - Base rules derive entirely from semantic tokens (`var(--bg-card)`, `var(--border-card)`, `var(--bg-input)`, `var(--text-primary)`, `var(--color-primary)`).
  - Four explicit theme blocks provide tailored card surfaces, table headers, count badges, search inputs, and domain removal buttons for Horizon Dark, Horizon Light, Classic Dark, and Classic Light.
- **App Limits Panel (`LimitsPanel.css`)**:
  - Overhauled with semantic tokens and theme override blocks for consistent card elevation, progress tracks, and extension banners across all 4 palettes.
- **Weekly Chart Bars**:
  - In Horizon Light, `.bar-track` uses `rgba(165, 91, 75, 0.08)` and `.bar-fill` uses `rgba(165, 91, 75, 0.28)`, with selected bars highlighted in primary terracotta `#A55B4B`.
- **Modals, Dialogs & Controls**:
  - `.dialog`, `.pin-input`, `.toggle-switch`, and `.skeleton-shimmer` dynamically update their background, borders, and animations based on the active theme.

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

