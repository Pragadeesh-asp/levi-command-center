# Development roadmap

## Phase 1 — Foundation and architecture (current)

- Tauri 2 + React 19 + TypeScript + Vite + Tailwind scaffold.
- Dark operations dashboard shell and navigation.
- SQLite relational schema and migration registration.
- Architecture, database, roadmap, progress, and issue documentation.
- Frontend build/lint verification; native build contingent on Windows prerequisites.

## Phase 2 — Accounts and browser management

- Account and browser/profile registry with validation, search, filters, archive behavior.
- Safe detection and manual path registration; never inspect credentials or mutate profiles.
- Browser-specific profile launch commands, launch error handling, and tests across supported browsers.

## Phase 3 — Projects and tasks

- Project registry and assignments; tasks, list/Kanban views, filters, assignment/status history.
- Database-backed dashboard counts and overdue detection.

## Phase 4 — Reminders

- Persistent scheduler, one-time/recurring/deadline reminders, notifications, snooze and reschedule.
- Startup recovery and reliable clock/time-zone behavior.

## Phase 5 — Activity and output

- Activity timeline with manual/system provenance.
- Deliverable vault with safe opening of references; no automatic file moves.

## Phase 6 — Command center and analytics

- Global search and real-data commands/shortcuts.
- Analytics derived from persisted records and documented definitions.

## Phase 7 — Release readiness

- Unit/integration/database/browser-launch tests, Windows compatibility testing, backup restore validation, accessibility review, production build, signed installer and release notes.

Each phase should have implementation notes and explicit evidence before it is reported complete. No phase should imply Claude's internal activity is observable.
