# Database design

SQLite is the durable local source of truth at the existing `command-center.db` location under the unchanged Tauri identifier `com.local.claude-command-center`. Schema migrations 001–006 add the initial relational schema, browser registry, workforce checkpoints/usage-limit events, task dependencies/file claims, file release history, and reminder delivery state. Migration changes must be additive/versioned and tested against a fresh database and an existing user database. Do not edit an already released migration in place or delete existing records.

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
- `tasks`, `task_history`: task state and field-change trail. The current task service writes changes in the same transaction as task updates and activity records.
- `reminders`: persisted due time, recurrence rule, status, snooze and optional associations.
- `activity_logs`: dated manual/system events, explicitly labeled by source.
- `deliverables`: references to existing files/repos/commits; no implicit file move or copy.
- `settings`: typed-at-read-time key/value preferences.

## Integrity rules

Foreign keys are declared on every relationship. Status and priority are constrained with CHECK clauses. Useful status/date and relationship indexes are included. Browser/profile deletion is restricted until references are resolved. Account, project and task links in historical records use `SET NULL` or cascade only where their dependent row has no value without its parent. Archival timestamps preserve normal operating history.

Migrations use `TEXT` timestamps in UTC ISO-8601-compatible form. App services use prepared parameters for all values. SQLite date comparisons must normalize to UTC. In migration 2, `browser_profiles.browser_id` references `browsers.id` with restrictive deletion; `accounts.browser_profile_id` references a registered profile, and lookup indexes support that mapping. Account create/update/archive and their corresponding activity insert run in a single SQLx transaction. Temporary-database tests inject activity insert failures and confirm rollback leaves neither partial account changes nor orphaned activity rows. A live temporary UI account was created, remapped to a temporary registered profile, and verified after application restart; its rows and isolated test directory were subsequently cleaned up. The pre-existing user database was not reset. Browser launch metadata and account last-accessed/activity writes are committed together. Profile registration removal archives the row and leaves its directory and browser data untouched. Browser profile metadata is limited to display names and paths; authentication data is neither queried nor copied.

Project and task mutations, task field history and activity records are committed transactionally by Rust commands. Migration 3 additively adds `task_checkpoints` and `usage_limit_events`; it does not rebuild or replace existing tables. A usage-limit transaction writes a checkpoint when an active task exists, changes that task to waiting without clearing `account_id`, moves the account to waiting, records the exact user-provided message/reset status, creates a persistent reminder, and writes activity/history atomically. Unknown reset times remain NULL with `reset_status='unknown'`; a one-day reminder is a manual check-in only. A separate explicitly confirmed resume transaction verifies available browser/profile state, resolves the event/check-in, and returns the original task to Ready on the same account. Migration 3 and rollback/resume behavior are tested against temporary SQLite databases. Dependencies and project-scoped active file claims are added in migrations 4–5; reminder delivery/retry metadata is added in migration 6.

## Backup and recovery (planned)

Phase 1 establishes the data directory and schema. Backup/restore UI and recovery verification remain future work. Before release, implement a SQLite-consistent backup (SQLite backup API or `VACUUM INTO`), retain dated backups, and verify restore into a separate database. Never overwrite the live database during recovery without an explicit user action.
### Migration 004 — task queue dependencies and file claims

`task_dependencies` stores same-project prerequisite edges and `task_history` preserves dependency edits. Rust rejects self edges, cycles, cross-project edges and assignment while a prerequisite is incomplete. `task_file_declarations` stores project-relative intended/modified paths for the queue. `task_file_claims` preserves claim acquisition/release history and active ownership. Canonical path keys normalize slash direction and dot segments and fold case for Windows; claims conflict for equal or parent/child paths within one project. Assignment, claim acquisition, task history and activity are written in one `BEGIN IMMEDIATE` transaction. Completion, archive and explicit unassignment release active claims; the usage-limit workflow leaves waiting tasks' claims in place.

Migration 004 is additive; it does not rewrite or remove existing account, browser, project or task rows. Migration tests apply it after migrations 1–3 in isolated SQLite files. It has also been applied to the configured local database by the Tauri SQL migration runner.

Verification on 2026-11-14: the Tauri development application applied migration 004 to the existing application database. Read-only checks afterward returned `integrity_check=ok` and no foreign-key violations. Existing counts remained one account and zero task records; no production records were created or changed for testing.

Migration 005 adds append-only `task_file_releases` records so a task can release selected paths while remaining assigned. The native development application applied it additively on 2026-11-14; read-only checks confirmed all migrations through version 5, integrity `ok`, no foreign-key violations, zero tasks and the pre-existing single account.

Project membership continues to use the migration 1 `project_accounts` relation. The project editor now passes the selected account IDs to Rust, which verifies each account remains active, then changes membership and appends matching activity records in one `BEGIN IMMEDIATE` transaction. A temporary SQLite test covers add/remove behavior and rollback when an account is missing. The current native UI verification added the clearly labeled project `Phase 0 Workforce Queue Verification - 2026-11-14` and associated the existing verification worker; it did not change the existing Phase 1.1 project, create tasks, or alter browser/profile/account records. Read-only integrity and foreign-key checks passed after the change.

### Migration 006 — reminder delivery

Adds `delivered_at`, `delivery_claimed_at`, `delivery_retry_at`, `delivery_attempts` and `last_delivery_error` to the existing reminders table, plus an index for due delivery selection. It does not rewrite or delete reminder records. Rust claims due rows in a `BEGIN IMMEDIATE` transaction before sending, records success or a five-minute retry, and reclaims a claim older than ten minutes after restart/crash. The migration was applied by the native Tauri SQL migration runner; read-only checks returned `integrity_check=ok` and no foreign-key violations. The production reminder count remained zero during verification.
### Migration 007 — checkpoint snapshots and handoffs

Migration 007 adds project/profile references and display-name snapshots to `task_checkpoints`, then creates `task_handoffs` and append-only `task_handoff_events`. The migration is additive and preserves existing checkpoint/task/account/browser data. Handoffs retain account/profile display snapshots for historical context. Rust transaction tests cover checkpoint persistence, completion package creation and rollback, review decisions, original-worker revision, Copilot-to-verification flow, failed verification followed by passing retry, invalid transitions, and authorized same-account resume. The actual local database reports migrations 1–7 successful, `integrity_check=ok`, and no foreign-key violations; these checks were performed read-only. At verification time `task_handoffs` contained zero rows, and no production task/checkpoint/handoff rows were created for testing.

### Migration 008 — AI conversations

Migration 008 adds persistent AI conversation and message tables, scoped to local application use. It contains provider/model metadata and conversation content, but no API secret fields. The actual local database reports migrations 1–8 successful. Final read-only checks returned `integrity_check=ok` and no foreign-key violations. At closeout it contained one AI conversation and one persisted local summary message from native UI verification. API credentials are stored through Windows Credential Manager by the Rust backend, not SQLite. No live provider connection was tested because no key was configured.

## Release backup and restore status (2026-11-14)

Backup/restore is not implemented or verified. There is no in-app backup/restore interface and no tested recovery procedure. Before distributing the application as a user-data-bearing release, add and test a SQLite-consistent backup (SQLite online backup API or `VACUUM INTO`) and restore into a separate database. Validate integrity and foreign keys after restore. Never test restore over the live database or replace it without an explicit user action. The current closeout did not copy, reset, or modify the production database for testing; all verification reads used SQLite read-only mode.
