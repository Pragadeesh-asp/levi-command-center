CREATE TABLE task_dependencies (
  task_id TEXT NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
  depends_on_task_id TEXT NOT NULL REFERENCES tasks(id) ON DELETE RESTRICT,
  created_at TEXT NOT NULL DEFAULT (datetime('now')),
  PRIMARY KEY(task_id, depends_on_task_id),
  CHECK(task_id <> depends_on_task_id)
);

CREATE INDEX idx_task_dependencies_prerequisite ON task_dependencies(depends_on_task_id);

CREATE TABLE task_file_claims (
  id TEXT PRIMARY KEY NOT NULL,
  project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
  task_id TEXT NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
  path_key TEXT NOT NULL,
  path_display TEXT NOT NULL,
  claim_kind TEXT NOT NULL CHECK(claim_kind IN ('intended','modified')),
  status TEXT NOT NULL DEFAULT 'active' CHECK(status IN ('active','released')),
  claimed_at TEXT NOT NULL DEFAULT (datetime('now')),
  released_at TEXT
);

CREATE INDEX idx_task_file_claims_active_project ON task_file_claims(project_id, status, path_key);
CREATE INDEX idx_task_file_claims_task ON task_file_claims(task_id, status);

CREATE TABLE task_file_declarations (
  task_id TEXT NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
  path_key TEXT NOT NULL,
  path_display TEXT NOT NULL,
  claim_kind TEXT NOT NULL CHECK(claim_kind IN ('intended','modified')),
  PRIMARY KEY(task_id, path_key, claim_kind)
);
