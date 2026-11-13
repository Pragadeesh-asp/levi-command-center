# Database design

SQLite is the durable local source of truth. Initial schema is `src-tauri/migrations/001_initial.sql`; it is registered as migration version 1 in the Rust host and preloaded on app startup. Migration changes must be additive/versioned and tested against both a fresh database and an existing user database. Do not edit an already released migration in place.

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

- `browsers`: named installation and executable path.
- `browser_profiles`: named existing browser profile, unique by `(browser_id, profile_path)`.
- `accounts`: Claude identity, selected profile, conversation URL, operational status/priority, notes and access timestamps.
- `projects`, `project_accounts`: project metadata and multi-account assignments.
- `tasks`, `task_history`: task state and immutable field-change trail for status/owner updates.
- `reminders`: persisted due time, recurrence rule, status, snooze and optional associations.
- `activity_logs`: dated manual/system events, explicitly labeled by source.
- `deliverables`: references to existing files/repos/commits; no implicit file move or copy.
- `settings`: typed-at-read-time key/value preferences.

## Integrity rules

Foreign keys are declared on every relationship. Status and priority are constrained with CHECK clauses. Useful status/date and relationship indexes are included. Browser/profile deletion is restricted until references are resolved. Account, project and task links in historical records use `SET NULL` or cascade only where their dependent row has no value without its parent. Archival timestamps preserve normal operating history.

The migration uses `TEXT` timestamps in UTC ISO-8601-compatible form. App services should use prepared parameters for all values. SQLite date comparisons must normalize to UTC. Task assignment and status edits must be wrapped with their `task_history` inserts in one transaction.

## Backup and recovery (planned)

Phase 1 establishes the data directory and schema. Backup/restore UI and recovery verification remain future work. Before release, implement a SQLite-consistent backup (SQLite backup API or `VACUUM INTO`), retain dated backups, and verify restore into a separate database. Never overwrite the live database during recovery without an explicit user action.
