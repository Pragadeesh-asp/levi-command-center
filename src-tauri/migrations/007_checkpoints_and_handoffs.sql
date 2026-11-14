ALTER TABLE task_checkpoints ADD COLUMN project_id TEXT REFERENCES projects(id) ON DELETE SET NULL;
ALTER TABLE task_checkpoints ADD COLUMN browser_profile_id TEXT REFERENCES browser_profiles(id) ON DELETE SET NULL;
ALTER TABLE task_checkpoints ADD COLUMN project_name_snapshot TEXT NOT NULL DEFAULT '';
ALTER TABLE task_checkpoints ADD COLUMN task_title_snapshot TEXT NOT NULL DEFAULT '';
ALTER TABLE task_checkpoints ADD COLUMN account_name_snapshot TEXT NOT NULL DEFAULT '';
ALTER TABLE task_checkpoints ADD COLUMN browser_profile_name_snapshot TEXT NOT NULL DEFAULT '';

CREATE TABLE task_handoffs (
  id TEXT PRIMARY KEY NOT NULL,
  task_id TEXT NOT NULL REFERENCES tasks(id) ON DELETE RESTRICT,
  account_id TEXT REFERENCES accounts(id) ON DELETE SET NULL,
  checkpoint_id TEXT REFERENCES task_checkpoints(id) ON DELETE SET NULL,
  kind TEXT NOT NULL CHECK(kind IN ('chatgpt_review','claude_revision','copilot_implementation','verification')),
  status TEXT NOT NULL CHECK(status IN ('prepared','feedback_recorded','revision_required','approved','started','submitted','passed','failed')),
  package_text TEXT NOT NULL,
  details_json TEXT NOT NULL DEFAULT '{}',
  account_name_snapshot TEXT NOT NULL DEFAULT '',
  browser_profile_name_snapshot TEXT NOT NULL DEFAULT '',
  created_at TEXT NOT NULL DEFAULT (datetime('now')),
  updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX idx_task_handoffs_task_date ON task_handoffs(task_id, created_at DESC);
CREATE INDEX idx_task_handoffs_kind_status ON task_handoffs(kind, status, created_at DESC);

CREATE TABLE task_handoff_events (
  id TEXT PRIMARY KEY NOT NULL,
  handoff_id TEXT NOT NULL REFERENCES task_handoffs(id) ON DELETE CASCADE,
  task_id TEXT NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
  account_id TEXT REFERENCES accounts(id) ON DELETE SET NULL,
  event_type TEXT NOT NULL,
  details_json TEXT NOT NULL DEFAULT '{}',
  created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX idx_task_handoff_events_task_date ON task_handoff_events(task_id, created_at DESC);
