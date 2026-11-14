# Development progress

Updated: 2026-11-14

## Workforce Control Centre — Phase 1: Projects and Tasks (implemented; partial)

- Added database-backed project and task create/edit/archive screens and activated the Projects/Tasks navigation.
- Rust commands validate records and commit project/task changes with activity rows; task field changes also write `task_history` in the same transaction. Task assignment is manual and requires an active project plus an account mapped to that project.
- Reused migrations 1 and 2; no migration or production database record was changed. Temporary-database tests cover project persistence, task history/activity atomicity and rollback, project membership validation, and invalid dates.
- `npm run build`, `npm run lint`, Rust formatting, and `cargo test` passed (21 Rust tests). Native Tauri process launched with a visible titled window. The desktop automation exposed an empty WebView accessibility region, so Projects/Tasks visual navigation was not verified. Vite also found port 5173 occupied and selected 5174 while Tauri's configured `devUrl` remains 5173; inspect the existing dev process before relying on a future native UI run. No records were created in the existing database.
- Still missing: worker availability enforcement, safe queue scheduling, dependencies/file claims, checkpoints/handoffs, persistent reminder scheduling, workforce dashboard, AI integration, and automatic Claude limit detection. No completion or limit detection is claimed.

## Phase 1.1 — Native Windows foundation (complete)

- Windows x64 host `x86_64-pc-windows-msvc`; Rust/Cargo 1.100.0, stable MSVC toolchain, MSVC and WebView2 available.
- Native Tauri development build and desktop launch passed. The existing SQLite file remains under `%APPDATA%/com.local.claude-command-center/command-center.db`.
- Frontend build/lint and SQLite schema smoke checks passed.
- Git checkpoint: `0fdcfeb Verify native Windows foundation`.

## Phase 2 — Accounts and browser management (implemented; ready for approval)

- Additive migration 2 registers browser installations and profiles. Tauri identifier/database path remain unchanged.
- Rust discovery and manual registration support Chrome, Edge, Brave, Opera and Firefox. The app stores executable/profile metadata and launches with argument vectors, validates account URLs to HTTPS `claude.ai`, and records launch activity.
- Account/profile pages support registration, mapping, editing, search/filter, priority, archive and quick launch.
- Phase 2.1 verified account create/edit/archive and corresponding activity writes in single SQLx transactions. Temporary SQLite integration coverage confirms rollback for account/profile and activity writes, and persistence after reopening. Live temporary account edit/profile remap and restart persistence passed. The live database was not reset.
- Phase 2.1 desktop UI showed the dashboard and Accounts/Browser Profiles navigation. A Brave `about:blank` profile test opened; authentication was not checked.
- Known Chromium singleton and Firefox parent-lock markers have never been removed or modified. Version-resource reading was fixed for canonicalized Windows paths; Brave displayed `157.1.99.57` in Phase 2.1.

## Phase 2.2 — Browser launch verification (launch matrix reported complete in Phase 2.3; closeout review pending)

### Tested and passed

- Rechecked environment: Rust 1.100.0, Cargo 1.100.0, `stable-x86_64-pc-windows-msvc`, npm available.
- Windows registry and existing native discovery identify Chrome, Edge, Brave, Opera and Firefox installations. During the native run, Chrome version `157.0.8103.51` and existing Brave version `157.1.99.57` displayed.
- `npm run tauri dev` compiled the native Tauri app and opened the actual `LEVI COMMAND CENTER` window. Accessibility state showed rendered SQLite dashboard counts and all navigation. Browser Profiles navigation and isolated Chrome installation/profile registration rendered and saved.
- A temporary Chrome test profile was registered under `%TEMP%\levi-phase2-2-7c24a390c2074ce9b6f623a4f9314239`; it contains no user profile data. The exact temporary profile and browser registrations were removed from SQLite after verifying there were no account references. No process referenced the test path and no files existed under the test directory. Directory removal was rejected by command policy, so the five empty test directories remain for manual cleanup.
- Windows process guard implementation uses `sysinfo` process executable and command-line metadata. An exactly matched Chromium root/profile is allowed to route to that same browser profile. A same-root conflict, unconfirmed profile, unreadable command line, or already-running Firefox profile blocks with a manual close warning. The app does not terminate processes or manipulate lock files.
- The test profile URL is `https://example.com/`; account URLs remain restricted to HTTPS `claude.ai`. No sign-in or authentication is inferred.

### Tested but did not pass / blocked

