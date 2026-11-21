CREATE TABLE ai_requests (
  id TEXT PRIMARY KEY NOT NULL,
  conversation_id TEXT NOT NULL REFERENCES ai_conversations(id) ON DELETE CASCADE,
  user_message_id TEXT NOT NULL UNIQUE REFERENCES ai_messages(id) ON DELETE CASCADE,
  assistant_message_id TEXT UNIQUE REFERENCES ai_messages(id) ON DELETE SET NULL,
  request_hash TEXT NOT NULL,
  status TEXT NOT NULL DEFAULT 'processing' CHECK(status IN ('processing','completed','failed','outcome_unknown')),
  attempt_count INTEGER NOT NULL DEFAULT 1 CHECK(attempt_count > 0),
  recovery_attempts INTEGER NOT NULL DEFAULT 0 CHECK(recovery_attempts >= 0),
  processing_started_at TEXT NOT NULL DEFAULT (datetime('now')),
  lease_expires_at TEXT,
  last_error TEXT,
  created_at TEXT NOT NULL DEFAULT (datetime('now')),
  updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE ai_action_proposals (
  id TEXT PRIMARY KEY NOT NULL,
  request_id TEXT NOT NULL REFERENCES ai_requests(id) ON DELETE CASCADE,
  assistant_message_id TEXT NOT NULL UNIQUE REFERENCES ai_messages(id) ON DELETE CASCADE,
  action_kind TEXT NOT NULL CHECK(action_kind IN ('create_task','create_reminder')),
  arguments_json TEXT NOT NULL CHECK(json_valid(arguments_json)),
  content_hash TEXT NOT NULL,
  status TEXT NOT NULL DEFAULT 'proposed' CHECK(status IN ('proposed','rejected','executed')),
  last_error TEXT,
  result_json TEXT CHECK(result_json IS NULL OR json_valid(result_json)),
  created_at TEXT NOT NULL DEFAULT (datetime('now')),
  decided_at TEXT
);

CREATE INDEX idx_ai_requests_conversation ON ai_requests(conversation_id, created_at DESC);
CREATE INDEX idx_ai_action_proposals_status ON ai_action_proposals(status, created_at DESC);
