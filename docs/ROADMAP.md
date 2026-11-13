# Development roadmap

## Phase 1 — Foundation and architecture (complete)

- Tauri 2 + React 19 + TypeScript + Vite + Tailwind scaffold.
- Dark operations dashboard shell and navigation.
- SQLite relational schema and migration registration.
- Architecture, database, roadmap, progress, and issue documentation.
- Frontend build/lint verification and native Windows verification.

## Phase 2 — Account and browser management (current)

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

## Phase 6 — LEVI AI integration

- Provider-independent orchestrator, adapter interface, conversation manager, model registry, streaming, retries/timeouts, usage tracking, and provider selection.
- FreeLLMAPI default and OpenRouter fallback/manual selection, implemented only after reviewing current provider documentation.
- Secure OS-backed credential management and backend-only requests.

## Phase 7 — AI context and action engine

- Ground answers in actual application data; controlled read tools, confirmed actions, authorized automation, and audit history.

## Phase 8 — Analytics and command interface

- Global search, real-data commands/shortcuts, and analytics derived from persisted records with documented definitions.

## Phase 9 — Testing and release readiness

- Unit/integration/database/browser-launch tests, Windows compatibility testing, backup restore validation, accessibility review, production build, signed installer and release notes.

Each phase should have implementation notes and explicit evidence before it is reported complete. No phase should imply Claude's internal activity is observable. LEVI AI must not be implemented before its phase is authorized.