- The profile test-launch button did not produce an observable launch through the available desktop automation input. A subsequent direct shell browser launch request was rejected by automatic command policy; no workaround was attempted.
- The desktop automation reported incoming user input for the native window. No further window control input was issued after refreshing the window state.

### Not tested

- At the end of Phase 2.2, the real browser launch matrix and loaded URL verification for Chrome, Edge, Brave, Opera and Firefox had not been completed. The later user-reported tests are recorded in Phase 2.3 and `BROWSER_VERIFICATION.md`.
- Profile ownership/reuse/contention against live browser sessions, process close ownership, or test-profile cleanup.
- Invalid/missing executable, invalid/missing/inaccessible profile and malformed URL UI messages.
- Default Claude URL, custom Claude URL and conversation URL in a launched browser.
- Minimize, restore, maximize, close and restart behavior during this milestone. Phase 2.1 did verify ordinary initial visibility and restart persistence.
- Actual authentication. Opening Claude is not sign-in evidence.

## Product direction

- Product identity is **LEVI ECOSYSTEM**: LEVI COMMAND CENTER (current), LEVI BROWSER and LEVI — COMMAND INTELLIGENCE (future). Tagline: “Your AI Workforce. Your Projects. Your Intelligence.”
- Architecture and roadmap docs record the provider-independent FreeLLMAPI default/OpenRouter secondary plan, Windows credential storage, backend-only AI requests, the hybrid permission model, alternative browser architectures, future project-level usage/time attribution and future schema concepts.
- No Phase 3 feature, AI provider, LEVI BROWSER, or new migration was implemented.

## Phase 2.2 regressions

- `npm run build`: passed (TypeScript and Vite production build).
- `npm run lint`: passed (oxlint).
- `cargo test`: 9 passed, 0 failed, including invalid executable/profile inputs, Claude URL restrictions, profile launch argument selection, lock-marker behavior and SQLite transaction rollback.
- `cargo fmt --check`: passed.
- `git diff --check`: passed; Git emitted only the repository's LF-to-CRLF working-copy notices.
- Final `npm run tauri dev`: native MSVC build succeeded and opened the real window. Windows reported title `LEVI COMMAND CENTER`, a nonzero window handle and a responding process.
- Browser matrix, URL loads, window controls and manual validation UI remain unverified; Phase 2.2 is not complete or approved until these receive a safe, observable run. See `KNOWN_ISSUES.md`.

## Phase 2.3 — Controlled browser verification closeout (awaiting review)

### Implemented in this preparation pass

- Account and profile launch commands now distinguish validated executable/profile, conflict-check result, process-creation result, requested URL and unknown browser outcome. Successful process creation returns PID, executable path and profile path. No response claims the browser window or URL was confirmed.
- Account activity now uses `account_launch_requested` and records the PID/path; database activity or access-time write failures are attached as warnings after process creation rather than being reported as if process creation failed.
- Browser Profiles offers a controlled harmless URL choice: `https://example.com/` or offline `about:blank`. Backend validation accepts only those test choices or an HTTPS `claude.ai` account URL.
- Test launches are serialized in the UI. After process creation, the UI requires the user to close or confirm the test window is absent before enabling the next test profile.
- UI guidance directs the user to add an installation manually and register a newly created temporary profile; detected-installation registration can also register discovered profile paths, so it is not the prescribed test path.

### Temporary directory inspection

Read-only inspection of `C:\Users\praga\AppData\Local\Temp\levi-phase2-2-7c24a390c2074ce9b6f623a4f9314239` found nine empty directories and no files: `chrome`, `edge`, `brave`, `opera`, `firefox-profile`, and `Profile 1` children under the four Chromium roots. No non-shell process command line referenced the designated test directory. Nothing was deleted. These empty directories are available for controlled tests; after testing, close each browser manually before asking the user to clean the exact designated root.

### User-reported browser matrix

The user reports registering Chrome, Edge, Brave, Opera and Firefox, assigning each a temporary isolated profile, launching through LEVI, visually confirming `https://example.com/`, and manually closing each browser. The read-only database contains corresponding installation/profile rows and the temporary directories contain browser-generated files, corroborating registration and profile initialization. No per-launch PID transcript, screenshot, or persistent launch log was supplied; visible launch, navigation, process creation and manual closure are therefore labeled user-reported rather than independently captured. Full paths, file inventory, registration anomaly and exact evidence boundaries are recorded in `BROWSER_VERIFICATION.md`.

