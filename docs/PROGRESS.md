# Development progress

## Week 1 stabilization (2026-11-22)

Branch `wip/week1-stabilize` (local; not pushed). It starts from `7550872`, a snapshot of all previously uncommitted work (JARVIS phase 3, Settings, UI overhaul and migration 009, which was already applied to the local database). `artifacts/` screenshots stay untracked.

- **Shared SQLite pool.** Every command used to open a new `SqlitePool` (45 call sites). `src-tauri/src/db.rs` now holds one lazily created pool in Tauri state with the same options; other modules import `crate::db::database`.
- **Backups and WAL.** A `levi-db-guard` plugin runs before `tauri-plugin-sql`: verified `VACUUM INTO` snapshot before any pending migration (startup stops if it fails), one daily snapshot (newest 14 kept), then WAL mode. Details and the manual restore procedure are in `DATABASE.md`.
- **Log files.** `tauri-plugin-log` writes info-and-above to the console and rotating files (5 × 5 MB, local time) in `%LOCALAPPDATA%\com.local.claude-command-center\logs`. The reminder scheduler's `eprintln!` now logs as an error, so failures survive release builds.
- **Repository hygiene.** `.gitattributes` (LF everywhere; the index was already LF, so no files changed) and a GitHub Actions workflow on `windows-latest`: `npm ci`, build, oxlint, `cargo fmt --check`, clippy, `cargo test` (skips the Credential Manager round trip).
- **Checks passed:** `npm run build`, `npm run lint`, `cargo fmt --check`, 86 Rust tests (80 existing plus 6 new in `db.rs`; the Credential Manager round trip was skipped so nothing was written to the local credential store). Clippy shows 13 existing style warnings and none from the new code.
- **Not verified:** a native launch with these changes (the first one will create the first daily snapshot and switch the database to WAL), the CI workflow on GitHub (branch not pushed), and the main-bundle size warning (587 kB) is unchanged.
- Remaining audit items, including `sql:allow-execute`, raw SQL in React, JARVIS tool calling and streaming, are not part of this milestone.

## Premium desktop UI and usability update (2026-11-17)

- The active shared palette is Gunmetal Black `#1C2126`, Lunar White `#F4F4F0`, and Neon Lime `#C7F000`; semantic surface, text, focus, border, button, status, shadow, motion and reduced-motion tokens are defined in `src/levi-tokens.css`.
- Added a mobile-width responsive navigation drawer with a scrim and an independently scrolling workspace. Dashboard and section content use the workspace scroll region so the desktop shell remains stable.
- The global LEVI panel is a compact assistant surface with conversation, privacy details, local speech playback controls and a direct route to AI Command. It uses the existing primary provider and Rust request lifecycle; it does not add a provider request path.
- Added a real Settings page for local Windows speech voice/speed preferences, provider setup routing, and current privacy behavior. Theme, startup, export/restore and notification controls are explicitly marked planned.
- Native dashboard launch and rendering were visually inspected with the new palette. The automation exposed the native accessibility tree but rejected UI input for opening overlays, so drawer, Settings, and global assistant interactions remain unverified. Final `npm run build`, `npm run lint`, and `cargo test` passed (81 tests). No database, migration, app identifier or provider credential was modified in this UI update.
- No commit or push.

Updated: 2026-11-14

## Shared LEVI Design System V2 (2026-11-15)

- Added `src/levi-tokens.css` with the same canonical Electric Black `#080B12`, Electric Navy Blue `#1646A0`, and Electric Crimson Red `#DC2638` values as LEVI Browser, plus matching neutral surfaces, spacing, radii, elevations, motion and focus tokens.
- Added `src/levi-theme.css` as a restrained overlay for existing navigation, cards, buttons, forms, empty states and dialogs. The existing React/Tauri architecture and real SQLite data flow are unchanged.
- Command Center frontend build and lint passed. The first full Rust run found a time-dependent reminder snooze test; the test now derives its snooze instant from SQLite's current clock. The full suite subsequently passed (55 tests).
- No schema, application identifier, database or runtime behavior changes were made for this visual update. No commit or push.

Phase 1 review update (2026-11-16): added the same semantic background, surface, text, border, focus, button, status, shadow and interaction timing roles as Browser. The existing AI Command screenshot artifact was inspected; no native UI was launched during this review. `npm run build`, `npm run lint`, 55 Rust tests, `cargo fmt --check`, `cargo build`, and `git diff --check` passed. Project/task behavior and AI/database logic were not modified. Provider connectivity and interactive native regression checks remain unverified in this review.

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

