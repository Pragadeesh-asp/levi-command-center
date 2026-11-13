# LEVI ECOSYSTEM roadmap

## Product direction

**LEVI ECOSYSTEM** connects three products:

- **LEVI COMMAND CENTER** — central operations for accounts, browsers, projects, tasks, reminders, activity, deliverables, settings and verified analytics.
- **LEVI BROWSER** — a future AI-first browsing environment with genuinely independent, persistent AI Spaces.
- **LEVI — COMMAND INTELLIGENCE** — a future assistant grounded in approved Command Center records and governed actions.

Tagline: “Your AI Workforce. Your Projects. Your Intelligence.” The products must share a coherent account/project model and secure local integration; they must not become unrelated apps. The Command Center remains practical, local-first and based on stored records. Missing data must be labeled rather than invented.

## Delivery status and sequence

### Phase 1 — Desktop foundation (complete)

Tauri 2, React, strict TypeScript, Vite, Windows desktop shell and versioned SQLite foundation.

### Phase 1.1 — Native Windows verification (complete)

Native MSVC build, visible desktop window, frontend checks and SQLite smoke test.

### Phase 2 — Account and browser management (implemented; ready for approval review)

Account registry, browser/profile metadata, manual registration, path validation and account-to-profile mapping. The user reports sequential isolated-profile launches and `example.com` navigation passed for all five supported browsers. Database/filesystem inspection corroborates registered paths and generated test profiles; no launch PID/screenshots/persistent launch log were supplied for independent evidence. See `BROWSER_VERIFICATION.md` for evidence scope, SQLite checks, process contention limits, temporary-directory inventory, and the Opera/Firefox registration anomaly. Phase 2 awaits user review and is not approved complete.

### Phase 2.1 — Account and database stabilization (verified)

Account create/edit/archive changes and activity rows are transactional. Temporary database rollback and restart persistence checks passed without resetting the user database.

### Phase 2.2 — Browser launch verification (user-reported matrix complete; evidence qualified)

The user reports that isolated profiles launched and displayed `https://example.com/` in Chrome, Edge, Brave, Opera and Firefox. Registrations and generated test profile directories were inspected, but launch PIDs, screenshots and persistent launch logs were not provided. The results remain labeled user-reported. A later final manual session confirmed invalid executable/profile UI feedback and native window controls; live browser contention/reuse remains unverified. See `BROWSER_VERIFICATION.md`.

### Phase 2.3 — Controlled browser verification workflow (implemented; manual UI checks user-confirmed)

Harmless test URLs, launch-state diagnostics and manual-close gating are implemented. The five browser launch outcomes are user-reported. In the final manual session, the user confirmed the duplicate registration error, invalid executable/profile validation, native window controls, close/restart, and Firefox profile persistence. No screenshots were supplied; these checks are user-confirmed rather than screenshot-verified. Live browser process contention/reuse remains unverified.

### Phase 2.4 — Browser profile registration integrity (implemented; regression verified)

Rust performs profile validation and registration in a SQLite `BEGIN IMMEDIATE` transaction, rejecting an active canonical directory registered under another browser. Same-browser upsert and archived-record behavior are tested. Windows path normalization is unit-tested. The user confirmed the conflict message appeared in the actual desktop UI and that no profile record changed. Phase 2 is ready for approval review, not automatically approved; see remaining contention limitations in `KNOWN_ISSUES.md`.

### Phase 3 — Project management (future; not started)

Projects, many-to-many account assignments, notes, decisions, milestones and historical account reassignment.

### Phase 4 — Task and workforce management (future; not started)

Backlog/ready/in-progress/waiting/review/blocked/completed states, account assignment, deadlines, priorities, deliverables, blockers and immutable task history. Provide useful list and Kanban views.

### Phase 5 — Reminder and notification engine (future; not started)

Persistent, restart-safe reminders connected to projects, tasks, accounts and deadlines; recurrence, snooze and user-confirmed account opening.

### Phase 6 — Activity history and Output Vault (future; not started)

Chronological, provenance-labeled activity and references to existing deliverables. Do not infer that browser presence means active work or move files implicitly.

