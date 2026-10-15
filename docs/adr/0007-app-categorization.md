# ADR 0007: Better app categorization

Date: 2026-10-15
Status: Proposed (plan; nothing here is built yet)

## Context

Category decides an app's colour, which budgets it counts toward, and
whether it can be blocked at all. Today it is often wrong or missing.

**How an app is identified**
- An app is its executable path. Every Store app (Netflix, TikTok,
  WhatsApp, Spotify from the Store) shows as one `ApplicationFrameHost.exe`
  "app" (`tracker-win/src/win.rs`, "Known gaps").
- Generic hosts are one app each, whatever they run: `javaw.exe` is
  always "Minecraft", and every Python program is one `python.exe` app.
- Helper processes (`steamwebhelper.exe`, launchers, crash reporters) are
  separate apps with their own time.

**What the classifier knows** (`agent/src/classify.rs`)
- About 200 hand-written executable names, then substring guesses on the
  publisher: any publisher containing "studios", "interactive" or
  "entertainment" counts as Games, whatever it makes.
- Then keyword guesses on the display name.
- Browsers are deliberately left uncategorized, so most of a typical day is
  grey "Everything else".

**What it has to go on**
- An app first seen in a usage report is named after its file ("Code",
  "Chrome"). The `FileDescription` and company in the executable's version
  resource are never read.
- Publishers exist only for apps found in the uninstall registry.

**When it runs**
- Only when an app is first seen. Improving the table never fixes apps
  already in the database, and nothing records *why* an app got its
  category.

**What the person can do**
- Correcting a category means the old-style categorize dialog or the app
  directory: Title Case, palette pills, 18 categories to choose from, no
  bulk changes.
- Nothing asks about the apps that matter: the uncategorized ones with
  the most time.

## Decision (proposed)

Four parts, in this order. Each is shippable alone.

### 1. Know more about each app

- **Version info**: the session helper (same user, no privilege needed)
  reads `FileDescription`, `ProductName` and `CompanyName` from an
  executable the first time it reports it, and sends them once with the
  existing `RegisterDiscoveredApps`. The display name becomes "Visual
  Studio Code", not "Code", and every app gets a publisher.
- **Store apps**: for a foreground `ApplicationFrameHost` window, find the
  child window owned by another process and read its Application User
  Model ID (`GetApplicationUserModelId`). New key kind
  `AppKey::Packaged(<package family name>)`, with the display name from
  the package manifest. Netflix, TikTok, WhatsApp, Spotify and the rest
  stop being one app.
- **Generic hosts**: for `javaw.exe` and `python*.exe`, read the command
  line (same user) and key on the main jar or script: Minecraft becomes
  `javaw:…\.minecraft\…`, and other Java apps get their own key. Electron
  apps already have their own executable.
- **Helpers fold into their app**: a small parent table
  (`steamwebhelper.exe` → `steam.exe`, `*crashhandler*` and `*updater*` →
  ignored) applied before recording.

### 2. A real signature database, re-applied when it improves

- **Data file**: move the table out of code into `crates/agent/data/apps.toml`,
  embedded at build time. Each entry can match on:
  - executable name;
  - Store package family;
  - path pattern;
  - exact publisher (no substrings).

  It gives a category, optional tags, and a display name. The aim is
  around 800 entries covering what children and adults actually run:
  launchers, the top Steam titles, Store apps, chat apps, editors, office
  suites, school apps, streaming apps.
- **Rules in order of trust**:
  1. Exact executable or package match.
  2. Known launcher library path (Steam, Epic, GOG, Xbox, Riot), as today.
  3. Exact publisher list.
  4. Version-info keywords, only as a last resort.

  The loose substring rules go.
- **Re-applied**: migration 0007 adds `apps.classified_by` (`user`,
  `signature`, `path`, `publisher`, `keyword`, `none`) and
  `apps.classifier_version`. When a new build raises the table version,
  every app not set by a person is classified again.
- **Accuracy test**: a fixture of real app identities (name, path,
  publisher, package) with the expected category, run in CI. It reports
  hits and misses so the table only gets better.

### 3. Browsers get a home

- **New category**: Web browsing (`browsers`, limitable). Chrome, Edge,
  Firefox, Brave, Opera, Vivaldi and Arc go there.
- **Its own hue**, instead of the grey of "Everything else". Browser time
  is then visible and can have a budget.
- **Prepares browser time** (rework item 5): the extension later splits
  this category by site.

### 4. Sorting apps, for people

These replace the two old dialogs with one v2 **Apps** subview (from
Settings → Apps, and from a category chip on Today), merged with rework
item 3 (budgets as app groups):

- **"Needs a home" first**: uncategorized apps by time used this week,
  one tap per app on the eight user-facing groups (Games, Social, Video,
  Music, Chat, Work and school, Web browsing, Other). The 18 fine
  categories stay for matching and show under "More".
- **Why**: every app shows how it was sorted ("In your Steam library",
  "Published by Riot Games", "You chose this"). An app sorted by a guess
  is marked so the person can confirm it.
- **Learning from a correction**: moving one app offers the same move for
  the others from that publisher or folder ("Move the other 4 Ubisoft
  apps too?").
- **Prompt at the right time**: when an uncategorized app passes 30
  minutes in a day, Today shows one card: "What is Foo?" with the group
  chips. It is dismissed for that app once answered or skipped.
- **Moving an app into a looser group** (for example out of Games) still
  needs the PIN, as recategorizing does today.

## Consequences

- Wire and storage changes:
  - `AppKey` gains a packaged variant;
  - `DiscoveredAppDto` carries the version-info fields;
  - migration 0007 adds the source and version columns.

  Docs, the IPC catalog and the generated TypeScript update in the same
  changes.
- Store-app keys appear as new apps. Their earlier time stays under
  `ApplicationFrameHost`.
- The signature table becomes data that can be reviewed and grown without
  touching code. There are no online lookups: everything ships with the
  app, in keeping with "nothing leaves the PC".
- Open questions for the owner:
  - Should Web browsing be limitable on its own before the extension exists?
  - Should the fine categories be hidden by default?
