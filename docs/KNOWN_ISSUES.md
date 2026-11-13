# Known issues and unresolved work

Updated: 2026-11-13

- **Account transaction failure injection in the live app:** Rust/SQLx temporary-database tests verify rollback of account/profile and activity writes. Live UI save/edit/restart succeeded; a forced SQL failure was not injected into the user's live database.
- **Browser lock ownership:** known Chromium singleton and Firefox parent-lock markers prevent another launch, and the app never deletes them. Marker detection is conservative and cannot reliably identify the owning/live process; processes are not terminated and sessions are not automatically reused. Absence of a marker cannot guarantee profile safety in every race.
- **Browser version UI:** Windows executable version metadata now reads correctly after Rescan; Brave displayed `157.1.99.57`. Chrome, Edge, Opera, and Firefox version display has not been individually checked.
- **Manual registration and launch matrix:** invalid/missing executable paths, unavailable profiles, URL navigation, and existing-process behavior across Chrome, Edge, Brave, Opera, and Firefox remain unverified end-to-end. Do not infer authentication from the Claude page opening.
- **Window stability:** native Tauri dev build launched, rendered, navigated, and restarted with the saved account visible. Initial WebView blank paint was transient during startup; the app rendered after startup settled. Restore/maximize interactions were not separately exercised.
- **Logging:** no persistent application log file is configured; Tauri dev output is available in the development terminal.
- **Projects/tasks/reminders/activity/analytics/output vault:** beyond Phase 2; navigation entries remain planned shells.
- **LEVI AI:** provider architecture is documented only. No provider, model, chat, credentials screen, or AI actions are implemented.
- **Backup/restore:** not implemented; see `DATABASE.md`.
- **Persistent app logging:** no logging plugin is configured. Native startup and Rust command output are available through the Tauri development session.