### Phase 7 — LEVI AI provider integration (future; not started)

Provider-independent orchestration, provider interface, conversation manager, model registry, streaming/error handling and actual usage capture. FreeLLMAPI is the preferred provider; OpenRouter is secondary. Before implementation, inspect current official documentation for endpoints, authentication, models, streaming, quotas and usage fields. Do not assume advertised ecosystem token totals are a personal allocation.

### Phase 8 — AI context and confirmed action engine (future; not started)

Ground read answers in stored records. Show exact action parameters before confirmed actions. Keep any automation constrained to user-defined rules and auditable; allow it to be disabled. Sensitive, destructive or externally consequential actions require confirmation.

### Phase 9 — LEVI BROWSER architecture study and prototype (future; not started)

Compare embedded Tauri/WebView2 environments with separate browser processes before choosing. Measure memory per active Space, CPU, background processes, startup, switching, persistence, isolation and concurrent-session limits. Independent WebView2 user-data directories are a candidate, not a settled architecture. Do not build a rendering engine or assume WebView2 is automatically lightweight.

### Phase 10 — Browser and Command Center integration (future; not started)

Use a narrow secure local interface. Resolve targets from actual account/project records, present the target and request approval before opening the correct LEVI Browser Space. Do not launch ambiguous or fabricated mappings.

### Phase 11 — Usage and credit intelligence (future; not started)

Separate API-reported, platform-reported, estimated and manually entered usage. Store provider/model/platform/account/project/date and request identifiers where available. Show “Usage data unavailable” when exact web-platform usage is not exposed. Never invent quota resets or automate account rotation to evade limits.

### Phase 12 — Project intelligence and analytics (future; not started)

Derive workforce and project summaries only from verified persisted data. Label missing, estimated and manually entered values.

### Phase 13 — Browser assistance (future; not started)

Conversation analysis must be user initiated or explicitly authorized, respect platform terms and privacy, and show proposed record changes before saving.

### Phase 14 — Optimization, security and release (future; not started)

Accessibility, performance, backup/restore, security review, compatibility, installer signing and release readiness.

## Data model direction

Current schema migrations 1 and 2 remain authoritative. Potential future entities include `projects`, `project_accounts`, `tasks`, `task_history`, `reminders`, `activity_logs`, `deliverables`, `project_notes`, `project_decisions`, `milestones`, `work_sessions`, `usage_records`, `usage_limit_events`, `ai_providers`, `ai_models`, `ai_conversations`, `ai_messages`, `ai_action_logs` and `automation_rules`. These are design concepts only; do not create all of them now. Inspect the live schema before additive, versioned migrations and preserve existing records.

Project-level usage and time tracking must retain account/project attribution and provenance. A work session should record project, account, optional task, start/end, active duration and recording method; an open browser is not evidence of active work.

## Permission and credential principles

LEVI AI will have three permission levels: (1) read approved application data, (2) propose an exact action for confirmation, and (3) execute only predefined actions under explicit, revocable automation rules. Maintain an audit trail; never grant unrestricted OS access.

Provider credentials must stay out of React, frontend environment variables, web storage and Git. Store secrets through Windows credential storage, route requests through trusted Rust code and redact secrets from logs. Do not include browser authentication material in AI context.

## Browser architecture and safety principles

LEVI BROWSER must use genuine per-Space storage isolation so cookies, local storage, cache, history, settings and permissions remain independent while tabs within one Space share state. This is future work, not a capability of the current external-browser registry. Independent profiles do not imply independent identities or immunity from provider enforcement.

Never extract passwords or cookies, modify browser databases/configuration, delete profile lock files, copy profile data or terminate user browser processes. Do not implement IP spoofing, fingerprint manipulation or provider-limit evasion. Document process-detection limits and request manual conflict resolution when the active profile cannot be identified reliably.

## Phase boundaries

Only the current authorized milestone may be implemented. Future sections are product direction, not authorization to begin their code or migrations. Phase 3 and all later work require explicit approval after final Phase 2 review.
