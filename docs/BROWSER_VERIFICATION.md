# Phase 2 browser verification and closeout

Updated: 2026-11-14

## Evidence status

The user reported that all five browsers were registered, each used an isolated temporary profile, each launched through LEVI COMMAND CENTER, and each visibly displayed `https://example.com/`. That report is recorded as **user-reported pass**, not as independently captured UI evidence. No screenshots, launch PID transcript, or persistent launch log was supplied. Current database rows and test-profile files corroborate browser/profile registration and browser-created profile initialization, but cannot prove process creation or visible navigation by themselves.

| Browser | Installation registration | Executable path currently resolves | Temporary profile row and directory | Process creation, visible launch, example.com, manual close |
| --- | --- | --- | --- | --- |
| Google Chrome | User-reported pass; row present | Verified present on disk | User-data root `C:\Users\praga\AppData\Local\Temp\levi-phase2-3-chrome-test`; profile `Test Profile`; row present | User-reported pass; no saved PID/screenshot. No browser processes were observed during closeout inventory. |
| Microsoft Edge | User-reported pass; row present | Verified present on disk | User-data root `C:\Users\praga\AppData\Local\Temp\levi-phase2-3-edge-test`; profile `Test Profile`; row present | User-reported pass; no saved PID/screenshot. No browser processes were observed during closeout inventory. |
| Brave | User-reported pass; row present | Verified present on disk | User-data root `C:\Users\praga\AppData\Local\Temp\levi-phase2-3-brave-test`; profile `Test Profile`; row present | User-reported pass; no saved PID/screenshot. No browser processes were observed during closeout inventory. |
| Opera | User-reported pass; row present | Verified present on disk | User-data root `C:\Users\praga\AppData\Local\Temp\levi-phase2-3-opera-test`; profile `Test Profile`; row present | User-reported pass; no saved PID/screenshot. No browser processes were observed during closeout inventory. |
| Mozilla Firefox | User-reported pass; row present | Verified present on disk | Profile directory `C:\Users\praga\AppData\Local\Temp\levi-phase2-3-firefox-test\Test Profile`; Firefox row present | User-reported pass; no saved PID/screenshot. No browser processes were observed during closeout inventory. |

### Investigation of the Firefox/Opera profile association

The current app database was read using SQLite `mode=ro` and `PRAGMA query_only=ON`. The relevant records are:

| Field | Correct Firefox registration | Archived Opera registration |
| --- | --- | --- |
| Browser ID | `b6567729-75d1-43ec-a15b-3c870e836d06` | `663b35f0-c0a6-45c9-8def-3f8081fb5f5f` |
| Browser display name / family | Firefox / `firefox` | Opera / `opera` |
| Executable | `\\?\C:\Program Files\Mozilla Firefox\firefox.exe` | `\\?\C:\Users\praga\AppData\Local\Programs\Opera\opera.exe` |
| Browser created (SQLite UTC) | `2026-11-13 17:38:37` | `2026-11-13 17:33:17` |
| Profile ID | `de66f57b-0733-4f92-ab95-894733554c7d` | `3d5cb8f5-9a3f-42e5-9519-be266a2b8602` |
| Profile display name | `Firefox Test Profile` | `Firefox Test Profile` |
| Profile directory | `\\?\C:\Users\praga\AppData\Local\Temp\levi-phase2-3-firefox-test\Test Profile` | Same exact canonical directory |
| User-data directory | `NULL` | `\\?\C:\Users\praga\AppData\Local\Temp\levi-phase2-3-firefox-test` |
| Profile created (SQLite UTC) | `2026-11-13 17:41:02` | `2026-11-13 17:39:37` |
| Profile updated / archived (SQLite UTC) | Updated `2026-11-13 17:41:22`; not archived | Updated and archived `2026-11-13 17:40:10` |
| Detection method / availability | `manual` / `available` | `manual` / `available` (archived) |
| Related accounts | 0 (active 0) | 0 (active 0) |

The Opera row is archived, not an active browser/profile mapping. Its creation precedes the correct Firefox profile row by about 85 seconds and it was archived before that row was created. That sequence is consistent with a mistaken manual registration followed by correction, but the exact operator action cannot be established from these rows alone. No record or browser data was modified during this investigation.

#### Root cause

