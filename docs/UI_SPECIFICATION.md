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
| [TitleBar] Tether (34px, custom drag region, logo, version tag, minimize/maximize/close)|
+-----------------------------------------------------------------------------------------+
| [Top Nav Bar: .app-sidebar] (58px sticky smoked glass)                                  |
|  [Tether Logo + Live Pill]  |  ( [Dashboard] [Web Filter] [App Limits] [Settings] )  |  |
|                             |               (Segmented Capsule)       | (< Today >)   |
+-----------------------------------------------------------------------------------------+
| [Main Content Area: .app-main-content]                                                  |
|                                                                                         |
|  (Active Tab View: overview | limits | web-filtering | settings)                        |
|                                                                                         |
+-----------------------------------------------------------------------------------------+
```

### App Bar (`TitleBar.tsx` & `TitleBar.css`)
- **Height**: 34px, `background: rgba(22, 10, 36, 0.98)` matching deep horizon obsidian purple.
- **Left Cluster**: 16px Tether brand logo, crisp "Tether" title (`12px`, font-weight 650), and `v0.1` version badge.
- **Window Controls**: Minimize, Maximize/Restore, Close caption buttons (46px hit width, hover states, `#e81123` close hover, with `transform: none !important` to prevent active-state distortion).

### Navigation Bar (`Sidebar.tsx` & `redesign.css`)
- **Container**: 58px min-height, `rgba(26, 11, 42, 0.94)` smoked glass, `backdrop-filter: blur(20px) saturate(135%)`.
- **Brand Cluster**: 34x34px rounded Tether emblem with warm shadow, "Tether" logotype (`1.02rem`, font-weight 750), and adjacent inline **Live Status Pill** (`status-pill--live` with green pulsing dot).
- **Center Nav Segmented Capsule**: Floating pill track (`rgba(16, 7, 28, 0.68)`) hosting 4 tab buttons. Active tab highlighted with elevated warm terracotta gradient (`rgba(165, 91, 75, 0.35)` to `rgba(122, 62, 48, 0.28)`) and amber icon highlight. Old dangling underline removed.
- **Date Stepper Capsule**: Matching floating pill track on the right with previous/next buttons and an interactive "Today" / date label with calendar icon for returning to Today when viewing past ledger records.

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
- **`LedgerRule.tsx`**: Visual 24-hour timeline bar (00 to 24 hours). Slices each usage interval, maps `appId` to `primary_category`, and colors using `categoryColors.ts`. Displays category legend at the bottom.
- **`UsageAside.tsx`**: Compact right-side card showing ranked app usage with visual progress bars.
- **`WeeklyChart.tsx`**: 7-day bar chart showing day-by-day totals with week-over-week deltas.
- **`CategoryMix.tsx`**: Circular conic-gradient donut chart showing time distribution by category.

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
- Cards: `.settings-card--language`, `.settings-card--appearance`, `.settings-card--advanced`, `.settings-card--security`, `.settings-card--tutorial`, `.settings-card--about`.
- **Security Card** (`.settings-card--security`):
  - **Strict Mode Toggle**: Prevents circumvention with inline `<LoadingSpinner size="sm" />`.
  - **Family DNS Protection Toggle**: Toggles Cloudflare Family DNS system-wide with live status indicator dot (Emerald green when protected, muted gray when disabled), copy explaining automatic preservation/restoration of original DNS, and an inline `<LoadingSpinner size="sm" />`.
  - **Admin PIN**: Displays current configuration state with Set/Change PIN modal triggers.
- **Loading Animations Across Settings**: Every toggle (`show_hud_overlay`, `strict_mode`, `family_dns`) and advanced parameter input (`limit_cooldown_hours`, `idle_threshold_secs`, `day_start_minutes`) displays a dedicated `<LoadingSpinner />` while persisting changes and temporarily locks input to prevent race conditions.
- Direct controls for idle threshold, day reset offset, strict mode, and anti-impulse cooldown.

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

## 5. Native Win32 GDI Overlays (`crates/session`)

The system renders two native Win32 GDI topmost overlays running in dedicated message loop threads inside `screentime-session`:

### A. Timer HUD Overlay (`hud.rs`)
- **Visual Design**: High-contrast 92x28 pill (14px corner radius) with smoked-glass translucency (alpha 235/255).
  - Background: Obsidian purple `#210F37` (`rgb(0x21, 0x0F, 0x37)`).
  - Border: Amber gold `#DCA06D` (`rgb(0xDC, 0xA0, 0x6D)`) when active timer, or warm terracotta `#A55B4B` (`rgb(0xA5, 0x5B, 0x4B)`) in normal budget tracking.
  - Indicator: 4px glowing indicator dot on the left.
  - Digits: Bold monospace ClearType `Consolas` tabular time readout in amber gold or ivory white.
- **60 Hz Real-Time Window Clamping**:
  - Attached via `SetTimer(hwnd, TRACK_TIMER_ID, 16, None)` (~60 FPS).
  - Follows `target_hwnd` position in real-time, clamping to the top-center of the application frame: `wr.top + 10`.
  - Auto-hides on app minimize (`IsIconic`) and restores smoothly when reopened.
  - Session loop no longer destroys/recreates the HUD every second; it preserves the window and only calls `run.update(...)`, respawning only when application focus changes.

### B. Limit Block Overlay (`overlay.rs`)
- **Visual Design**: Topmost, borderless window covering the target window with a centered floating obsidian card (alpha 245/255).
  - Full-window Backdrop: Deep obsidian purple veil `#130922` (`rgb(0x13, 0x09, 0x22)`).
  - Centered Card: Floating rich purple card `#281133` (`rgb(0x28, 0x11, 0x33)`) with subtle border `#56225C`.
  - Header Badge: "LIMIT REACHED" in burgundy well (`rgb(0x4A, 0x18, 0x22)`) with warm terracotta border and amber gold text.
  - App Label: Vibrant amber gold `#DCA06D`.
  - Actions:
    - Primary: "+15 MIN EXTEND" and "OK" in warm terracotta `#A55B4B` with white text.
    - Secondary: "QUIT APP" and "CLEAR" in elevated dark purple `#3B1842` with white text.
  - PIN Pad:
    - Glowing amber gold dots (`rgb(0xDC, 0xA0, 0x6D)`) with warm terracotta glow rings for entered digits; recessed dark purple wells for unlit slots.
    - 3x4 grid of rounded tactile buttons (1–9, C, 0, +15 MIN).
- **60 Hz Clamping & Dynamic Coverage**:
  - 16 ms Win32 tracking timer (`TRACK_TIMER_ID`) queries `GetWindowRect(target_hwnd)`.
  - When the user drags or resizes the blocked application, the overlay adjusts its bounds via `SetWindowPos` at 60 FPS, ensuring seamless clipping and complete mouse interception.
  - Minimization handling: Hides on `IsIconic(target_hwnd)` and restores on `IsWindowVisible(target_hwnd)`.

