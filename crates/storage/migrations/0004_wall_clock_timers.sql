ALTER TABLE overrides ADD COLUMN expires_utc TEXT;
UPDATE overrides SET expires_utc = datetime(granted_utc, '+' || granted_secs || ' seconds');