The registration path validates that the directory exists and is structurally plausible for the selected family; it does not establish that the directory belongs to that browser family.

1. The Browser Profiles manual form captures the selected `profileBrowserId` and the user-entered/chosen profile path. `saveProfile` calls `upsertProfile(browser.id, ...)`.
2. `upsertProfile` loads the selected browser's `family`, passes that family and the supplied path to Tauri `validate_profile`, then inserts the row with that selected `browser_id`.
3. Rust `validate_profile_paths` checks a supported family and an existing directory. For Chrome, Edge, Brave and Opera, it additionally checks an existing user-data directory and that the selected directory is its direct child. It does not inspect ownership markers or prove the path is an Opera profile. For Firefox, only the directory existence/path canonicalization checks apply.
4. The Firefox test directory is a direct child of the supplied parent directory, so it satisfies Opera's structural Chromium path checks. The UI/database association is therefore determined by the selected browser ID, not by detected profile-family provenance.

The database enforces that `browsers.family` is one of the supported values, `browser_profiles.browser_id` references a browser, and accounts reference an existing browser profile. It also enforces `UNIQUE(browser_id, profile_path)`, which prevents duplicate paths only within the same browser record. It does not constrain a profile path to one browser family or prevent the same path from appearing under two different browser IDs. The family `CHECK` is on the browser row; it does not validate the semantics of a related profile path.

#### Risk assessment

- **Current effective risk is low:** the Opera row has `archived_at` set; browser/profile list queries and both account-profile validation and launch lookup exclude archived profiles. The read-only query found no account references to either profile.
- **If an equivalent row were active and launched as Opera:** the launch builder would use Opera's Chromium flags `--user-data-dir=<Firefox test root>` and `--profile-directory=Test Profile`. It would start Opera against the supplied directory; there is no Firefox fallback. This might fail, encounter a storage lock, or cause Opera/Chromium state to be created or changed in the Firefox profile directory. No such launch was attempted in this investigation.
- **Wrong-browser data access is possible:** a manually misassociated path is passed to the selected browser executable. The app does not read/copy cookies or credentials, but the launched browser itself may read or write files in the selected profile path.
- **Account association follows the bad row:** an account assigned to an active misregistered profile would resolve through that profile's `browser_id` and launch the wrong browser family. This archived row currently has no account references and cannot receive a new account assignment through the normal validation path.
- **The gap is broader than Firefox/Opera:** all four Chromium families share the same structural validator, so directory shape alone cannot distinguish Chrome, Edge, Brave and Opera. Firefox accepts any existing directory for manual registration. Other manual cross-family mistakes are possible; this investigation did not audit every directory's content for ownership.

#### Implemented correction

- Profile registration now goes through the Rust `register_browser_profile` command. The frontend no longer validates in Rust and then inserts the profile separately through the SQL plugin.
- Rust validates the selected browser and profile, canonicalizes the requested directory, then checks other browsers' active profile paths inside the same SQLite transaction. Windows comparison normalizes separators, dot segments, extended path prefixes, and case. Existing stored paths are canonicalized when possible, with normalized path comparison as a fallback.
- The transaction starts with SQLite `BEGIN IMMEDIATE`; concurrent profile registrations serialize before duplicate checking and insertion.
- A matching active path under another browser registration is rejected with a message naming both browser registrations and instructing the user to use the existing registration or choose another directory.
- Same-browser duplicate registration retains upsert behavior, including reuse of the existing row and unarchiving that same-browser row. An archived match under a different browser does not block a new registration. No profile-family inference from unreliable directory markers was added.

Focused read-only rechecks returned `integrity_check = ok`, `foreign_key_check = []`, and exactly two records for the canonical Firefox test path (one archived Opera row and one active Firefox row). Both rows, including the archived Opera row, were left unchanged. Regression results after implementation: `npm run build` passed; `npm run lint` passed; `cargo test` passed with 17 passed and 0 failed; `cargo fmt --check` passed; `git diff --check` passed with only LF-to-CRLF working-copy notices. The Rust suite includes seven profile-registration integrity tests for cross-family rejection, same-browser upsert, distinct paths, archived cross-family rows, Windows path normalization, nonexistent paths and concurrency. Remaining unknowns: why the earlier Opera row was entered, whether Opera was ever launched with that path, whether it wrote anything there, and whether other manually registered directory paths have incorrect family provenance. No profile file contents or credentials were read; no test browser was launched and no cleanup was performed.

