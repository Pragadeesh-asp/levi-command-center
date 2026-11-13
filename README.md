# Claude Command Center

**One Control Room. Every AI Account. Total Clarity.**

A local-first Windows desktop workspace for coordinating Claude accounts, browser profiles, projects, tasks, reminders, and deliverables.

## Current phase

Phase 1 establishes the Tauri/React foundation, database schema, and dashboard shell. Operational modules are not implemented yet. See [docs/PROGRESS.md](docs/PROGRESS.md) and [docs/ROADMAP.md](docs/ROADMAP.md).

## Development

Requirements: Node.js/npm for the frontend; Rust stable/Cargo, Microsoft C++ Build Tools with Windows SDK, and WebView2 Runtime for native Tauri development on Windows.

```powershell
npm install
npm run dev
```

Frontend verification:

```powershell
npm run build
npm run lint
```

Native desktop development:

```powershell
npm run tauri dev
```

See [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) and [docs/DATABASE.md](docs/DATABASE.md) before extending the app.
