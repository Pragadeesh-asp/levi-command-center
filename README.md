# LEVI COMMAND CENTER

**Your Personal AI Operations Headquarters.**

A local-first Windows desktop workspace for coordinating Claude accounts and browser profiles, with projects, tasks, reminders, and deliverables planned for later phases.

## Current phase

Phase 2 implements the account registry, browser/profile registry, and safe one-click launch workflow. The application retains the original Tauri identifier and SQLite location so existing data remains available. See [docs/PROGRESS.md](docs/PROGRESS.md) and [docs/ROADMAP.md](docs/ROADMAP.md).

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
