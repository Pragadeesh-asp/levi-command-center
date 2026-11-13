# Database design

SQLite is the durable local source of truth at the existing `command-center.db` location under the unchanged Tauri identifier `com.local.claude-command-center`. Initial schema is `src-tauri/migrations/001_initial.sql` (version 1); Phase 2 adds `src-tauri/migrations/002_browser_registry.sql` (version 2). Migration changes must be additive/versioned and tested against a fresh database and an existing user database. Do not edit an already released migration in place or delete existing records.

## Relationships

```text
browsers 1 ── * browser_profiles 1 ── * accounts
projects * ── * accounts          (project_accounts)
projects 1 ── * tasks * ── 0..1 accounts
tasks 1 ── * task_history
accounts/projects/tasks 1 ── * reminders (optional references)
accounts/projects/tasks 1 ── * activity_logs (optional references)
accounts/projects/tasks 1 ── * deliverables (optional references)
settings: key/value application settings
```

## Entity responsibilities

- `browsers`: named installation, executable path/version, detection method, availability, and access timestamp.
- `browser_profiles`: registered existing browser profile, profile/user-data directories, detection method, availability, access timestamp and archival timestamp; unique by `(browser_id, profile_path)`.
- `accounts`: manually registered Claude identity, selected profile, conversation URL, project, operational status/priority, notes and access timestamps.
- `projects`, `project_accounts`: project metadata and multi-account assignments.
- `tasks`, `task_history`: task state and immutable field-change trail for status/owner updates.
- `reminders`: persisted due time, recurrence rule, status, snooze and optional associations.
- `activity_logs`: dated manual/system events, explicitly labeled by source.
- `deliverables`: references to existing files/repos/commits; no implicit file move or copy.
- `settings`: typed-at-read-time key/value preferences.

## Integrity rules

Foreign keys are declared on every relationship. Status and priority are constrained with CHECK clauses. Useful status/date and relationship indexes are included. Browser/profile deletion is restricted until references are resolved. Account, project and task links in historical records use `SET NULL` or cascade only where their dependent row has no value without its parent. Archival timestamps preserve normal operating history.

Migrations use `TEXT` timestamps in UTC ISO-8601-compatible form. App services use prepared parameters for all values. SQLite date comparisons must normalize to UTC. In migration 2, `browser_profiles.browser_id` references `browsers.id` with restrictive deletion; `accounts.browser_profile_id` references a registered profile, and lookup indexes support that mapping. Account create/update/archive and their corresponding activity insert run in a single SQLx transaction. Temporary-database tests inject activity insert failures and confirm rollback leaves neither partial account changes nor orphaned activity rows. A live temporary UI account was created, remapped to a temporary registered profile, and verified after application restart; its rows and isolated test directory were subsequently cleaned up. The pre-existing user database was not reset. Browser launch metadata and account last-accessed/activity writes are committed together. Profile registration removal archives the row and leaves its directory and browser data untouched. Browser profile metadata is limited to display names and paths; authentication data is neither queried nor copied.

## Backup and recovery (planned)

Phase 1 establishes the data directory and schema. Backup/restore UI and recovery verification remain future work. Before release, implement a SQLite-consistent backup (SQLite backup API or `VACUUM INTO`), retain dated backups, and verify restore into a separate database. Never overwrite the live database during recovery without an explicit user action.
