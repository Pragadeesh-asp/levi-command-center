CREATE TABLE task_file_releases (
  id TEXT PRIMARY KEY NOT NULL,
  task_id TEXT NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
  path_key TEXT NOT NULL,
  path_display TEXT NOT NULL,
  released_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX idx_task_file_releases_task ON task_file_releases(task_id, released_at DESC);
