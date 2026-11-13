# Known issues and unresolved work

Updated: 2026-11-13

- **Phase 2 end-to-end save retest pending:** the first frontend multi-statement account save hit SQLite `database is locked`; account writes now use atomic Rust/SQLx transactions. Unit and build checks passed, but the rebuilt window minimized and could not be restored through the desktop control service in this run.
- **Concurrent browser profile launch:** one test launch against the host's registered Brave profile emitted Chromium storage lock errors, likely because the profile was already in use. The app opened `about:blank` in Brave but did not verify authentication. Further process/profile contention testing is required before claiming support for concurrent same-profile launch.
- **Browser versions:** detected executables are identified, but version metadata currently shows “not detected.” Version extraction is deferred because browser CLI version flags may hand off to an existing profile process.
- **Manual custom paths and other browser launchers:** supported by the interface/backend design, but manual registration and launch error paths still need end-to-end Windows verification.
- **Projects/tasks/reminders/activity/analytics/output vault:** beyond Phase 2; navigation entries remain planned shells.
- **LEVI AI:** provider architecture is documented only. No provider, model, chat, credentials screen, or AI actions are implemented.
- **Backup/restore:** not implemented; see `DATABASE.md`.
- **Persistent app logging:** no logging plugin is configured. Native startup and Rust command output are available through the Tauri development session.
