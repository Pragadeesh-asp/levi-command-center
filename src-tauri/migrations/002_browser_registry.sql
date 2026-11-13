ALTER TABLE browsers ADD COLUMN browser_version TEXT;
ALTER TABLE browsers ADD COLUMN detection_method TEXT NOT NULL DEFAULT 'manual';
ALTER TABLE browsers ADD COLUMN availability_status TEXT NOT NULL DEFAULT 'available';
ALTER TABLE browsers ADD COLUMN last_accessed_at TEXT;

ALTER TABLE browser_profiles ADD COLUMN user_data_directory TEXT;
ALTER TABLE browser_profiles ADD COLUMN detection_method TEXT NOT NULL DEFAULT 'manual';
ALTER TABLE browser_profiles ADD COLUMN availability_status TEXT NOT NULL DEFAULT 'available';
ALTER TABLE browser_profiles ADD COLUMN last_accessed_at TEXT;
ALTER TABLE browser_profiles ADD COLUMN archived_at TEXT;

CREATE INDEX idx_profiles_browser_active ON browser_profiles(browser_id, archived_at);
CREATE INDEX idx_accounts_profile_active ON accounts(browser_profile_id, archived_at);
