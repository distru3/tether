-- Windows accounts (docs/adr/0005-per-account-usage.md).
--
-- Usage, blocks, extra time and block-screen answers belong to the account
-- they happened on. Rows written before this migration get account_id 0
-- ("nobody yet"); the first account the agent meets adopts them
-- (`Db::ensure_account`).

CREATE TABLE accounts (
  id             INTEGER PRIMARY KEY,
  -- Windows security identifier, e.g. S-1-5-21-...-1001.
  sid            TEXT    NOT NULL UNIQUE,
  name           TEXT    NOT NULL,
  -- Limits apply only to watched accounts. New accounts are watched.
  supervised     INTEGER NOT NULL DEFAULT 1 CHECK (supervised IN (0, 1)),
  first_seen_utc TEXT    NOT NULL,
  last_seen_utc  TEXT    NOT NULL
);

ALTER TABLE usage_intervals ADD COLUMN account_id INTEGER NOT NULL DEFAULT 0;
CREATE INDEX idx_intervals_account_day ON usage_intervals (account_id, day_key);

ALTER TABLE overrides ADD COLUMN account_id INTEGER NOT NULL DEFAULT 0;

-- Tables whose primary key gains the account are rebuilt.

CREATE TABLE usage_daily_new (
  account_id   INTEGER NOT NULL DEFAULT 0,
  day_key      INTEGER NOT NULL,
  subject_type TEXT    NOT NULL CHECK (subject_type IN ('app', 'site')),
  subject_id   INTEGER NOT NULL,
  seconds      INTEGER NOT NULL DEFAULT 0,
  PRIMARY KEY (account_id, day_key, subject_type, subject_id)
) WITHOUT ROWID;
INSERT INTO usage_daily_new (account_id, day_key, subject_type, subject_id, seconds)
  SELECT 0, day_key, subject_type, subject_id, seconds FROM usage_daily;
DROP TABLE usage_daily;
ALTER TABLE usage_daily_new RENAME TO usage_daily;

CREATE TABLE block_state_new (
  account_id        INTEGER NOT NULL DEFAULT 0,
  subject_type      TEXT    NOT NULL CHECK (subject_type IN ('app', 'site')),
  subject_id        INTEGER NOT NULL,
  reason            TEXT    NOT NULL,
  blocked_since_utc TEXT    NOT NULL,
  expires_utc       TEXT,
  PRIMARY KEY (account_id, subject_type, subject_id)
) WITHOUT ROWID;
INSERT INTO block_state_new (account_id, subject_type, subject_id, reason, blocked_since_utc, expires_utc)
  SELECT 0, subject_type, subject_id, reason, blocked_since_utc, expires_utc FROM block_state;
DROP TABLE block_state;
ALTER TABLE block_state_new RENAME TO block_state;

CREATE TABLE block_reasons_new (
  account_id   INTEGER NOT NULL DEFAULT 0,
  day_key      INTEGER NOT NULL,
  app_id       INTEGER NOT NULL REFERENCES apps(id) ON DELETE CASCADE,
  reason       TEXT    NOT NULL CHECK (reason IN ('finish', 'bored', 'habit')),
  recorded_utc TEXT    NOT NULL,
  PRIMARY KEY (account_id, day_key, app_id)
);
INSERT INTO block_reasons_new (account_id, day_key, app_id, reason, recorded_utc)
  SELECT 0, day_key, app_id, reason, recorded_utc FROM block_reasons;
DROP TABLE block_reasons;
ALTER TABLE block_reasons_new RENAME TO block_reasons;