## Checkpoint and Handoff Center (2026-11-14)

Implemented an additive migration 007 and SQLite-backed Checkpoint Center and Handoff Center. Checkpoints snapshot task/project/worker/profile context and capture completed work, remaining work, files, blockers and next action. The UI supports search, history, A/B comparison, context copying, and same-account resume only after explicit availability verification and authorization. Task completion reports atomically create a checkpoint, deliverable, review package, task history and activity; review feedback creates a manual Claude revision or Copilot handoff; verification records are append-only events, and only a user-recorded passing verification completes the task and releases file claims.

External ChatGPT, Claude and Copilot execution remains manual. The UI copies packages but does not send them or claim external work happened. The native app rendered the Dashboard, Checkpoint Center and Handoff Center; current production DB contained zero checkpoint/handoff/task rows, so transactional form flows were verified with Rust tests rather than production UI submissions. Migration 007 applied to the existing DB; read-only `integrity_check` was `ok` and `foreign_key_check` returned no rows. Rust regression suite: 50 tests passed; frontend build and lint passed. See the Handoff Center section in `DATABASE.md` and `KNOWN_ISSUES.md` for boundaries. No new commit was created; HEAD remains `92f474d9c88bda4e8241e6fe275b75f52886d8c6`.

## Final manual verification session (2026-11-14)

The user confirmed the following live UI and native window tests passed: duplicate active Firefox-path registration under Opera showed the expected Rust error with no profile row change; a nonexistent executable and nonexistent profile directory were rejected with their expected validation messages; minimize, restore, maximize, restore, normal close and restart worked; and Firefox Test Profile remained registered after restart. No screenshots were attached, so all of these outcomes are **user-confirmed**, not screenshot-verified. See `BROWSER_VERIFICATION.md` for the test-by-test evidence classification.

The final manual checks close the previously identified UI, invalid-path and window-control gaps. Live browser process contention/reuse remains untested and is deferred to avoid interfering with personal sessions. The five browser launch matrix remains user-reported as described earlier; it has no per-launch PID, screenshot or persistent-log evidence.

The final regression rerun passed: `npm run build`, `npm run lint`, `cargo test` (17 passed, 0 failed), `cargo fmt --check`, and `git diff --check`. No database or browser profile data was modified, no temporary profile was cleaned up, and no commit was created.

**Phase 2 is ready for approval review, not approved automatically. Phase 3 has not started.** Preserve the accumulated changes and wait for explicit approval before committing or beginning later-phase implementation.

## AI Command Intelligence and MVP stabilization — 2026-11-14

- Added migration 008 and the AI Command screen. The local deterministic workforce summary and handoff prompt flows use actual application records and persist in SQLite. They are labeled as local outputs, not live AI responses. Conversation persistence was verified through native UI use and a full application close/restart.
- OpenAI-compatible and Anthropic provider configuration is implemented in Rust. API credentials use Windows Credential Manager via the `keyring` crate; they are never stored in frontend storage or SQLite. A real provider connection test was **not performed** because no API key/provider credential is configured. Provider connectivity remains unverified; no live AI response is claimed.
- Native Tauri screens were opened and visually inspected: Dashboard, Projects, Tasks/Task Queue, AI Workforce, Reminders, Checkpoint Center, Handoff Center, AI Command, Accounts and Browser Profiles. Existing records rendered. Empty task/checkpoint/handoff states match the database; no task or handoff test rows were inserted. No browser was launched.
- Windows reminder delivery passed: an immediate reminder displayed in Windows Notification Center and was recorded as delivered; snooze reset delivery state without an extra attempt; completion persisted. A separately scheduled reminder was delivered after LEVI was closed and restarted. Both verification records were preserved. Windows Do Not Disturb was already on, so banner suppression is expected; notification history showed the notifications. Permission-denied behavior was not induced. Exact-once toast behavior across a crash remains inherently unguaranteed.
- Read-only production database checks: `integrity_check=ok`, `foreign_key_check=[]`; migrations 1–8 are successful. Counts: 5 browser installations, 7 profiles, 1 account, 3 projects, 0 tasks, 2 reminder verification records, 0 task checkpoints, 0 task handoffs, 1 AI conversation and 1 AI message. The application was not used to reset or alter existing business records; verification-created reminder/conversation data remains.
- Regression checks passed: `npm run build`, `npm run lint`, `cargo test -- --test-threads=1` (55 passed, 0 failed), `cargo fmt --check`, and `git diff --check`. Native Tauri development build and launch passed. Screens and interactions were verified visually; the native app closed normally. The app was not packaged/signed as a release installer.
- Backup/restore remains **not implemented and not verified**. There is no in-app backup/restore path or documented tested restore procedure; this is a release-readiness blocker for safe user-managed recovery.
- No Phase 3, LEVI Browser or Sunshine Home work was started. No commit has yet been created. The milestone commit is pending final diff review and excludes database files, browser profile data and generated build output.

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

