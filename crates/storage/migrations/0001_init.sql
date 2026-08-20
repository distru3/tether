-- Initial schema.
--
-- Conventions:
--   * Timestamps are RFC 3339 strings in UTC. Never local time.
--   * `day_key` is a local calendar day as YYYYMMDD (see st-core::DayKey).
--   * Durations are whole seconds.

-- ---------------------------------------------------------------------------
-- Taxonomy
-- ---------------------------------------------------------------------------

CREATE TABLE categories (
  id       INTEGER PRIMARY KEY,
  slug     TEXT    NOT NULL UNIQUE,
  name     TEXT    NOT NULL,
  -- 'limitable' | 'block_only' | 'never_block'
  kind     TEXT    NOT NULL CHECK (kind IN ('limitable', 'block_only', 'never_block')),
  color    TEXT    NOT NULL,
  builtin  INTEGER NOT NULL DEFAULT 0 CHECK (builtin IN (0, 1))
);

-- ---------------------------------------------------------------------------
-- Subjects: apps and sites
-- ---------------------------------------------------------------------------

CREATE TABLE apps (
  id                  INTEGER PRIMARY KEY,
  -- Canonical "kind:value" form of st-core::AppKey.
  app_key             TEXT    NOT NULL UNIQUE,
  display_name        TEXT    NOT NULL,
  publisher           TEXT,
  primary_category_id INTEGER NOT NULL REFERENCES categories(id),
  -- Once set, the classifier must never overwrite the category. A human
  -- decision outranks any signature database.
  user_classified     INTEGER NOT NULL DEFAULT 0 CHECK (user_classified IN (0, 1)),
  icon_png            BLOB,
  first_seen_utc      TEXT    NOT NULL,
  last_seen_utc       TEXT    NOT NULL
);

-- Additional categories used only for limit matching, never for reporting.
-- This is what lets TikTok be both Social Media and Short-Form Video while
-- category totals still sum to 100%.
CREATE TABLE app_tags (
  app_id      INTEGER NOT NULL REFERENCES apps(id) ON DELETE CASCADE,
  category_id INTEGER NOT NULL REFERENCES categories(id) ON DELETE CASCADE,
  PRIMARY KEY (app_id, category_id)
) WITHOUT ROWID;

CREATE TABLE sites (
  id                  INTEGER PRIMARY KEY,
  domain              TEXT    NOT NULL UNIQUE,
  primary_category_id INTEGER NOT NULL REFERENCES categories(id),
  user_classified     INTEGER NOT NULL DEFAULT 0 CHECK (user_classified IN (0, 1))
);

CREATE TABLE site_tags (
  site_id     INTEGER NOT NULL REFERENCES sites(id) ON DELETE CASCADE,
  category_id INTEGER NOT NULL REFERENCES categories(id) ON DELETE CASCADE,
  PRIMARY KEY (site_id, category_id)
) WITHOUT ROWID;

-- Primary category plus tags, de-duplicated. Category budgets are computed
-- through this view.
CREATE VIEW app_categories AS
  SELECT id AS app_id, primary_category_id AS category_id FROM apps
  UNION
  SELECT app_id, category_id FROM app_tags;

-- ---------------------------------------------------------------------------
-- Usage
-- ---------------------------------------------------------------------------

-- One row per contiguous foreground span. The sampler polls at ~1 Hz but
-- collapses identical consecutive samples, so a two-hour session is one row.
CREATE TABLE usage_intervals (
  id            INTEGER PRIMARY KEY,
  subject_type  TEXT    NOT NULL CHECK (subject_type IN ('app', 'site')),
  subject_id    INTEGER NOT NULL,
  -- Distinguishes concurrently logged-in users.
  session_id    TEXT    NOT NULL,
  start_utc     TEXT    NOT NULL,
  end_utc       TEXT    NOT NULL,
  duration_secs INTEGER NOT NULL CHECK (duration_secs >= 0),
  day_key       INTEGER NOT NULL
);

CREATE INDEX idx_intervals_day ON usage_intervals (day_key);
CREATE INDEX idx_intervals_subject_day
  ON usage_intervals (subject_type, subject_id, day_key);

-- Materialised rollup. Every dashboard query and every limit check reads this
-- rather than aggregating raw intervals.
CREATE TABLE usage_daily (
  day_key      INTEGER NOT NULL,
  subject_type TEXT    NOT NULL CHECK (subject_type IN ('app', 'site')),
  subject_id   INTEGER NOT NULL,
  seconds      INTEGER NOT NULL DEFAULT 0,
  PRIMARY KEY (day_key, subject_type, subject_id)
) WITHOUT ROWID;

-- ---------------------------------------------------------------------------
-- Limits and schedules
-- ---------------------------------------------------------------------------

CREATE TABLE limits (
  id                 INTEGER PRIMARY KEY,
  target_type        TEXT    NOT NULL CHECK (target_type IN ('app', 'category', 'total')),
  -- NULL when target_type = 'total'.
  target_id          INTEGER,
  default_minutes    INTEGER NOT NULL CHECK (default_minutes >= 0),
  -- JSON array of 7 nullable integers, Monday first.
  weekday_minutes    TEXT    NOT NULL DEFAULT '[null,null,null,null,null,null,null]',
  enabled            INTEGER NOT NULL DEFAULT 1 CHECK (enabled IN (0, 1)),
  created_utc        TEXT    NOT NULL,
  -- Anti-impulse cooldown: loosening or removing a limit only takes effect at
  -- this time (typically +24h). Tightening applies immediately.
  effective_from_utc TEXT,
  UNIQUE (target_type, target_id)
);