Current app database review found an extra Opera profile row named `Firefox Test Profile` pointing at the Firefox temporary profile path, in addition to the expected Firefox row. No account records reference either path. This registration anomaly is preserved for user review and was not changed.

The closeout inventory found five populated Phase 2.3 browser test roots and one empty Phase 2.2 scaffold. Nothing was deleted. Only the six exact roots listed in `BROWSER_VERIFICATION.md` are proposed for cleanup, pending user approval and confirmation that all test browser processes are closed. No user browser data or production database contents were modified.

### Closeout results

- `npm run build`: passed.
- `npm run lint`: passed.
- `cargo test` from `src-tauri`: passed, 10 passed and 0 failed.
- `cargo fmt --check` from `src-tauri`: passed.
- `git diff --check`: passed; Git emitted only LF-to-CRLF working-copy notices.
- Existing app SQLite database opened read-only: `PRAGMA integrity_check` returned `ok`; `PRAGMA foreign_key_check` returned no rows. Counts at inspection: 5 browser rows, 7 profile rows, 0 accounts and 0 activity rows. No database write/reset was performed.
- Closeout process enumeration found no running supported browser process. This is only a current snapshot and not historical proof that manual closure happened.
- Browser launch and page results are user-reported; no independent launch trace or screenshots are available. Exact temporary roots and inventory are in `BROWSER_VERIFICATION.md`.
- Manual invalid-path UI feedback, browser-specific contention under live conflicts, and minimize/restore/maximize controls remain unverified end-to-end.
- No commit created. The Phase 2 closeout is awaiting user review; do not start Phase 3.

### Profile registration integrity follow-up

- Registration now goes through one Rust command and a SQLite `BEGIN IMMEDIATE` transaction. It validates the selected browser/profile and rejects a canonical path that is active under any other browser registration. Windows comparison handles path casing, separators, dot segments and extended path prefixes.
- Same-browser registrations retain upsert/reactivation behavior. Archived registrations under a different browser are preserved and do not block a new registration.
- The frontend receives actionable conflict feedback from Rust and no longer inserts browser-profile rows separately.
- Seven tests cover active cross-family rejection, same-browser upsert, distinct directories, archived cross-family allowance, Windows path normalization, nonexistent directories and concurrent registrations. `cargo test`: 17 passed, 0 failed.
- Final regressions: `npm run build`, `npm run lint`, `cargo fmt --check` and `git diff --check` passed. The read-only SQLite integrity and foreign-key checks passed.
- The archived Opera/Firefox record pair in the existing database was not modified. Read-only integrity and foreign-key checks passed. No profile data or directories were changed.
- No schema migration or unrelated feature work was added. Ownership of an arbitrary unregistered manual path still cannot be reliably inferred; family confirmation remains a user responsibility.

## Final Phase 2 audit (2026-11-14; approval pending)

### Audit and regression results

- Git HEAD remains `58a7125b6580c50f36f96dd110c701de44ef4df1` (`Stabilize account transactions and browser profile safety`). The working tree has 13 modified tracked files and one added, untracked `docs/BROWSER_VERIFICATION.md`; no deleted files. No commit was created. See the final audit response for file-by-file phase attribution.
- `git diff --check`: passed; only Git LF-to-CRLF working-copy notices appeared.
- Final regression rerun: `npm run build`, `npm run lint`, `cargo test` (17 passed, 0 failed), and `cargo fmt --check` all passed.
- The existing app database was inspected in read-only mode. `integrity_check` returned `ok`; `foreign_key_check` returned no rows. It contains 5 browser registrations, 7 profile records (6 active, 1 archived), 0 accounts and 0 activity rows. No active cross-browser profile-path collision was found. The historical archived Opera/Firefox duplicate remains unchanged.

### Verification boundary

- Initial native Tauri window, dashboard rendering, account/browser navigation, prior account create/edit/restart persistence and transaction rollback tests have earlier verification evidence in this document.
- The five browser launch results are user-reported passes. The registrations and generated test directories corroborate setup, but no launch PID transcript, screenshots, or persistent launch logs establish the visible browser/page result independently.
- The Phase 2.4 conflict path is covered by Rust tests, including Windows normalization and SQLite concurrency. The user later confirmed the expected conflict message in the actual desktop UI and no profile row change; no screenshot was supplied.
- The user confirmed nonexistent executable/profile errors, minimize/restore/maximize, normal close/restart, and Firefox profile persistence. These were not screenshot-verified. Live browser contention/reuse remains untested. The existing browser-family/executable pair is not cryptographically or semantically verified by manual installation registration.
- Phase 2 is implemented and regression-clean and is **ready for approval review, not approved automatically**. Phase 3 has not started.