## Mission — GitHub recovery and remote setup (2026-11-15)

- Verified GitHub identity: `Pragadeesh-asp`. Global Git author identity is `Pragadeesh-asp <pragadeesh409@gmail.com>`.
- Created and verified the private repository: <https://github.com/Pragadeesh-asp/levi-command-center>. Owner is `Pragadeesh-asp`; GitHub default branch is `main`.
- Configured `origin` as `https://github.com/Pragadeesh-asp/levi-command-center.git` and pushed `main` without force. Local `main` tracks `origin/main`.
- Preserved existing commit `07dcafdb33ce20c2bb7d12aead8cf0ce6f73642d` and all prior history. The historical local branch `master` remains intact at that commit. Created `main` from the same history and added only the `.gitignore` data-protection rules before the initial push.
- Initial push tip: `5d64305cd8777b1ed7f1f358adc05ed6c85fa566` (`chore: protect local and sensitive data from Git`). This section is a documentation-only follow-up commit pushed after the initial history transfer. See `git log -5 --oneline` and `git rev-parse HEAD` for the current pushed tip.
- The project source and application behavior were not changed during this GitHub setup. The repository was clean and synchronized with `origin/main` at final verification.

## Phase 2 — JARVIS Intelligence Foundation (implementation in progress; provider/native verification pending)

- Preserved the existing Tauri/React/Rust/SQLite architecture, Windows Credential Manager integration, migrations 001–008, and all pre-existing working-tree changes. Added migration 009 for idempotent AI request records and typed, approval-gated action proposals. The migration was exercised after migrations 001–008 against an isolated in-memory SQLite database; existing sample AI history remained intact and the foreign-key check was empty. The production database was inspected read-only and left unchanged, so migration 009 remains pending until a later normal application startup.
- Expanded the existing Command Intelligence context from curated local records (projects, tasks and blockers, workers/profile associations, reminders, deliverables, activity, provider/model and available capabilities). It does not include API keys. Provider responses are labeled as live AI; local summaries and handoff prompt generation remain separate deterministic records.
- Added typed `create_task` and `create_reminder` proposals only. Rust rejects unknown action kinds/fields and validates project/task links and bounded arguments. The UI displays exact parameters and requires explicit confirmation. Execution uses existing transactional task/reminder services; stable action IDs make retries idempotent. No SQL, shell, PowerShell, browser automation, account assignment or deletion tool is exposed. Request, assistant response and proposal persistence have rollback and duplicate-request tests.
- The UI exposes actual provider configuration/credential-store status, explicit connection testing, conversation history, local workspace summaries, handoff prompt generation, action review, and current request/error/approval states. Voice controls are deliberately unavailable; the interface does not request microphone access.
- Provider status is **not verified**: the production settings table has no provider configuration. No external request was sent, and Credential Manager contents were not inspected. The connection test remains available after the user configures a provider. No provider connectivity, invalid-key or timeout scenario is claimed as tested against a real provider.
- Voice architecture review: do not use the Edge Web Speech API for a privacy-local default because Microsoft's current Edge policy documentation says it uses Azure Cognitive Services and sends voice data off-device ([Microsoft Edge policy](https://learn.microsoft.com/en-us/deployedge/microsoft-edge-browser-policies/speechrecognitionenabled)). Windows `Windows.Media.SpeechRecognition` is not immediately available to this unpackaged Tauri app: it requires MSIX package identity and microphone consent, and dictation may depend on the Windows online speech setting ([Windows speech recognition requirements](https://learn.microsoft.com/en-us/windows/apps/develop/input/speech-recognition)). A local push-to-talk recognizer such as [Vosk](https://github.com/alphacep/vosk-api) or [whisper.cpp](https://github.com/ggml-org/whisper.cpp) should be benchmarked before selection; compare accuracy, latency, model size and resource use, and ship/download models only after explicit user choice. Keep recognition and speech synthesis as separate replaceable interfaces. No voice dependency or microphone capability has been added.
- Automated verification: `npm run build`, `npm run lint`, `cargo fmt --check`, `cargo build`, `git diff --check`, and all 64 Rust tests passed. An approval-path test found that proposal retrieval selected a conversation ID directly from the proposal table instead of joining through its request; both list and single-proposal reads now use the correct request join. Native UI verification and production migration application are deferred to avoid silently changing the existing user database during this phase. No commit or push was made.
- Remaining limitations: no configured real AI provider was available for live verification; voice is not implemented; action execution is limited to task/reminder creation; no automatic Claude limit detection or external AI execution is implemented. The added migration will run through the application's existing migration runner at its next database startup; verify a backup and normal migration behavior before release.

