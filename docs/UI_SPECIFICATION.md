# UI specification

How the screens are built today. The visual and voice contract is
`docs/DESIGN_SYSTEM.md`; this file maps it to code. Current state only:
history is in git. When code and this file disagree, fix one of them in the
same change.

## 1. Foundations

- **Themes**: `light`, `dark`, `system` (labelled "Like Windows"). Palettes in
  `ui/src/styles/redesign.css` (`[data-theme="dark"]`, `[data-theme="light"]`);
  shared scales (fonts, spacing, radii, shadows, motion) in `tokens.css`.
  `useTheme.ts` stores the choice in `localStorage` (`tether_theme`) and via the
  Tauri `set_theme` command (for the native HUD); `index.html` stamps the theme
  before React loads. Retired theme names migrate in three places that must
  agree: `useTheme.ts normalizePref`, the `index.html` script and
  `ui/src-tauri/src/lib.rs normalize_theme`.
- **Components** for the screens: `ui/src/styles/tether.css` (prefix `tt-`,
  loaded last): page head, buttons (`tt-btn` + `--primary/--outline/--ghost/--danger/--sm`,
  `tt-link`, `tt-icon-btn`), cards and rows (`tt-card`, `tt-row*`,
  `tt-list`, `tt-columns`, `tt-col-stack`), budget rows and meters, notes and
  banners, inputs (`tt-input`, `tt-select`, `tt-segmented` with
  `aria-pressed`, `tt-chips`, `tt-day-pill`), tags (`tt-tag`), setup, settings,
  Today, Activity, tray panel, block-screen reasons. Dialog styles (`.dialog`,
  `.btn`, `.field`, `.pin-input`) are in `app.css`; the budget editor and the
  switch (`.toggle-switch`) in `redesign.css`.
- **Budget hues**: `tt-hue-{games,social,video,total,other}` sets `--hue`,
  `--hue-tile`, `--hue-fill`, `--hue-ink` (`ui/src/budgetHue.ts`: Games →
  games; Social Media and Short-Form Video → social; Video & Streaming → video;
  total → total; everything else → other).
- **Fonts** are bundled (Rubik, Unbounded, JetBrains Mono via fontsource); the
  Tauri CSP blocks remote fonts.
- **Text**: every visible string, `aria-label`, `title` and placeholder goes
  through i18n (`ui/src/locales/{en,ar}/translation.json`, same keys in both).
  Arabic has all plural forms where counts appear; numbers inside Arabic
  sentences are wrapped in U+2068/U+2069. No letter-spacing on Arabic.
- **Numbers and times**: durations are amounts (`formatDuration`: "6h 11m");
  clock times and dates format through `Intl` in the UI language
  (`clockLabel`, `formatDayLabel`); a reset at midnight reads "midnight"
  (`backAtLabel`). "Today" follows the agent's `day_start_minutes`
  (`format.ts setDayStartMinutes`).
- **Copy**: sentence case, plain verbs, the same name for an action through a
  flow; dialogs put the main button last.
- **Accessibility**: one level-1 heading per page; the title bar is a
  labelled "Window" region; icon-only controls have labels; switches use
  `role="switch"`; meters are `role="progressbar"`. axe-core finds nothing on
  any screen (checked with the screenshot scenarios, light, dark, Arabic and
  narrow).

## 2. Shell

```
[Title bar: "Tether", minimize, maximize, hide]                 (TitleBar.tsx)
[mark] Tether [Live] | (Today) (Limits) (Activity) (Settings) | (< Today >)   (Sidebar.tsx)
[page]
```

- The window has no system frame; the title bar is drawn by the app on every
  screen, setup included. Close hides the window (Tether keeps running).
- Pill nav: the active item is a filled plum pill (`aria-current="page"`).
  Labels hide under 980px.
- The day stepper shows only on Today (hidden, keeping its space, elsewhere).
  Arrow keys also step days.
- Switching tabs or subviews scrolls to the top (`scrollTop.ts`).
- While the agent is unreachable: a `role="alert"` banner. While tracking is
  unavailable (no reports for a minute): a notice under the nav.

## 3. Today (`pages/TodayPage.tsx`, `todayModel.ts`)

Top to bottom: hero (ring, headline, day strip), blocked apps, budget tiles,
suggestion, schedule and website cards, most used apps, this week.

- **Ring**: time left of the total budget ("Done · all 6h used" when used
  up), else time used.
- **Headline** priority: total used up, a budget done, total low, a budget
  low, else "You're on track today". Past days show the date.
- **Day strip**: the agent's day (day start to +24 h), usage coloured by the
  budget each app counts toward, else by the app's own category hue
  (`todayModel.ts hueForApp`), schedules hatched, an orange "now" line;
  labels at their real positions. The legend names the hue ("Games",
  "Social media", "Video and streaming", "Everything else"; `hueLabel`).
- **Blocked apps** (`BlockedBanner`): each blocked app with "Allow 15 more
  minutes" (PIN).
- **Tiles**: one per enabled app or category budget; the tint fills to the
  share left; status pill (Plenty left, Running low ≤15 min or ≤20 %, Done
  for today, Extra time); "Back at midnight" when done; Edit.
- **Suggestion** ("From last week", `suggestBudget`): the most used app with
  no budget, averaging 45 min a day or more; Add a budget / Not now (hidden
  for 7 days, `localStorage`). Only on today, only when a budget exists.