## Process-contention protection and limits

- Windows process enumeration compares executable path and readable command-line profile selectors. Exact Chromium profile matches may be routed to the same Chromium session. Same-root/different-profile and ambiguous matches are blocked. Firefox uses `-no-remote` and refuses an already-running exact profile.
- Browser process enumeration is a point-in-time, best-effort observation. Command lines may be inaccessible; a process can start after the check; browser singleton forwarding/locking may affect PID behavior. A lock marker alone does not identify its owner or prove a profile is active.
- Current closeout enumeration found no running Chrome, Edge, Brave, Opera or Firefox processes, and no test-root command-line matches. This does not prove what happened during the earlier user-run launches or eliminate process-check races.
- LEVI does not terminate browser processes, delete lock files, inspect credentials, copy profile data, or modify browser databases.

## Temporary test-directory inventory

Read-only recursive inventory was taken for the six exact roots below. It records metadata (paths, entry counts and byte totals); profile file contents, cookies and databases were not opened. All five Phase 2.3 roots contain browser-generated test-profile state and caches and are not empty.

| Cleanup candidate (exact root) | Inventory | User data boundary |
| --- | --- | --- |
| `C:\Users\praga\AppData\Local\Temp\levi-phase2-3-chrome-test` | 268 directories, 349 files, 32,154,184 bytes; includes `Default` and `Test Profile` plus browser cache/model data | Dedicated test user-data root; includes the `Test Profile` registered for Chrome. |
| `C:\Users\praga\AppData\Local\Temp\levi-phase2-3-edge-test` | 802 directories, 1,648 files, 110,444,343 bytes; includes `Test Profile` plus Edge component/cache data | Dedicated test user-data root; includes the `Test Profile` registered for Edge. |
| `C:\Users\praga\AppData\Local\Temp\levi-phase2-3-brave-test` | 90 directories, 158 files, 11,605,754 bytes; includes `Test Profile`, extension/component caches and browser state | Dedicated test user-data root; includes the `Test Profile` registered for Brave. |
| `C:\Users\praga\AppData\Local\Temp\levi-phase2-3-opera-test` | 64 directories, 156 files, 19,350,424 bytes; includes `Test Profile`, browser state and cache data | Dedicated test user-data root; includes the `Test Profile` registered for Opera. |
| `C:\Users\praga\AppData\Local\Temp\levi-phase2-3-firefox-test` | 36 directories, 172 files, 65,534,839 bytes; top-level contains `Test Profile` | Dedicated Firefox test profile directory. It is referenced by both the Firefox row and the anomalous Opera row. |
| `C:\Users\praga\AppData\Local\Temp\levi-phase2-2-7c24a390c2074ce9b6f623a4f9314239` | 9 directories, 0 files, 0 bytes; contains `brave`, `chrome`, `edge`, `opera` (each with empty `Profile 1`) and `firefox-profile` | Earlier Phase 2.2 empty test scaffold; no files to preserve were found. |

**Approval-only cleanup proposal:** after the user confirms the five Phase 2.3 test profiles are disposable, confirms all test browsers are closed, and resolves the Opera-to-Firefox registration anomaly, cleanup may remove only the six exact roots listed above. No cleanup has been performed. Do not broaden the target to `%TEMP%`, browser installation folders, AppData browser data, or any other path. The Phase 2.3 browser-profile registrations in SQLite are separate records and are not included in this filesystem cleanup proposal.

## Database and regression checks

The existing `%APPDATA%\com.local.claude-command-center\command-center.db` was opened with SQLite read-only mode (`mode=ro`, `query_only=ON`). `PRAGMA integrity_check` returned `ok`; `PRAGMA foreign_key_check` returned no rows. Counts at inspection: 5 browsers, 7 profiles, 0 accounts, 0 activity rows. No production data was written or reset.

- `npm run build`: passed.
- `npm run lint`: passed.
- `cargo test` from `src-tauri`: passed, 10 passed, 0 failed.
- `cargo fmt --check` from `src-tauri`: passed.
- `git diff --check`: passed; Git reported LF-to-CRLF working-copy notices only.

## Remaining verification limits