### Final audit classification

- **Passed:** frontend/Rust regressions and read-only SQLite integrity checks; Rust-level registration-conflict tests; earlier native visibility/navigation and account persistence checks.
- **User-reported pass:** sequential isolated-profile launch and example.com display in all five browsers; there are no independent PID/screenshot/log artifacts.
- **Failed:** no failures in the final regression rerun.
- **User-confirmed (no screenshots supplied):** actual desktop conflict-error presentation; nonexistent executable/profile UI feedback; minimize/restore/maximize; normal close/restart; Firefox profile persistence after restart.
- **Not tested:** live browser profile contention/reuse; inaccessible profile and malformed URL UI paths.
- **Deferred:** deliberate conflicts against personal profiles or automated process closure, to avoid disturbing user sessions. The current audit changed neither production DB records nor test-profile folders.
- `npm run build`: passed (TypeScript and Vite production build).
- `npm run lint`: passed (oxlint).
- `cargo test` (run from `src-tauri`): passed, 10 passed and 0 failed, including launch feedback, URL validation, browser/profile argument selection, profile lock-marker handling and SQLite transaction rollback.
- `cargo fmt --check` (run from `src-tauri`): passed.
- `git diff --check`: passed. Git emitted only LF-to-CRLF working-copy notices.
- Cargo commands initially run at repository root reported no `Cargo.toml`; rerunning from `src-tauri` passed. No source changes were needed for this working-directory correction.
- Browser launch results and the final manual checks are user-reported/user-confirmed as detailed above; no per-launch PID, screenshot or persistent log was supplied. Build/test checks do not independently verify those live behaviors.
- Do not create a commit until the user has reviewed the intended changes and verification status. Do not start Phase 3.

## Next

## Final manual verification session (2026-11-14)

The user confirmed the following live UI and native window tests passed: duplicate active Firefox-path registration under Opera showed the expected Rust error with no profile row change; a nonexistent executable and nonexistent profile directory were rejected with their expected validation messages; minimize, restore, maximize, restore, normal close and restart worked; and Firefox Test Profile remained registered after restart. No screenshots were attached, so all of these outcomes are **user-confirmed**, not screenshot-verified. See `BROWSER_VERIFICATION.md` for the test-by-test evidence classification.

The final manual checks close the previously identified UI, invalid-path and window-control gaps. Live browser process contention/reuse remains untested and is deferred to avoid interfering with personal sessions. The five browser launch matrix remains user-reported as described earlier; it has no per-launch PID, screenshot or persistent-log evidence.

The final regression rerun passed: `npm run build`, `npm run lint`, `cargo test` (17 passed, 0 failed), `cargo fmt --check`, and `git diff --check`. No database or browser profile data was modified, no temporary profile was cleaned up, and no commit was created.

**Phase 2 is ready for approval review, not approved automatically. Phase 3 has not started.** Preserve the accumulated changes and wait for explicit approval before committing or beginning later-phase implementation.

## 2026-11-14 — Workforce implementation update

- Preserved all prior uncommitted Phase 1 Projects/Tasks, Phase 2 browser/account, and worker-availability work. `HEAD` remains `0c6b8e08341734e2e67b195d594b80101421b0a2`; no commit was created.
- Vite development serving is fixed at `127.0.0.1:5173` with `strictPort: true`, matching Tauri `devUrl`. No unrelated process was terminated. Port 5173 was free before the current Tauri launch; prior auto-switching is prevented by strict mode.
- Added migration 3 for structured task checkpoints and usage-limit events. It was applied on native app startup without resetting the existing SQLite database. Read-only `integrity_check` returned `ok` and `foreign_key_check` returned no rows. Existing verification project/account rows remain; the active verification project and account were not removed.
- Added the `AI Workforce` screen with actual account/profile/task/reminder/activity queries. Native Tauri screenshot and accessibility tree verified the dashboard and Workforce Center render, show the existing registered Brave test worker, and load real summary records. No account or browser was launched as part of this verification.
- Added an explicit manual usage-limit action. It records user-entered message/checkpoint/reset information, preserves the task’s account assignment while setting task/account to waiting, creates a persisted reminder, and writes history/activity in one `BEGIN IMMEDIATE` transaction. Missing reset time is stored as unknown; the associated one-day reminder is only a manual check-in. Same-account resume requires availability verification, user authorization and browser/profile validation. No fabricated limit report was submitted to the production database.
- Rust tests: 26 passed, including usage-limit success, rollback and confirmed-resume behavior, availability checks, project mapping, active assignment concurrency and Phase 2 browser/profile integrity. The first new rollback-test assertion was corrected to distinguish the existing creation history from the attempted waiting transition; the rerun passed.
- `npm run build`, `npm run lint`, `cargo fmt --check`, `git diff --check`: passed. The Tauri development build and native application launched and rendered. UI navigation Overview → AI Workforce and real SQLite-backed status were visually verified. Closing via UI was not confirmed; the dev session was stopped by this task.
- Still incomplete: user-facing checkpoint browser and review/revision/Copilot handoff workflow; live AI connectivity; automatic Claude usage-limit detection; native verification of the usage-limit submit workflow. No authentication success or external-agent execution is claimed.

