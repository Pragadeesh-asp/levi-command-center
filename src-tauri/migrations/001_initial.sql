PRAGMA foreign_keys = ON;

CREATE TABLE browsers (
  id TEXT PRIMARY KEY NOT NULL,
  name TEXT NOT NULL,
  executable_path TEXT NOT NULL,
  family TEXT NOT NULL CHECK (family IN ('chrome','edge','brave','opera','firefox','other')),
  created_at TEXT NOT NULL DEFAULT (datetime('now')),
  updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE browser_profiles (
  id TEXT PRIMARY KEY NOT NULL,
  browser_id TEXT NOT NULL REFERENCES browsers(id) ON DELETE RESTRICT,
  profile_name TEXT NOT NULL,
  profile_path TEXT NOT NULL,
  created_at TEXT NOT NULL DEFAULT (datetime('now')),
  updated_at TEXT NOT NULL DEFAULT (datetime('now')),
  UNIQUE(browser_id, profile_path)
);

CREATE TABLE projects (
  id TEXT PRIMARY KEY NOT NULL,
  name TEXT NOT NULL,
  description TEXT NOT NULL DEFAULT '',
  status TEXT NOT NULL DEFAULT 'active' CHECK(status IN ('active','paused','completed','archived')),
  priority INTEGER NOT NULL DEFAULT 2 CHECK(priority BETWEEN 1 AND 4),
  start_date TEXT,
  deadline TEXT,
  notes TEXT NOT NULL DEFAULT '',
  created_at TEXT NOT NULL DEFAULT (datetime('now')),
  updated_at TEXT NOT NULL DEFAULT (datetime('now')),
  archived_at TEXT
);

CREATE TABLE accounts (
  id TEXT PRIMARY KEY NOT NULL,
  name TEXT NOT NULL,
  identifier TEXT,
  browser_profile_id TEXT NOT NULL REFERENCES browser_profiles(id) ON DELETE RESTRICT,
  conversation_url TEXT,
  status TEXT NOT NULL DEFAULT 'idle' CHECK(status IN ('working','waiting','idle','blocked','completed','needs_attention')),
  priority INTEGER NOT NULL DEFAULT 2 CHECK(priority BETWEEN 1 AND 4),
  notes TEXT NOT NULL DEFAULT '',
  created_at TEXT NOT NULL DEFAULT (datetime('now')),
  updated_at TEXT NOT NULL DEFAULT (datetime('now')),
  last_accessed_at TEXT,
  archived_at TEXT
);

CREATE TABLE project_accounts (
  project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
  account_id TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
  assigned_at TEXT NOT NULL DEFAULT (datetime('now')),
  PRIMARY KEY(project_id, account_id)
);

CREATE TABLE tasks (
  id TEXT PRIMARY KEY NOT NULL,
  project_id TEXT REFERENCES projects(id) ON DELETE SET NULL,
  account_id TEXT REFERENCES accounts(id) ON DELETE SET NULL,
  title TEXT NOT NULL,
  description TEXT NOT NULL DEFAULT '',
  priority INTEGER NOT NULL DEFAULT 2 CHECK(priority BETWEEN 1 AND 4),
  status TEXT NOT NULL DEFAULT 'backlog' CHECK(status IN ('backlog','ready','in_progress','waiting','review','blocked','completed')),
  start_date TEXT,
  deadline TEXT,
  reminder_at TEXT,
  expected_minutes INTEGER CHECK(expected_minutes IS NULL OR expected_minutes >= 0),
  actual_minutes INTEGER CHECK(actual_minutes IS NULL OR actual_minutes >= 0),
  instructions TEXT NOT NULL DEFAULT '',
  completion_notes TEXT NOT NULL DEFAULT '',
  blocker TEXT NOT NULL DEFAULT '',
  created_at TEXT NOT NULL DEFAULT (datetime('now')),
  updated_at TEXT NOT NULL DEFAULT (datetime('now')),
  completed_at TEXT,
  archived_at TEXT
);

CREATE TABLE task_history (
  id TEXT PRIMARY KEY NOT NULL,
  task_id TEXT NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
  changed_at TEXT NOT NULL DEFAULT (datetime('now')),
  field TEXT NOT NULL,
  old_value TEXT,
  new_value TEXT,
  source TEXT NOT NULL DEFAULT 'manual' CHECK(source IN ('manual','system'))
);

CREATE TABLE reminders (
  id TEXT PRIMARY KEY NOT NULL,
  title TEXT NOT NULL,
  body TEXT NOT NULL DEFAULT '',
  kind TEXT NOT NULL DEFAULT 'check_in' CHECK(kind IN ('one_time','recurring','deadline','check_in')),
  due_at TEXT NOT NULL,
  recurrence_rule TEXT,
  account_id TEXT REFERENCES accounts(id) ON DELETE SET NULL,
  project_id TEXT REFERENCES projects(id) ON DELETE SET NULL,
  task_id TEXT REFERENCES tasks(id) ON DELETE SET NULL,
  status TEXT NOT NULL DEFAULT 'pending' CHECK(status IN ('pending','snoozed','completed','cancelled')),
  snoozed_until TEXT,
  created_at TEXT NOT NULL DEFAULT (datetime('now')),
  completed_at TEXT
);

CREATE TABLE activity_logs (
  id TEXT PRIMARY KEY NOT NULL,
  occurred_at TEXT NOT NULL DEFAULT (datetime('now')),
  activity_type TEXT NOT NULL,
  description TEXT NOT NULL,
  source TEXT NOT NULL DEFAULT 'manual' CHECK(source IN ('manual','system')),
  account_id TEXT REFERENCES accounts(id) ON DELETE SET NULL,
  project_id TEXT REFERENCES projects(id) ON DELETE SET NULL,
  task_id TEXT REFERENCES tasks(id) ON DELETE SET NULL
);

CREATE TABLE deliverables (
  id TEXT PRIMARY KEY NOT NULL,
  title TEXT NOT NULL,
  project_id TEXT REFERENCES projects(id) ON DELETE SET NULL,
  account_id TEXT REFERENCES accounts(id) ON DELETE SET NULL,
  task_id TEXT REFERENCES tasks(id) ON DELETE SET NULL,
  file_location TEXT,
  repository_url TEXT,
  commit_reference TEXT,
  notes TEXT NOT NULL DEFAULT '',
  completed_at TEXT NOT NULL DEFAULT (datetime('now')),
  created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE settings (
  key TEXT PRIMARY KEY NOT NULL,
  value TEXT NOT NULL,
  updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX idx_accounts_status ON accounts(status) WHERE archived_at IS NULL;
CREATE INDEX idx_accounts_profile ON accounts(browser_profile_id);
CREATE INDEX idx_tasks_status_deadline ON tasks(status, deadline) WHERE archived_at IS NULL;
CREATE INDEX idx_tasks_project ON tasks(project_id);
CREATE INDEX idx_tasks_account ON tasks(account_id);
CREATE INDEX idx_task_history_task_date ON task_history(task_id, changed_at DESC);
CREATE INDEX idx_reminders_due_status ON reminders(status, due_at);
CREATE INDEX idx_activity_date ON activity_logs(occurred_at DESC);
CREATE INDEX idx_activity_account ON activity_logs(account_id, occurred_at DESC);
CREATE INDEX idx_deliverables_project ON deliverables(project_id);