- **Schedule card**: the schedule on now, else the next one. **Websites
  card**: sites blocked and Family DNS. Both open Limits.
- **Most used** (`UsageAside`) and **This week** (`WeeklyChart`: bars open a
  day; a sentence instead of bars when nothing is recorded).

## 4. Limits (`pages/LimitsPage.tsx`)

- Header with the cooldown in words and **+ New budget**.
- **Waiting** banner (neutral plum) per pending loosening, with Cancel.
- **Budgets**: swatch, name, rule, a meter of time left today ("Used up
  today"), switch, Edit.
- **Schedules** card (side): each schedule with time range, days and switch;
  Manage opens the Schedules subview.
- **Websites** card (side): add a site inline, up to three sites as chips,
  "+N more", the Family DNS switch; Manage opens the Websites subview.

**Schedules subview** (`components/DowntimeSection.tsx`): "Your schedules"
(On now tag, "10 PM – 7 AM (overnight) · Mon–Fri", switch, Edit, delete with
an inline "Delete?" confirm); schedule editor dialog (name, From/Until, an
overnight note, quick picks and day pills); "Always allowed" (a select of
detected apps, Allow, removable chips).

**Websites subview** (`components/WebFilteringPanel.tsx`): "Add sites"
(inline form, Import a list of up to 100, a collapsed AI-prompt helper);
"Blocked sites" (count of visible sites, search, rows with an unblock icon
(PIN), paging, and "N more sites are on the hidden adult list. Show them"
(PIN)).

**Budget editor** (`components/LimitEditorDialog.tsx`): App / Category /
Total switcher and picker (new budgets), the app's category with Change
category, daily amount (h/m fields, preset pills, slider ticks from 1h),
"Different limit on some days" (quick picks, day buttons, per-day adjuster),
"Budget is on" switch, then a note saying when the change applies
(`limitRules.ts waitsForCooldown` mirrors the agent: raising waits for the
cooldown; lowering, adding and switching off apply now), Cancel / Save.

## 5. Activity (`pages/ActivityPage.tsx`, `activityModel.ts`)

A week (stepper, never past today): summary sentence; "Screen time by day"
(bars split by hue as on the day strip, a dashed total-budget line, days over budget in
danger text, bars open the day); "Most used this week" under the chart; side
column with "Budgets this week" (average and days run out, total included)
and "When time ran out" (block-screen answers).

## 6. Settings (`pages/SettingsPage.tsx`)

Groups: **Protection** (who it's for, PIN, strict mode, wait before raising),
**Timer on screen** (show timer, in full-screen games, peek shortcut, chime
volume with Play), **Day and tracking** (day start, idle after), **Language
and look**, **Apps** (opens the app directory), **About and help** (version
and six help topics, collapsible). Changes go through `actions.setSetting`;
the agent decides which need the PIN and the UI asks for it then
(`useLedgerActions.guarded`).

App directory (`AppDirectoryDialog.tsx`) and categorize dialog
(`CategorizeDialog.tsx`) still use the older dialog layout.

## 7. Setup (`components/setup/SetupFlow.tsx`)

First run (`localStorage screentime_first_run_completed`), with the title bar
and a language switch: 1. who it's for (`profile`), 2. starting budgets
(templates: Games, Social, Video, Bedtime schedule, all screen time),
3. PIN and recovery code (skippable only for "Me"), 4. websites (Family DNS).

## 8. Block screen (`components/BlockOverlay.tsx`, `ui/src-tauri/src/overlay_bridge.rs`)

The `overlay` window, always dark, over the blocked app (at least 520×820 with
the PIN pad, DPI-scaled, clamped to the monitor).

- The reason is re-derived in the agent's order (`blockModel.ts`): schedule
  in force, the app's budget, total screen time.
- Badge, eyebrow ("TikTok · 20m every day, all used"), title ("Time's up for
  TikTok today" / "Homework time is on"), "TikTok is waiting behind this
  screen, so nothing is lost.", a strip to when it comes back.
- "What were you about to do?" chips (optional, `RecordBlockReason`).
- **Close {app}** (primary). Extra time only when it can work (never during a
  schedule or in strict mode): with a PIN, a link opens the keypad; without,
  "Allow 15 more minutes". Escape only closes the keypad.
- The overlay hides only after the app actually closed; otherwise it shows
  the error and stays.

## 9. Timer HUD (`crates/session/src/mpo.rs`, `hud.rs`, `hud_palette.rs`)

A 92×28 pill over apps with a budget: plum or white by theme (`theme.txt`,
`system` resolved from Windows), an orange dot during extra time, all orange
in the last minute. Draggable (position remembered), never takes focus,
slides in and out. Hidden over full-screen games unless enabled; the peek
shortcut and the chimes show it for 4 s.

## 10. Tray (`ui/src-tauri/src/tray.rs`, `tray_panel.rs`, `components/TrayPanel.tsx`)

- Left click: the tray panel (`tray` window, `?view=tray`), 360px wide and as
  tall as its content (`fit_tray_panel` keeps it against the taskbar):
  ring, time used and next schedule, a row per budget, Open Tether. Hides on
  Escape or focus loss; reloads when shown and every 30 s.
- Right click: the menu. "Reset network settings" and "Stop Tether and its
  service" go through the PIN prompt (`run_tray_action`).
- Only the host shows the tray and overlay windows; `main.tsx` shows its own
  window only for `main`.