## LEVI AUTOPILOT CORE — queue and assignment vertical slice (2026-11-14)

Implemented in the current working tree. Additive migrations 004 and 005 introduce task dependency edges, project-scoped file declarations, claim lifecycle history and explicit per-file release history. Rust assignment validation is authoritative and runs in `BEGIN IMMEDIATE` transactions for manual and automatic queue assignment. It checks active project, account/project membership, worker and browser/profile availability, dependency completion and overlapping project file claims. Project-account membership can now be managed from the project editor and is saved with activity records atomically. Same-account waiting tasks retain their claims; completion, archiving and explicit release free them. Assignment result UI appears only after the backend returns success, and labels assignment separately from work having started.

The Task Queue now has Ready/active/blocked/waiting/review filters, priority/project/worker/estimate, dependency blockers and chains, file conflicts/declarations, assignment history, manual assignment/release and a persisted opt-in automatic assignment setting. While LEVI Command Center is open, the application checks the enabled queue every 10 seconds; the Rust transaction remains authoritative. The Workforce Center now shows recent committed assignments and active file-claim conflicts. Actual Claude work execution and automatic Claude usage-limit detection remain unimplemented.


Verification update 2026-11-14: `npm run build`, `npm run lint`, 42 Rust tests, `cargo fmt --check`, and `git diff --check` passed in the latest run. A native Tauri session visually showed the dashboard and Task Queue; the project editor displayed and saved a worker membership in a clearly named verification project. The queue displayed the correct empty state with zero task records. Assignment, dependency blocker, file-conflict and release interactions were not attempted because there were no task records and testing would require inserting records into the production database. These paths are covered by Rust tests, but native end-to-end UI behavior remains unverified. Read-only checks after UI-created verification records returned `integrity_check=ok` and no foreign-key violations. No task rows were added. Earlier dev runs produced transient watcher `EBUSY` warnings while Rust was compiling its executable/PDB; a clean relaunch rendered the app. No completion or external Claude work is claimed.

## Phase 1 — Persistent reminder engine (implemented; live notification delivery unverified)

- Migration 006 adds delivery timestamp, delivery claim, retry/backoff, attempt count and last-error fields to the existing `reminders` table; it applied on the actual Tauri launch without resetting the database. Read-only `integrity_check` returned `ok`; `foreign_key_check` returned no rows; migration records 1–6 are present.
- The native Reminders screen rendered with zero production reminders. Users can opt in to desktop notifications after the OS permission request, create linked reminders, snooze for one hour, complete or cancel reminders. Create/state changes and activity records use Rust transactions. Existing manual usage-limit reporting continues to create reminders.
- A Rust scheduler starts after the frontend database connection/migrations are ready, polls due reminders every 30 seconds, claims rows transactionally, uses stable notification identifiers, records successful delivery, and retries failures after five minutes. Old claims can be recovered after ten minutes. Due reminders remain persisted while LEVI is closed and are checked after the next launch.
- Reminder tests cover single claiming, stale-claim recovery, delivery retry/backoff, snooze/reset, completion, invalid links and transaction rollback. The full Rust suite passed 42/42; `npm run build`, `npm run lint`, `cargo fmt --check` and `git diff --check` passed. The final queue interactions in the native UI and real Windows toast delivery remain unverified.
- No notification permission was requested and no test toast/reminder was created. Therefore actual Windows toast visibility, permission denial messaging and end-to-end delivery remain unverified. Recurring reminders and delivery while LEVI is closed are not implemented.
