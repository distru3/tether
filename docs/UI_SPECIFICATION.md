# Screentime UI Architecture & Redesign Specification

This document defines the frontend layout, design system tokens, and component architecture for the Screentime dashboard. Verified against the codebase on **2026-09-12**.

---

## 1. Design System: Smoked-Glass Analytics Workspace

The UI uses a dark, smoked-glass telemetry aesthetic defined primarily in `ui/src/styles/redesign.css` with fallback tokens in `tokens.css`.

### Core Palette
- **Backgrounds**: Deep indigo canvas (`--redesign-bg: #091540`, `--redesign-bg-soft: #101e50`).
- **Glass Surfaces**: Translucent white panels (`--redesign-panel: rgba(243, 244, 244, 0.075)`, `--redesign-panel-strong: rgba(243, 244, 244, 0.12)`).
- **Borders**: Translucent white 1px rules (`--redesign-line: rgba(243, 244, 244, 0.18)`).
- **Typography**: System sans-serif for headings/copy (`--redesign-ink: #f3f4f4`, `--redesign-ink-soft: #d6def1`, `--redesign-muted: #9daaca`), monospace for time/data metrics.
- **Accents**:
  - Warm Action Blue/Indigo: `#1b2cc1` / `#7692ff`
  - Neon Green (Success / Live): `#e4ff30` / `#72c879`
  - Amber / Warning: `#f59e0b`
  - Rose / Blocked Danger: `#ff6b8a`
- **Elevation & Blur**: `backdrop-filter: blur(22px); box-shadow: 0 22px 52px rgba(0, 0, 0, 0.28); border-radius: 16px;`.

---

## 2. Layout Structure & Top Navigation

Unlike the legacy 240px vertical sidebar, the redesigned layout is a **top navigation bar** spanning the full width of the window:

```
+-----------------------------------------------------------------------------------------+
| [TitleBar] Screentime (36px, custom drag region, minimize, maximize, close)             |
+-----------------------------------------------------------------------------------------+
| [Top Nav Bar: .app-sidebar] (74px sticky)                                               |
|  [Logo + Live Status]  |  [Dashboard] [Web Filter] [App Limits] [Settings]  | [Stepper] |
+-----------------------------------------------------------------------------------------+
| [Main Content Area: .app-main-content]                                                  |
|                                                                                         |
|  (Active Tab View: overview | limits | web-filtering | settings)                        |
|                                                                                         |
+-----------------------------------------------------------------------------------------+
```

### Nav Items (`Sidebar.tsx`)
1. **Dashboard** (`overview`): Daily timeline, top applications, weekly chart, category distribution.
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

### Web Filtering (`WebFilteringPanel.tsx`)
- Shell class: `.web-filter-page-shell`.
- Manual domain entry input.
- Bulk import via text file or AI prompt generator with 100-domain safety guard.
- NSFW adult content filter toggle with PIN protection.
- Active domain rules table with delete actions.

### Settings (`App.tsx` Settings Tab)
- Shell class: `.settings-page`.
- Cards: `.settings-card--language`, `.settings-card--appearance`, `.settings-card--advanced`, `.settings-card--security`, `.settings-card--tutorial`, `.settings-card--about`.
- Direct controls for idle threshold, day reset offset, strict mode, and anti-impulse cooldown.
