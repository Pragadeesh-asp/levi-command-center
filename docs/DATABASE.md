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

## Migration 009 — AI request recovery and approval proposals

Migration 009 is additive. It creates `ai_requests` with a unique request ID, conversation/user-message references, payload hash, processing/recovery counters, lease timestamps and explicit `processing`, `completed`, `failed` or `outcome_unknown` states. `ai_action_proposals` stores a strict typed action JSON value, a SHA-256 fingerprint, approval status, error and result. JSON validity, status values and reference uniqueness are constrained in SQLite. Rust performs request reservation/recovery and proposal review/execution inside `BEGIN IMMEDIATE` transactions. A changed proposal or stale displayed fingerprint is rejected before execution; task/reminder, activity/history and executed proposal status commit together. SQLx migration-runner tests apply migrations 001–009 twice to an isolated in-memory database and check the migration ledger/integrity; a deliberately broken migration confirms transactional rollback and retry on a disposable test database. Existing AI history is retained by the additive migration. The production database was not migrated in this phase.

### Required operator procedure before migration 009

Migration 009 will be applied by the normal Tauri SQL plugin migration runner the next time the app opens its database. Do not launch the application until the backup is complete and verified.

1. Close LEVI Command Center normally and confirm its process has exited. Do not copy only a live SQLite main file while WAL writes may be active.
2. Create a dated backup outside the repository. With the SQLite CLI installed, run in PowerShell (adjust only the backup folder if needed):

   ```powershell
   $db = Join-Path $env:APPDATA 'com.local.claude-command-center\command-center.db'
   $backupDir = Join-Path $env:USERPROFILE 'Documents\LEVI-backups'
   New-Item -ItemType Directory -Path $backupDir -Force | Out-Null
   $backup = Join-Path $backupDir ("command-center-pre-009-{0}.db" -f (Get-Date -Format 'yyyyMMdd-HHmmss'))
   sqlite3 $db ".backup '$backup'"
   if ($LASTEXITCODE -ne 0) { throw 'SQLite backup failed; do not launch LEVI.' }
   sqlite3 $backup 'PRAGMA integrity_check;'
   sqlite3 $backup 'PRAGMA foreign_key_check;'
   Get-FileHash -Algorithm SHA256 $backup
   ```

3. Continue only if backup creation succeeds, `integrity_check` prints `ok`, `foreign_key_check` prints no rows, and the backup hash is recorded. Keep the original backup unchanged.
4. Launch LEVI normally once to apply migrations 1–9. If startup reports a migration error, close the app and preserve both the untouched pre-migration backup and the current database for diagnosis. Do not manually edit `_sqlx_migrations` or rerun SQL by hand.
5. After successful startup, close LEVI and inspect the active database read-only. Confirm integrity, no foreign-key violations, and migration ledger version 9. Compare existing conversation/message counts with the pre-launch read-only counts; migration 009 must not remove history.
6. A restore has not been tested. To validate a restore, restore the backup to a separate disposable database and run integrity/FK checks there. Never overwrite the live database as a test. Restoring the pre-009 backup to production would discard all database changes made after that backup and should only be done as an explicit recovery decision with LEVI closed.

This procedure describes the preflight that was current on 2026-11-14. It was superseded by the verified run recorded below; the old statement that migration 009 is pending is historical and no longer current.

## Migration 009 production application and recovery verification (2026-11-16)

- With LEVI closed, the SQLite online backup API produced `C:\Users\praga\Documents\LEVI-backups\command-center-pre-009-20261116-171705.db` outside the repository (303,104 bytes; SHA-256 `7d3dea1d53c0107379c3eeca035373fbe1db3bd369deabd8575e5b4a427e9d5f`). Independent read-only validation found migrations 1–8, `integrity_check=ok`, no foreign-key violations and the expected existing row counts. The backup was left unchanged.
- A separate disposable restore was opened and checked successfully: integrity, foreign keys, migration history and row counts matched the backup. The verification script did not close its SQLite destination connection before temporary-directory cleanup. The exact disposable directory remains at `C:\Users\praga\AppData\Local\Temp\levi-restore-verification-6f8xw704`; the environment rejected removal, and no alternative cleanup was attempted. This temporary output is not the production database.
- Normal Tauri application startup ran the configured SQLx/Tauri migration runner; no migration SQL was applied manually. After a normal application-window close and dev-runner exit, read-only inspection confirmed migrations 1–9, `integrity_check=ok`, no foreign-key violations, and presence of `ai_requests` and `ai_action_proposals`. Existing counts remained projects=3, tasks=0, reminders=2, conversations=1, messages=1. The two new tables have zero rows. No verification records were inserted into production.
- This verifies one production backup and one restore into a separate disposable database, not an in-app backup/restore feature, scheduled backup policy, or recovery-over-production procedure. Keep the pre-009 backup as a read-only recovery point; restoring it over production would omit migration 009 and any later data.