## Phase 2.1 — Intelligence reliability and safety closure (2026-11-16)

- Hardened approval integrity with a canonical SHA-256 fingerprint over versioned typed action parameters. The UI sends the displayed fingerprint; Rust checks it against both stored arguments and the stored fingerprint inside the same `BEGIN IMMEDIATE` transaction used for task/reminder creation, activity/history and proposal resolution. Stale or altered proposals require a refreshed review; rejected proposals cannot execute; repeated/concurrent approvals return the single committed result.
- Added persisted AI request payload hashes, request IDs, a two-minute processing lease and recovery attempt state. An active duplicate is blocked, a completed duplicate returns its saved response, and expired/interrupted requests are recorded as outcome unknown without automatic resend. The UI labels the uncertain result, preserves the prompt and requires an explicit new-request confirmation that warns about possible duplicate charges.
- Preserved migration 008's `ai_messages.response_kind` constraint. Unknown outcomes use its existing `error` message kind and expose the related `ai_requests.status` separately. This was caught by aligning the test fixture with migration 008 before completion.
- Added isolated SQLx migration-runner tests that apply migrations 001–009 twice, validate the ledger/integrity/foreign keys, and inject a partial SQL failure to verify rollback and retry. Migration 009 remains unapplied to production. A backup and migration checklist is in `DATABASE.md`; backup/restore has not been implemented or tested.
- Provider HTTP uses the existing provider adapters with a 45-second timeout, redirects disabled and sanitized status errors. No key was read, saved or printed; no provider connection was configured and no external request was sent. Streaming, cancellation and independent real-provider tests remain unverified/not implemented.
- Native UI launch was intentionally skipped because normal startup applies migration 009 to the production database. Frontend build/lint and isolated Rust tests are not native UI verification. The exact backup prerequisite and native/provider verification gaps remain in `KNOWN_ISSUES.md`.
- Verification results for this implementation are recorded in the closeout report; Git HEAD remains the pre-work baseline and no commit or push was created.
- Final automated rerun: Command Center `npm run build`, `npm run lint`, all 72 Rust tests, `cargo fmt --check`, `cargo build`, and `git diff --check` passed. The migration runner was exercised twice against an isolated DB and injected-failure rollback/retry passed. Browser regression checks also passed (`npm run build`, 16 Rust tests, `cargo fmt --check`, `cargo build`, `git diff --check`); no Browser files were modified. Production SQLite was opened read-only: integrity `ok`, no foreign-key rows, migrations 1–8, one existing conversation/message, and no `ai_requests` table. It was not modified.
- One defect found during recovery-test execution was an ambiguous `conversation_id` in the interrupted-request recovery query; it is now explicitly qualified. Another schema mismatch was corrected by storing recovery messages under the migration-008-supported `error` kind and exposing request status separately. Native UI, Credential Manager availability, provider connectivity, and backup/restore remain unverified. No commit or push was made.

## Phase 2.1 — final completion and native release verification (2026-11-16)

