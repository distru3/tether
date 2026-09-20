# Tether UI Architecture & Redesign Specification

This document defines the frontend layout, design system tokens, and component architecture for the **Tether** dashboard. Verified against the codebase on **2026-09-13**.

---

## 1. Design System & Brand Identity: Tether Solid Modern Workspace

The application is branded as **Tether**, featuring an official brand mark depicting an architectural "T" encircled by an orbital tether connecting to a clock/timer orb in warm terracotta and caramel hues.

The UI utilizes a crisp, solid, high-contrast modern aesthetic (Obsidian Dark & Titanium Light) defined in `ui/src/styles/tokens.css`, `ui/src/styles/redesign.css`, and `ui/src/styles/app.css`. All glassmorphism (`backdrop-filter: blur`, semi-transparent frosted cards) has been completely eliminated in favor of opaque, tactile surfaces with 1px border contrast:

### Core Palettes (4 Modern Cohesive Palettes)

#### A. Midnight Cobalt (Signature Modern Dark Theme)
- **Base Canvas**: `#0B0E17` (`--bg-app`, `--redesign-bg`) — Deep carbon base.
- **Solid Surfaces**: `#121724` (`--bg-panel`, `--bg-card`, `--redesign-panel`) — Sleek carbon panel level 1.
- **Elevated Surfaces**: `#182030` (`--bg-card-elevated`, `--redesign-panel-strong`) — Elevated carbon dialogs and cards.
- **Recessed / Inputs**: `#0E121D` (`--bg-input`, `--bg-recessed`) — Recessed carbon inputs.
- **Borders**: `#1E2638` (`--border-card`), `#161D2B` (`--border-subtle`), `#2A364F` (`--border-strong`).
- **Typography**: `#F8FAFC` primary text (crisp slate white), `#94A3B8` secondary slate, `#64748B` muted text.
- **Accents**: Electric Cobalt `#4F46E5` (`--color-primary`), Indigo `#6366F1`, Emerald `#10B981`, Rose `#F43F5E`.
- **Character**: Precision dark cockpit / technical analytics workspace.

#### B. Slate Charcoal (Clean Neutral Dark Charcoal & Ice Cyan)
- **Base Canvas**: `#0D1117` (`--bg-app`, `--redesign-bg`) — Clean dark charcoal base.
- **Solid Surfaces**: `#161B22` (`--bg-panel`, `--bg-card`, `--redesign-panel`) — Dark graphite panels.
- **Elevated Surfaces**: `#21262D` (`--bg-card-elevated`, `--redesign-panel-strong`).
- **Recessed / Inputs**: `#090D12` (`--bg-input`, `--bg-recessed`).
- **Borders**: `#30363D` (`--border-card`), `#21262D` (`--border-subtle`), `#3D444D` (`--border-strong`).
- **Typography**: `#F0F6FC` primary text (crisp neutral silver-white), `#8B949E` secondary graphite, `#6E7681` muted text.
- **Accents**: Ice Cyan `#38BDF8` (`--color-primary`), Polar Blue `#0284C7`, Emerald `#3FB950`, Rose `#F85149`.
- **Character**: Minimalist, distraction-free neutral dark workspace.

#### C. Clean Titanium (Pure White Surfaces & Soft Porcelain)
- **Base Canvas**: `#F8FAFC` (`--bg-app`, `--redesign-bg`) — Soft porcelain canvas, easy on eyes.
- **Solid Surfaces**: `#FFFFFF` (`--bg-panel`, `--bg-card`, `--redesign-panel`) — Pure white cards.
- **Elevated Surfaces**: `#FFFFFF` with soft elevation.
- **Recessed / Inputs**: `#F1F5F9` (`--bg-input`, `--bg-recessed`).
- **Borders**: `#E2E8F0` (`--border-card`), `#EEF2F6` (`--border-subtle`), `#CBD5E1` (`--border-strong`).
- **Character**: Pure architectural minimalism / macOS studio workspace.

#### D. Nordic Frost (Icy Slate Canvas & Arctic Cyan/Teal Theme)
- **Base Canvas**: `#F0F4F8` (`--bg-app`) — Icy slate canvas.
- **Solid Surfaces**: `#FFFFFF` (`--bg-panel`, `--bg-card`) — Crisp frosted white panels.
- **Elevated Surfaces**: `#FFFFFF` with subtle frosty shadow.
- **Recessed / Inputs**: `#FFFFFF`.
- **Borders**: `#D0DEEB` (`--border-card`), `#DFE8F1` (`--border-subtle`), `#B8CDE0` (`--border-strong`).
- **Typography**: `#0C1A24` primary text, `#304856` secondary text, `#5C7688` muted text.
- **Accents**: Arctic Cyan `#0284C7` (`--color-primary`), Arctic Teal `#0D9488`.
- **Character**: Crisp Nordic clarity / glacial precision workspace.

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
- **Native OS Input Blocking Without Hooks**:
  - Upon limit trip, `screentime-session` calls `EnableWindow(target_hwnd, FALSE)` to make the blocked application completely inert to mouse, keyboard, and drag events natively at the Win32 OS level.
  - Eliminates machine-wide key swallowing, allows uninhibited `Alt+Tab` and Windows key usage, and removes antivirus/EDR false positives.
  - Upon unlock or focus dismissal, `screentime-session` calls `EnableWindow(target_hwnd, TRUE)` to instantly re-enable normal app interaction.
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
1. **Midnight Cobalt** (`midnight-cobalt`): Deep carbon base (`#0B0E17`), sleek steel borders (`#1E2638`), crisp titanium text (`#F1F5F9`), and electric cobalt/indigo accents (`#4F46E5`, `#6366F1`).
2. **Slate Charcoal** (`slate-charcoal`): Clean dark charcoal base (`#0D1117`), graphite panels (`#161B22`), crisp silver-white text (`#F0F6FC`), and ice cyan accents (`#38BDF8`, `#0284C7`).
3. **Clean Titanium** (`clean-titanium`): Soft porcelain canvas (`#F8FAFC`), pure white surfaces (`#FFFFFF`), deep slate text (`#0F172A`), and royal cobalt accents (`#4338CA`, `#4F46E5`).
4. **Nordic Frost** (`nordic-frost`): Icy slate canvas (`#F0F4F8`), crisp white panels (`#FFFFFF`), deep fjord navy text (`#0C1929`), and arctic cyan/teal accents (`#0284C7`, `#0D9488`).

### A. Semantic Surface Hierarchy & Theme Selectors
- **Unified Dual-Selector Syntax**:
  - To maintain backward compatibility with legacy preferences while supporting new themes, all component rules in `redesign.css` support both new IDs (`midnight-cobalt`, `slate-charcoal`, `clean-titanium`, `nordic-frost`) and aliases (`dark`, `light`, `horizon-dark`, `horizon-light`, `classic-dark`, `classic-light`).
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

