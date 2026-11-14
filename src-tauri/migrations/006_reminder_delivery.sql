ALTER TABLE reminders ADD COLUMN delivered_at TEXT;
ALTER TABLE reminders ADD COLUMN delivery_claimed_at TEXT;
ALTER TABLE reminders ADD COLUMN delivery_retry_at TEXT;
ALTER TABLE reminders ADD COLUMN delivery_attempts INTEGER NOT NULL DEFAULT 0 CHECK(delivery_attempts >= 0);
ALTER TABLE reminders ADD COLUMN last_delivery_error TEXT;

CREATE INDEX idx_reminders_delivery_due
  ON reminders(status, due_at, snoozed_until, delivered_at, delivery_retry_at);