-- Downtime windows: outside these, only the allowlist runs.
CREATE TABLE schedules (
  id           INTEGER PRIMARY KEY,
  name         TEXT    NOT NULL,
  -- Bitmask, bit 0 = Monday.
  weekday_mask INTEGER NOT NULL,
  start_minute INTEGER NOT NULL CHECK (start_minute BETWEEN 0 AND 1439),
  end_minute   INTEGER NOT NULL CHECK (end_minute BETWEEN 0 AND 1439),
  enabled      INTEGER NOT NULL DEFAULT 1 CHECK (enabled IN (0, 1))
);

-- "Always allowed", even during downtime.
CREATE TABLE allowlist (
  subject_type TEXT    NOT NULL CHECK (subject_type IN ('app', 'site')),
  subject_id   INTEGER NOT NULL,
  PRIMARY KEY (subject_type, subject_id)
) WITHOUT ROWID;

-- ---------------------------------------------------------------------------
-- Enforcement state
-- ---------------------------------------------------------------------------

CREATE TABLE block_state (
  subject_type   TEXT    NOT NULL CHECK (subject_type IN ('app', 'site')),
  subject_id     INTEGER NOT NULL,
  -- 'limit' | 'downtime' | 'category_filter' | 'manual'
  reason         TEXT    NOT NULL,
  blocked_since_utc TEXT NOT NULL,
  -- Day boundary at which the block lifts. NULL means indefinite (content
  -- filters such as adult content, which never expire).
  expires_utc    TEXT,
  PRIMARY KEY (subject_type, subject_id)
) WITHOUT ROWID;

-- PIN-approved extensions. Additive, and scoped to a single day_key so they
-- expire with everything else at the boundary.
CREATE TABLE overrides (
  id           INTEGER PRIMARY KEY,
  target_type  TEXT    NOT NULL CHECK (target_type IN ('app', 'category', 'total')),
  target_id    INTEGER,
  day_key      INTEGER NOT NULL,
  granted_secs INTEGER NOT NULL CHECK (granted_secs > 0),
  granted_utc  TEXT    NOT NULL,
  reason       TEXT
);

CREATE INDEX idx_overrides_day ON overrides (day_key);

-- ---------------------------------------------------------------------------
-- Web filtering
-- ---------------------------------------------------------------------------

CREATE TABLE blocklists (
  id               INTEGER PRIMARY KEY,
  name             TEXT    NOT NULL UNIQUE,
  source_url       TEXT,
  version          TEXT,
  -- SHA-256 of the fetched payload; refuse to load on mismatch.
  checksum         TEXT,
  enabled          INTEGER NOT NULL DEFAULT 1 CHECK (enabled IN (0, 1)),
  last_updated_utc TEXT
);

CREATE TABLE block_rules (
  id                 INTEGER PRIMARY KEY,
  blocklist_id       INTEGER REFERENCES blocklists(id) ON DELETE CASCADE,
  category_id        INTEGER REFERENCES categories(id) ON DELETE SET NULL,
  domain             TEXT    NOT NULL,
  include_subdomains INTEGER NOT NULL DEFAULT 1 CHECK (include_subdomains IN (0, 1)),
  -- 'block' | 'allow'; explicit allow wins, so a user can unblock one site
  -- inside an otherwise blocked category.
  action             TEXT    NOT NULL DEFAULT 'block' CHECK (action IN ('block', 'allow'))
);

CREATE INDEX idx_block_rules_domain ON block_rules (domain);

-- ---------------------------------------------------------------------------
-- Audit and settings
-- ---------------------------------------------------------------------------

-- Append-only. Records overrides, limit changes, and every detected tamper
-- attempt (service stopped, hosts file edited, DNS changed, clock moved).
-- Without this the app cannot tell the user what happened, and a parent cannot
-- see why a limit stopped working.
CREATE TABLE audit_log (
  id     INTEGER PRIMARY KEY,
  ts_utc TEXT NOT NULL,
  kind   TEXT NOT NULL,
  detail TEXT
);

CREATE INDEX idx_audit_ts ON audit_log (ts_utc);

CREATE TABLE settings (
  key   TEXT PRIMARY KEY,
  value TEXT NOT NULL
) WITHOUT ROWID;

INSERT INTO settings (key, value) VALUES
  -- Minutes after local midnight at which budgets reset.
  ('day_start_minutes', '0'),
  -- Seconds without input before usage stops accruing.
  ('idle_threshold_secs', '60'),
  -- Seconds of warning before a process is frozen, so work can be saved.
  ('grace_countdown_secs', '60'),
  -- Window titles are privacy-sensitive; off by default.
  ('capture_window_titles', 'false'),
  -- Strict mode disables the "+15 minutes" escape hatch entirely.
  ('strict_mode', 'false'),
  -- Hours before a loosened limit takes effect.
  ('limit_cooldown_hours', '24'),
  ('telemetry_enabled', 'false'),
  ('schema_seeded', 'false');