- Created a SQLite online backup outside the repository before startup: `C:\Users\praga\Documents\LEVI-backups\command-center-pre-009-20261116-171705.db` (303,104 bytes; SHA-256 `7d3dea1d53c0107379c3eeca035373fbe1db3bd369deabd8575e5b4a427e9d5f`). The backup was independently reopened read-only; it reported migrations 1–8, `integrity_check=ok`, no foreign-key violations, and the expected pre-migration record counts.
- A disposable restore to a separate temp database passed integrity, foreign-key, migration and record-count checks. The verification script left its destination connection open, so its exact temp directory remains; cleanup was rejected by the execution environment and was not attempted by another method. Path: `C:\Users\praga\AppData\Local\Temp\levi-restore-verification-6f8xw704`. It is unrelated to production data.
- The native Command Center was started normally. SQLx/Tauri applied migration 009; the migration ledger now contains versions 1–9. After a normal close, read-only SQLite checks passed (`integrity_check=ok`, no foreign-key rows). Existing counts were preserved: 3 projects, 0 tasks, 2 reminders, 1 conversation and 1 message. New `ai_requests` and `ai_action_proposals` tables are present with zero rows. No production test records were created.
- Native screenshot/accessibility inspection verified the dashboard and AI Command Intelligence render. The existing conversation and stored `LOCAL SUMMARY` loaded and were labeled as local; the page showed provider setup required, the provider test button disabled, and no live response. Loading provider status invoked the existing `has_secret` helper, which calls the Windows Credential Manager API and reduces its result to a boolean; no secret value was displayed or logged. No credentials were configured, no external AI request was sent, and no AI action was executed. The native process exited after closing the app window; the dev runner also exited cleanly.
- Final native interaction was read-only. Proposal approval/rejection, stale/duplicate handling and recovery UI were not exercised in the production database because no disposable native database was configured. These paths remain covered only by Rust/SQLite tests, not native end-to-end testing.
- Final regression results: Command Center `npm run build`, `npm run lint`, 72 Rust tests, `cargo fmt --check`, `cargo build`, and `git diff --check` passed. Browser regression checks passed (`npm run build`, 16 Rust tests, `cargo fmt --check`, `cargo build`, `git diff --check`); no Browser source was modified. Browser multi-tab manual verification items remain unverified as listed below and in Browser documentation.
- Provider connectivity, credential-store runtime availability, streaming/cancellation, voice, automatic Claude limit detection, packaged/signed release, and Windows installer behavior remain unverified or unimplemented. No commit or push was created. Phase 2 verification is complete to the demonstrated scope; it is not a claim of provider-connected or release-packaged status.

## Phase 3 — JARVIS Experience implementation update (initial snapshot, superseded below)

- The AI Command screen now presents a JARVIS identity/status area, saved conversation history, explicit local-summary vs live-response labels, a compact curated-context disclosure, provider/model settings, cancellation control, and rendered Markdown. Markdown links are inert text, and the Markdown renderer is lazy-loaded to keep the initial JavaScript bundle below the Vite warning threshold.
- Rust remains authoritative for provider requests, workspace context, SQLite persistence, typed action proposals and approval. Model-list retrieval is opt-in in the UI and requires an explicit confirmation; a chat connection test has a separate charge warning. No live provider call was made during this work.
- Provider selection response: OpenAI-compatible, with FreeLLMAPI preferred and OpenRouter secondary. The exact FreeLLMAPI deployment base URL is pending from the user. The UI no longer assumes the public OpenAI endpoint. The current saved configuration supports one active provider endpoint at a time; a separately saved secondary slot and automatic failover are not implemented. The existing explicit retry creates a new request and warns about uncertain duplicate charges; it is not provider failover.
- FreeLLMAPI's published repository describes OpenAI-compatible `/v1/chat/completions` and `/v1/models`, but this does not establish which deployment, URL, model catalog, or authentication key the user will use. OpenRouter's published OpenAI-compatible interface is documented at `https://openrouter.ai/api/v1`; neither service has been configured or tested against LEVI.
- Native Tauri dev UI visually rendered the AI Command screen, provider-setup state, saved local summary, bounded context disclosure, and unavailable voice controls. The native UI reported Windows Credential Manager unavailable, so key saving and real provider use are blocked pending diagnosis. No key was entered and no microphone permission was requested.
- Voice input and output remain intentionally unavailable. There is no selected/approved speech engine or tested local transcription/playback path. Controls are disabled and disclose that no recording/audio transfer occurs.
- Automated results after this update: `npm run build` passed (Markdown split into a lazy chunk; main JS 380.42 kB), `npm run lint` passed, `cargo test` passed (75 tests), `cargo fmt --check` passed after formatting, `cargo build` passed, and `git diff --check` passed. These checks do not establish provider connectivity or voice operation.
- No schema change was introduced for the JARVIS screen, provider model discovery, or cancellation. Existing migration 009 is unchanged by this update. No database rows were deliberately inserted or changed during native inspection beyond the application's normal read/startup behavior.
- Remaining blockers at that checkpoint: explain/fix the observed Credential Manager status; obtain exact FreeLLMAPI base URL; add separate provider profiles and explicit fallback choice; then run an authorized connection/model test. Credential status and provider profiles are updated in the Phase 3 closeout below. Voice and provider connectivity remain unverified.

## Phase 3 — Credential Manager and provider profiles (2026-11-16)

