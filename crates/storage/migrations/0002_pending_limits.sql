-- Anti-impulse cooldown for loosening limits.
--
-- A loosened limit keeps enforcing its *previous* values until
-- `effective_from_utc`; the new (looser) values wait here and are promoted
-- into `limits` when the cooldown elapses. Tightening never touches this
-- table. A `delete` row removes the limit when it becomes effective.

CREATE TABLE pending_limits (
  id                 INTEGER PRIMARY KEY,
  target_type        TEXT    NOT NULL CHECK (target_type IN ('app', 'category', 'total')),
  target_id          INTEGER,
  -- 'update' | 'delete'
  action             TEXT    NOT NULL CHECK (action IN ('update', 'delete')),
  default_minutes    INTEGER,
  weekday_minutes    TEXT,
  enabled            INTEGER,
  effective_from_utc TEXT    NOT NULL,
  UNIQUE (target_type, target_id)
);