## Phase 3 JARVIS provider and voice notes (initial snapshot, superseded below)

The AI Command UX and provider request controls reuse the existing `settings` key/value table and Windows Credential Manager; no migration was introduced. Model catalog results are transient and are not persisted. AI conversation/action storage remains migration 008/009. The user's exact FreeLLMAPI deployment endpoint is pending; do not create a guessed provider URL/model or initiate a request until configuration and credentials are explicitly authorized. Current implementation stores a single active provider endpoint, not a second independent provider slot or automatic fallback policy. Voice is not configured and stores no audio.

### Phase 3 provider settings correction (2026-11-16)

The implementation now stores primary values in the existing `ai_provider`, `ai_base_url`, and `ai_model` settings keys, and secondary values in `ai_secondary_provider`, `ai_secondary_base_url`, and `ai_secondary_model`. The `ai_manual_fallback_enabled` setting stores the user’s opt-in for **manual alternate-provider retry**; it does not enable automatic failover. Credentials are held separately in Windows Credential Manager, keyed by provider profile, protocol and normalized endpoint. No key material is stored in SQLite. These are additive key/value entries only: schema and migrations 001–009 are unchanged. The endpoint and model catalog is not written to any new database tables. No production provider settings were configured during verification.

## Automatic backups, shared pool and WAL (2026-11-22)

Implemented in `src-tauri/src/db.rs`. No schema change and no new migration.

**Startup guard.** The `levi-db-guard` plugin is registered after logging and before `tauri-plugin-sql`, and plugins initialize in registration order, so it runs before any migration:

1. No `command-center.db` yet (first launch): it does nothing; the SQL plugin creates and migrates the file.
2. The `_sqlx_migrations` ledger is behind the newest compiled migration: it writes `backups/command-center-pre-migration-vNNN-to-vMMM-<local time>.db` with `VACUUM INTO`, reopens the copy, switches it to a single-file rollback journal and requires `PRAGMA integrity_check` = `ok`. **If any of that fails, startup stops** with the reason in the log file and the database untouched. This replaces the manual pre-migration procedure above; keep using that procedure only if you want an extra copy outside the app folder.
3. Otherwise, if the newest daily snapshot is older than 24 hours, it writes `backups/command-center-daily-<local time>.db` the same way and keeps the newest 14 daily snapshots. A failed daily snapshot is logged and retried next launch; it never blocks startup. Pre-migration snapshots are never pruned.
4. It switches the database to WAL (`PRAGMA journal_mode=WAL`, a persistent file setting) so the plugin's pool and the Rust pool no longer block each other. `synchronous` stays at SQLite's default (FULL) for durability.

**Locations (Windows).** Database: `%APPDATA%\com.local.claude-command-center\command-center.db`. Snapshots: `%APPDATA%\com.local.claude-command-center\backups\`. Logs: `%LOCALAPPDATA%\com.local.claude-command-center\logs\`. Snapshots contain everything in the database, including AI conversation text; API keys are not in SQLite.

**Shared pool.** Rust commands now share one lazily created `SqlitePool` held in Tauri state (`db::DbState`) with the previous options: foreign keys on, 5-second busy timeout, never create the file. Paths follow tauri-plugin-sql, which resolves `sqlite:` against `app_config_dir` (identical to `app_data_dir` on Windows).

**WAL side files.** While LEVI runs, `command-center.db-wal` and `command-center.db-shm` sit next to the database. They are normal and are already covered by `.gitignore` (`*.db-*`). Never copy `command-center.db` alone while LEVI is running; use a snapshot or the SQLite online backup API.

**Restore a snapshot (manual; no in-app restore yet).**

1. Close LEVI completely.
2. Keep the current state first: `sqlite3 command-center.db ".backup 'command-center-before-restore.db'"`.
3. Delete `command-center.db-wal` and `command-center.db-shm` if they exist. They belong to the current database and must not be applied to the restored file.
4. Copy the chosen snapshot over `command-center.db`.
5. Start LEVI. If the snapshot predates the newest migration, the guard takes a new pre-migration snapshot and the SQL plugin migrates it.

**Verification.** `db.rs` tests cover first launch, pre-migration snapshot plus WAL switch, fail-closed startup when the backup folder can't be created, daily snapshot cadence and 14-file retention, the shared pool's foreign keys and WAL mode, and that the guard and the SQL plugin's `preload` entry name the same file. Not yet observed in the native app: the first launch with these changes will create the first daily snapshot and switch the existing database to WAL.
