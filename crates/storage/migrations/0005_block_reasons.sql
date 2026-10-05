-- What someone was about to do when an app was blocked (the block screen's
-- "What were you about to do?"). One answer per app per day: answering
-- again replaces it, so the counts on Activity cannot be padded.
CREATE TABLE block_reasons (
  day_key      INTEGER NOT NULL,
  app_id       INTEGER NOT NULL REFERENCES apps(id) ON DELETE CASCADE,
  reason       TEXT    NOT NULL CHECK (reason IN ('finish', 'bored', 'habit')),
  recorded_utc TEXT    NOT NULL,
  PRIMARY KEY (day_key, app_id)
);
