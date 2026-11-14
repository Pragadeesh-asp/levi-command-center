# Known issues and verification gaps

Updated: 2026-11-14

## Phase 2 final audit — ready for approval review

The user reports that Chrome, Edge, Brave, Opera and Firefox were each registered, assigned an isolated temporary test profile, launched through LEVI, visually confirmed at `https://example.com/`, and manually closed. The read-only SQLite inspection found the five browser registrations and corresponding temporary profiles; filesystem inspection found browser-generated files under all five test roots. These facts corroborate registration and profile initialization, not process creation, visible navigation, or manual closure. No screenshots, per-launch PID transcript, or persistent launch log were supplied. Record the launch outcomes as **user-reported**, not independently evidenced. See `BROWSER_VERIFICATION.md` for the matrix and inventory.

## Profile registration integrity

Manual registration now rejects a canonical directory that is already active under another browser registration. Validation and insertion share a Rust SQLite `BEGIN IMMEDIATE` transaction; Windows path casing and normalization are covered by tests. Same-browser upsert behavior is preserved; archived cross-browser rows do not block registration and remain untouched. The identified Opera row is archived and has no account references.

This prevents re-registering a known active path under another browser. It cannot prove the family of an arbitrary unregistered path; manual path registration still relies on explicit family selection. Other existing profile records have not been exhaustively classified by directory ownership.

The browser test UI is implemented with harmless URL choices, explicit `unknown` launch outcome, PID/path diagnostics and a manual-close gate. It passed build and unit regression checks but was not independently exercised end-to-end in this closeout.

Browser closeout regressions passed: `npm run build`, `npm run lint`, `cargo test` (17 passed after the profile-integrity improvement), `cargo fmt --check`, and `git diff --check`; read-only SQLite `integrity_check` returned `ok` and `foreign_key_check` returned no rows. These checks do not prove browser page rendering or full UI interaction.

The extra Opera profile registration points to the Firefox temporary profile directory but is archived and has no account references. It remains untouched. The new active-path registration check prevents an active Firefox path from being registered again under Opera; it does not infer ownership of previously unregistered arbitrary paths.

The final read-only database audit found 5 browser installations, 7 profile rows (6 active and 1 archived), 0 accounts and 0 activity rows. `integrity_check` returned `ok`, `foreign_key_check` returned no rows, and no active profile directory was registered under more than one browser. The archived Opera/Firefox path duplicate remains in history and was not changed.

Other review risks: manual browser installation registration checks that the selected `.exe` exists but does not prove the executable matches the selected family. Detected-browser registration inserts the browser row before sequential profile registrations; a later profile conflict can leave a partially imported set. Neither case was changed in this narrowly scoped milestone.

Final-audit status: build/lint/Rust tests/formatting/diff and read-only SQLite checks passed. The user confirmed that the live UI showed the expected Phase 2.4 conflict message, rejected nonexistent executable/profile paths, and that minimize/restore/maximize/close/restart worked and the Firefox profile persisted. No screenshots were attached, so these are user-confirmed rather than screenshot-verified. The five-browser launch matrix is also user-reported. Live session contention/reuse remains untested. See `BROWSER_VERIFICATION.md` for evidence classification.

## Browser process contention

Windows launch checks now enumerate processes using `sysinfo`, compare executable paths and inspect readable command-line profile selectors. Exact Chromium user-data-root and profile matches may reuse that same session; different/unconfirmed profiles in a matching root are rejected. Firefox launches use `-no-remote`; an already-running exact profile is rejected instead of reused. When process details are unreadable or cannot be tied to a profile, the user receives a warning to close the browser normally and retry. LEVI never kills browser processes or deletes lock markers.

Detection remains a best-effort snapshot. Windows may restrict command-line access depending on process privileges. Browsers can start between the check and spawn, and singleton/locking behavior remains the final arbiter. A lock marker alone cannot establish ownership or prove a profile is active. The five user-reported sequential launches do not test contention or races. Do not describe process detection as race-free. Exact-session reuse is implemented but has not been verified against a deliberately active matching session.

## Validation and browser behavior

- Nonexistent executable and profile-directory rejection were confirmed by the user in the live UI with expected messages. Invalid/missing-path variants, inaccessible directories, malformed URLs and their UI messages remain untested end-to-end.
- Browser-specific executable detection and version reading passed in UI for Chrome and the existing Brave registration. Edge, Opera and Firefox version display were not individually verified.
- Current read-only inspection found five populated Phase 2.3 test roots and one empty Phase 2.2 scaffold. Exact paths, inventories and a cleanup proposal are in `BROWSER_VERIFICATION.md`. Nothing was deleted. One Opera row references the Firefox test folder, so resolve that registration before cleaning the folder. Cleanup requires explicit user approval; never include browser installation or personal profile paths.
- Phase 2.4 cross-family conflict behavior passed Rust tests and the user confirmed the expected error appeared in the actual desktop UI without changing a profile record. No screenshot was supplied.
- Browser installation/profile discovery reads display labels and paths only. It does not read cookies, passwords, authentication databases, or copy profile data.

## Window and application behavior

The real Tauri window started visibly, loaded the local SQLite dashboard and navigated to Browser Profiles in earlier verification. In the final manual session, the user confirmed minimize, restore, maximize, restore, normal close and restart worked; the Firefox profile remained registered after restart. No screenshots were supplied. Live browser-session contention remains untested.

