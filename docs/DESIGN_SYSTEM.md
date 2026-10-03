# Tether design system (v2)

Approved 2026-10-03. The source mockups are the "Tether · full set" page of the
design canvas (Today, block screen, Limits, budget editor, PIN prompt, Activity,
Settings, Arabic Today, setup, timer and tray). This file is the written
contract; when code and this file disagree, fix one of them in the same change.

The direction combines three explored options: soft budget tiles that fill with
the time left, a 24-hour strip of the day, and the plum and orange of the app
icon.

## 1. Who and when

Tether serves two people, chosen on first run (`profile` setting):

- **Me** (`self`): someone limiting their own use. PIN optional. The 24-hour
  wait on loosening is the main protection. Copy is first person ("your
  budgets").
- **Someone I look after** (`guardian`): a parent setting limits on a child's
  standard Windows account. PIN required. Copy addresses the child on the
  surfaces the child sees ("A grown-up can add time with the PIN").

Surfaces in order of how often they are seen: timer (HUD) and tray, block
screen, Today, Limits, Activity, Settings. Design the small surfaces first.

## 2. Principles

1. Lead with time **left**, not time spent.
2. One place for rules: budgets, schedules and websites live on **Limits**.
3. Say when a change applies **before** saving: lowering applies now, raising
   waits for the cooldown (`limit_cooldown_hours`, 24 h by default).
4. Start from real use: suggestions come from last week's usage; first run
   offers templates because there is no history yet.
5. Plain words. No "orders", "ledger", "telemetry". A control says what it does.

## 3. Colour

Two themes plus "Like Windows" (follows the OS light/dark setting). Theme names
stored: `light`, `dark`, `system`. Older names migrate (`midnight-cobalt`,
`slate-charcoal` → `dark`; `clean-titanium`, `nordic-frost` → `light`).

| Token (role) | Light | Dark |
|---|---|---|
| Page background | `#F4F1F8` | `#1C1229` |
| Card / surface | `#FFFFFF` | `#26193B` |
| Raised surface, inputs | `#F9F7FC` | `#2E1D45` |
| Border | `#E7E0F0` | `#3A2752` |
| Strong border | `#DDD3EA` | `#4A3266` |
| Text | `#23163A` | `#F4EEFB` |
| Secondary text | `#4E4363` | `#D9CCF2` |
| Muted text (≥4.5:1) | `#6B5F80` | `#B3A3CF` |
| Brand (plum) fill | `#23163A` | `#F4EEFB` (inverted for selected pills) |
| Accent (orange) fill | `#F08A3C` | `#F08A3C` |
| Accent as text | `#A84F0E` | `#FFB27A` |
| Lilac | `#C9B6F2` | `#C9B6F2` |
| Danger text | `#B42318` | `#FFA3B0` |
| Success text | `#176A52` | `#7FD6BE` |

Budget hues (tile background / fill level / swatch):

| Budget | Light tile | Light fill | Swatch | Dark tile | Dark fill |
|---|---|---|---|---|---|
| Games | `#EEE7FF` | `#D6C9FA` | `#8F6CE6` | `#2D2150` | `#46337A` |
| Social | `#FFE6E0` | — | `#E8705C` | `#3A1F2E` | — |
| Video | `#DDF4EC` | `#B4E6D7` | `#2FA383` | `#18332E` | `#23504A` |
| Everything else | — | — | `#B9AFC8` | — | `#6E6185` |

Orange marks "now" and "attention" (running low, the primary action on the
block screen). Do not use it for decoration.

## 4. Type

- **Rubik** (body, UI, Arabic). Covers Latin and Arabic.
- **Unbounded** (display): screen titles and big numbers ("1h 05", "20 min").
  It has no Arabic glyphs, so Arabic headings fall back to Rubik.
- Numbers use tabular figures.
- Scale: 13 (captions), 15 (body), 17 (section titles), 20–21 (card titles),
  28 (page titles), 34–44 (hero numbers).

Both fonts are bundled (fontsource); the Tauri CSP blocks remote fonts.

## 5. Shape and space

- Radii: pills `99px`; cards `22–28px`; tiles `26px`; small squircles `11–17px`.
- Page gutter 40px, card padding 18–24px, gaps 16–22px.
- Cards are flat on light (1px shadow), bordered on dark. No glass or blur.

## 6. Components

- **Ring**: time left of the total budget, orange arc on a tinted track, value
  in Unbounded inside.
- **Day strip**: 4 am–4 am (follows `day_start_minutes`), usage blocks coloured
  by budget, grey for apps outside any budget, hatched bedtime, orange "now"
  line. Every mark sits at its real time; labels are positioned, not spaced.
- **Budget tile**: tinted card whose lower part fills to the fraction of time
  left. Status pill: "Running low" (orange), "Done for today" (plum), "Plenty
  left" (white). Edit button bottom corner.
- **Budget row** (Limits): swatch squircle, name, schedule and apps, today's
  usage bar, Edit.
- **Pill nav**: Today · Limits · (Activity, later) · Settings; selected item is
  a filled plum pill.
- **Switch**: plum track with an orange knob when on; knob moves toward the
  inline end (RTL aware).
- **"Applies when" note**: orange-tinted box with a clock icon, shown in every
  editor that can loosen something, before the save button.
- **Waiting banner**: on Limits, one row per pending loosening, with Cancel.

## 7. Screens

| Screen | Job | Status |
|---|---|---|
| Setup (4 steps) | Who it's for, starting budgets, PIN, websites | Phase 1 |
| Limits | Every rule in one place | Phase 1 |
| Budget editor | Change a limit, see when it applies | Phase 1 |
| Settings | Four short groups | Phase 1 |
| Today | Time left, day strip, budget tiles | Done (Phase 2) |
| Block screen | Pause and choose | Phase 2 (borrowing/reasons need the agent, Phase 4) |
| Timer and tray | Glance | Phase 2 |
| Activity | The week against the limits | Later |
| Installer | Say what gets installed, then hand over to setup | Done: NSIS, plum sidebar, Rubik/Unbounded embedded (`packaging/README.md`) |

## 8. Voice

- Sentence case. Short. Name what people recognise ("Websites", not "DNS
  filter rules").
- Times as people say them ("Back at 4 am", "in 3h 18m").
- The block screen never scolds: "Games are done for today", "nothing is
  lost".
- Arabic is first-class: every string in `locales/ar`, layouts mirror, numbers
  stay Latin digits as elsewhere in the app.