- Root cause of the reported generic “Credential Manager unavailable” state: the frontend initialized the provider configuration with `credentialStoreAvailable=false` before its asynchronous Rust `get_ai_config` response arrived. The native screenshot captured that transient state; it did not establish a Windows Credential Manager failure. The initial state is now `checking` and the UI gives separate messages for store unavailable, credential missing/available, lookup failure, invalid configuration, changed configuration, and an unset endpoint.
- A Windows-only Rust integration test uses a unique disposable Credential Manager entry and synthetic sentinel values to verify store availability, initial missing entry, write/read, replacement, removal and missing-entry handling. It passed. The dedicated test entry is removed by the test; it does not use provider keys or touch unrelated credential targets. Store errors are mapped to safe categories; credential contents and raw platform errors are not logged or shown.
- Primary FreeLLMAPI and secondary OpenRouter profiles now have independent provider protocol, endpoint and model settings, separate credential targets, provider-specific request fingerprints, and independent in-session connection results. The primary endpoint remains blank until the user supplies their exact FreeLLMAPI deployment URL. The secondary defaults to the documented OpenRouter base URL. No schema migration was needed; profile values and the manual-fallback preference use the existing `settings` table.
- Fallback is opt-in and explicitly manual. LEVI never changes provider during an in-flight request. Retrying on the alternate profile is a separately confirmed new request ID, with an additional duplicate-charge warning when the previous outcome is unknown. Existing request recovery/idempotency and action approval protections remain in place.
- OpenRouter’s documented model-list/chat paths and Bearer authentication were checked against its official docs. FreeLLMAPI project documentation describes `/v1/models`, `/v1/chat/completions` and Bearer authentication for its OpenAI-compatible interface; that does not determine the user’s installed deployment URL, auth key, model IDs or exact compatibility. No provider request, model discovery, key entry, or credential was used. [OpenRouter API docs](https://openrouter.ai/docs/api/api-reference/chat/send-chat-completion-request?explorer=true) · [FreeLLMAPI project docs](https://github.com/valorisa/FreeLLMAPI).
- Native UI verification: the AI Command page rendered. The primary profile correctly says its endpoint is unset and no credential lookup was attempted. Selecting secondary showed “Windows Credential Manager is ready; no credential is saved for this endpoint,” and the manual fallback control was visible and off. No key, model lookup, connection test, or chat request was submitted.
- Checks passed: `npm run build`, `npm run lint`, `cargo test` (79 passed), `cargo fmt --check`, `cargo build`, and `git diff --check`. The Windows credential round-trip is included in the 79 Rust tests. Provider profile hashing/setting, credential-state distinction, safe error-category, and OpenRouter model-name parsing tests also passed. No production database values were intentionally changed; no migration was added.
- Still blocked: the exact primary FreeLLMAPI endpoint/auth configuration must be supplied. A live provider test remains unauthorized and was not run. No API key was requested or entered. Voice remains unavailable. The current native UI check verifies store readiness and profile states, not a real provider connection or a packaged release. No commit or push was created; preserve all existing dirty work.

## JARVIS ecosystem transformation — implementation snapshot (2026-11-16)

- Added a global LEVI Command Intelligence launcher and expandable assistant panel across Command Center sections. It reuses the existing Rust provider commands, SQLite conversation history, bounded backend-selected workspace snapshot, primary/secondary profile selection, request IDs and typed action proposal workflow. The current section is explicitly included as interface context; saved workspace facts remain Rust-selected.
- Sending is user-initiated. No provider request, model discovery, or connection test was issued during this implementation. A proposed action is not executed from the global panel; the panel sends the user to the full AI Command page to inspect and approve exact parameters.
- Added local Windows WebView speech synthesis controls for voice choice, rate and stop. Microphone input, transcription, wake-word, and external speech services remain unimplemented; no audio permission was requested or audio transmitted.
- Extended semantic typography tokens and improved key Command Center and Browser chrome text sizes while preserving the Browser's native tab/WebView architecture and profile data directories. Browser website content is a separate native WebView and was not styled or inspected by this visual update.
- Browser page-context AI is not integrated. Reading selected text or page contents requires a deliberate native bridge and privacy review; a chrome overlay also cannot safely extend over native website WebViews without lifecycle/bounds work. No silent page collection is present.
- This is an implementation snapshot, not completion of the transformation. Native visual interaction, microphone input, Browser assistant, provider runtime behavior, model capability discovery and end-to-end restart checks remain unverified or unimplemented.