## Approval status

Phase 2 is implemented and ready for approval review, not approved automatically. Required regression checks passed and the previously open manual UI/window checks are user-confirmed. Process contention/reuse remains a documented limitation and was not tested against personal browser sessions. Phase 3 has not started.

## Existing limitations

- Account transaction rollback is covered using temporary SQLite databases. A forced transaction failure was not injected into the live database.
- Persistent application log file is not configured; Tauri development output is available in the development session.
- Project/task CRUD, manual and automatic assignment, task history, account/browser/profile eligibility, dependencies and active file claims are implemented and covered by temporary-database tests. Native task assignment interactions remain unverified; structured checkpoint and handoff workflows are not yet implemented.
- The manual usage-limit action (migration 3) stores the exact user-entered message, optional reset time, checkpoint, waiting state, persistent reminder and activity transactionally. Same-account resume requires explicit availability verification and user authorization. No Claude page monitoring or automatic detection exists; the manual submit/resume screens were not exercised against a production event.
- Reminder records are persisted, shown in the Workforce Center and managed from a dedicated screen. A 30-second Rust scheduler and retry/recovery state are implemented, but real Windows toast delivery and permission-denial behavior have not been tested. Delivery runs only while LEVI is open; overdue reminders are checked on the next start. Recurrence is not implemented.
- The new Workforce Center renders from real account, task, reminder and activity tables. It is visually verified in the native Tauri app. It is a status view, not an automated agent monitor. Live model-backed Command Intelligence is not configured or implemented.
- The AI Center, provider integrations, Claude page monitoring, and automatic completion detection are not implemented. A future limit-monitoring integration needs an explicitly scoped, opt-in mechanism; the current external-browser architecture cannot inspect page content.
- LEVI AI and LEVI BROWSER are architectural plans only. No provider, model, browser Space, AI action or analytics data exists in the current implementation.
- SQLite backup/restore remains planned; see `DATABASE.md`.
## LEVI AUTOPILOT CORE implementation status (2026-11-14)

- Task dependencies and project-scoped file declarations/claims are implemented in the current uncommitted work. Claims are coordination metadata only; LEVI does not watch or lock files on disk, so external editors/processes can still modify a claimed file. The SQLite transaction prevents competing LEVI assignments from acquiring overlaps.
- Automatic assignment is opt-in and polls every 10 seconds only while the Command Center process is running. It does not start external Claude work. Waiting tasks remain reserved under their original account and claims.
- UI blockers are explanatory snapshots; Rust performs the authoritative dependency and path checks at assignment time. If another task changes between refresh and click, the backend rejection is displayed.
- File-path claims use normalized project-relative lexical paths, including Windows case folding, separators, dot segments, and trailing space/dot equivalence. They do not resolve NTFS junctions, symbolic links, hard links, or other filesystem aliases; workers should use consistent project-relative names. Claims are coordination metadata and do not prevent edits made outside LEVI.
- Automatic Claude limit detection, external worker execution, and actual Claude completion detection are not implemented. Usage limits remain user-reported.
- Migrations 004 and 005 regression tests passed; the latest full Rust suite passed 42 tests. The native app showed Overview, the Reminders screen, and the Task Queue empty state. Task assignment/blocker/conflict/release controls were not exercised because the production database has no task rows and no test tasks were inserted. Tauri dev startup logged transient `EBUSY` watcher warnings for its executable and PDB while Cargo was building; the window still opened and no subsequent runtime error was observed.

## Workforce verification update (2026-11-14)

- Project-account membership was added to the project editor and Rust persists membership changes with activity records in a `BEGIN IMMEDIATE` transaction. The native UI showed a worker selector and saved one worker to the new verification project. The initial project membership was absent from existing projects; task assignment requires explicit membership.
- The native dashboard, Reminders screen, and Task Queue empty state rendered. The queue reported zero persisted tasks. Native manual assignment, dependency blocker, file conflict and release behavior remain unverified: no production task/file-claim records were inserted for UI testing. Equivalent backend cases are covered by Rust tests.
- Latest regression checks passed: frontend build/lint, 42 Rust tests, formatting and diff checks. The production SQLite file previously passed read-only integrity and foreign-key checks after the UI-created verification project and membership. No production task, reminder, or file claim was inserted.
- Tauri dev can briefly emit `EBUSY` watcher messages while Cargo builds its executable/PDB. The app rendered after a clean relaunch. Avoid concurrent Cargo test/build and active Tauri dev sessions; stop only the verification-launched Tauri session before rebuilding.
- Current reminder gaps: actual Windows notification delivery and denial handling have not been verified; the polling scheduler does not run while the app is closed; recurring reminders are not supported.
- Reminder engine update: migration 006, scheduler, persistent claim/retry fields and reminder UI are implemented. Current Windows toast delivery and OS permission prompts are unverified; the feature is opt-in and no notification test was triggered. The scheduler runs only while the app is open, then performs catch-up after restart. Recurrence is not supported.
- Current native verification: window launch and dashboard/queue/reminder rendering are confirmed. Task queue transactions are backend-tested, while the queue's manual assignment, blocker inspection, file conflict and release flows remain unverified in the actual UI. No task records were created to avoid changing production data solely for testing.
