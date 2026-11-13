# Known issues and unresolved work

Updated: 2026-11-13

- **Dashboard is a shell:** KPI values are intentionally shown as em dashes and empty states. No operational data modules or database-backed analytics are implemented.
- **Navigation is shell-level:** selecting sections updates the active navigation and breadcrumb; destination pages are not implemented yet.
- **Persistent notifications not implemented:** notification plugin is registered, but scheduler and restart recovery are Phase 4.
- **Profile launch not implemented:** no browser detection or process launch code exists yet.
- **Backup/restore not implemented:** required before release; see `DATABASE.md`.
- **No persistent app log configured:** successful native startup was checked through Tauri dev output and the live UI. A logging facility remains future work if persistent diagnostics are needed.
