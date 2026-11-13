# Known issues and unresolved work

Updated: 2026-11-13

- **Native toolchain absent:** Rust/Cargo and MSVC tools were not available during inspection. Tauri Rust host and installer are unverified. WebView2 Runtime is present.
- **Dashboard is a shell:** dashboard values are intentionally shown as em dashes and empty states. No operational data modules or database-backed analytics exist yet.
- **Navigation is shell-level:** navigation selection works, but destination modules are not implemented and should not be presented as completed features.
- **Persistent notifications not implemented:** plugin is registered, but reminder scheduler and startup recovery are Phase 4.
- **Profile launch not implemented:** no browser detection or process launch code exists yet.
- **Backup/restore not implemented:** required before release; see `DATABASE.md`.
- **Capabilities need native review:** SQL execute permission is currently granted to support parameterized app queries. Restrict it further if the plugin supports granular operations; no filesystem/shell capability is granted.
- **Native tests outstanding:** frontend build/lint and a SQLite schema smoke check passed; native Tauri build and app-launch tests require Rust/Cargo and MSVC.
