CREATE TABLE task_checkpoints (
  id TEXT PRIMARY KEY NOT NULL,
  task_id TEXT REFERENCES tasks(id) ON DELETE SET NULL,
  account_id TEXT REFERENCES accounts(id) ON DELETE SET NULL,
  summary TEXT NOT NULL,
  remaining_work TEXT NOT NULL DEFAULT '',
  files_changed TEXT NOT NULL DEFAULT '',
  last_successful_step TEXT NOT NULL DEFAULT '',
  next_action TEXT NOT NULL DEFAULT '',
  blockers TEXT NOT NULL DEFAULT '',
  created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX idx_task_checkpoints_task_date ON task_checkpoints(task_id, created_at DESC);

CREATE TABLE usage_limit_events (
  id TEXT PRIMARY KEY NOT NULL,
  account_id TEXT NOT NULL REFERENCES accounts(id) ON DELETE RESTRICT,
  task_id TEXT REFERENCES tasks(id) ON DELETE SET NULL,
  checkpoint_id TEXT REFERENCES task_checkpoints(id) ON DELETE SET NULL,
  exact_message TEXT NOT NULL,
  detected_at TEXT NOT NULL DEFAULT (datetime('now')),
  reset_at TEXT,
  reset_status TEXT NOT NULL CHECK(reset_status IN ('confirmed','unknown')),
  status TEXT NOT NULL DEFAULT 'waiting' CHECK(status IN ('waiting','resolved','cancelled')),
  created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX idx_usage_limit_account_status ON usage_limit_events(account_id, status, detected_at DESC);