- Browser launch steps are user-reported; no per-launch PID, screenshot, or persistent application log is available for independent replay/audit.
- The manual close report is user-reported. The closeout process snapshot found no browser processes, but process enumeration is not a historical proof of closure.
- Browser contention behavior against each real browser was not stress-tested; only unit coverage and the reported sequential test are available.
- Inaccessible profile and malformed URL UI feedback remain unverified end-to-end. Nonexistent executable/profile feedback and window controls were subsequently user-confirmed in the final manual session; no screenshots were supplied.
- Authentication was neither tested nor inferred from opening a URL.

## Final audit classification

### Passed with inspectable evidence

- Frontend build and lint; 17 Rust tests; Rust formatting; diff whitespace check.
- Read-only SQLite integrity and foreign-key checks; current browser/profile counts and absence of active cross-browser path collisions.
- Rust-level profile registration conflict handling, Windows path-key normalization, archived-row allowance, same-browser upsert, nonexistent-path validation and concurrent serialization.
- Earlier Phase 2 evidence records a visible native Tauri window and Accounts/Browser Profiles navigation, plus account save/edit/remap and restart persistence. These checks predate the Phase 2.4 command change.

### User-reported pass, not independently captured

- Chrome, Edge, Brave, Opera and Firefox each launched using an isolated temporary profile, visibly showed `https://example.com/`, and were manually closed, as reported by the user. There are no saved per-browser launch PIDs, screenshots, or persistent launch logs.

### Final manual verification session (user-confirmed; no screenshots supplied)

The user confirmed the following live desktop checks. No screenshots or recordings were attached to this session, so these results are **user-confirmed**, not screenshot-verified:

| Check | Result | Evidence classification | Result detail |
| --- | --- | --- | --- |
| Duplicate active profile under another browser | Passed | User-confirmed; no screenshot supplied | The live UI displayed the expected Rust validation error for registering the active Firefox test directory under Opera. The user confirmed no profile record changed. |
| Nonexistent browser executable | Passed | User-confirmed; no screenshot supplied | The live UI rejected the deliberately nonexistent executable path with the expected validation error. |
| Nonexistent profile directory | Passed | User-confirmed; no screenshot supplied | The live UI rejected the nonexistent directory. The user confirmed browser data was unchanged. |
| Minimize and restore | Passed | User-confirmed; no screenshot supplied | The user confirmed the native window minimized and restored. |
| Maximize and restore again | Passed | User-confirmed; no screenshot supplied | The user confirmed both window states worked. |
| Normal close and restart | Passed | User-confirmed; no screenshot supplied | The user confirmed normal close and subsequent application restart. |
| Firefox profile persistence after restart | Passed | User-confirmed; no screenshot supplied | The user confirmed Firefox Test Profile remained registered after restart. |

The current source audit independently confirms that the registration dialog invokes the Rust `register_browser_profile` command and renders command errors inline; this is implementation evidence, not a screenshot of the manual run. The reported rejection and unchanged records are recorded as user-confirmed outcomes. No database write, profile cleanup, or browser-data modification was performed by this audit.

### Not tested end-to-end

- Existing-session reuse or conflict behavior with a live matching browser process.
- A dedicated screenshot-verified capture of the final manual checks above; none was supplied.

### Deferred

- Deliberately testing process contention against personal profiles or automatically closing browser processes was not done because that would risk interfering with user sessions. The final manual session did not include a live contention/reuse test. No production database rows or browser profile directories were changed during the audit.

No failures occurred in the final regression rerun. Historical automation restrictions that blocked an earlier browser attempt are superseded by the user's later manual report, but do not provide independent evidence for those launches.

## Status

Final Phase 2 audit on 2026-11-14: the read-only database check found 5 browser rows, 7 profile rows (6 active, 1 archived), no account/activity rows, and no active cross-browser path collision. The archived Opera/Firefox duplicate and all temporary directories remain untouched. Final regressions passed (`npm run build`, `npm run lint`, 17 Rust tests, `cargo fmt --check`, `git diff --check`).

The duplicate-registration error and both invalid-path errors were confirmed in the live UI by the user. Window controls, close/restart and profile persistence were also user-confirmed. No screenshots were supplied for these outcomes. The five-browser launch matrix remains user-reported without PID/screenshot/persistent-log evidence. Live browser contention/reuse remains unverified. Phase 2 is ready for approval review, but is not approved automatically. No Phase 3 work or commit was created.
