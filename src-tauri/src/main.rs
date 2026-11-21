#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use tauri_plugin_sql::{Migration, MigrationKind};
mod browser;
mod db;
mod handoffs;
mod intelligence;
mod reminders;
mod workforce;

/// Writes to the console and to rotating files in the app log folder
/// (%LOCALAPPDATA%\com.local.claude-command-center\logs on Windows).
fn log_plugin() -> tauri::plugin::TauriPlugin<tauri::Wry> {
    use tauri_plugin_log::{RotationStrategy, Target, TargetKind, TimezoneStrategy};
    tauri_plugin_log::Builder::new()
        .targets([
            Target::new(TargetKind::Stdout),
            Target::new(TargetKind::LogDir { file_name: None }),
        ])
        .level(log::LevelFilter::Info)
        .max_file_size(5 * 1024 * 1024)
        .rotation_strategy(RotationStrategy::KeepSome(5))
        .timezone_strategy(TimezoneStrategy::UseLocal)
        .build()
}

fn main() {
    let migrations = vec![
        Migration {
            version: 1,
            description: "initial_relational_schema",
            sql: include_str!("../migrations/001_initial.sql"),
            kind: MigrationKind::Up,
        },
        Migration {
            version: 2,
            description: "browser_registry_metadata",
            sql: include_str!("../migrations/002_browser_registry.sql"),
            kind: MigrationKind::Up,
        },
        Migration {
            version: 3,
            description: "workforce_checkpoints_and_usage_limits",
            sql: include_str!("../migrations/003_workforce_checkpoints.sql"),
            kind: MigrationKind::Up,
        },
        Migration {
            version: 4,
            description: "task_queue_dependencies_and_file_claims",
            sql: include_str!("../migrations/004_task_queue_dependencies_and_claims.sql"),
            kind: MigrationKind::Up,
        },
        Migration {
            version: 5,
            description: "task_file_release_history",
            sql: include_str!("../migrations/005_task_file_releases.sql"),
            kind: MigrationKind::Up,
        },
        Migration {
            version: 6,
            description: "persistent_reminder_delivery_state",
            sql: include_str!("../migrations/006_reminder_delivery.sql"),
            kind: MigrationKind::Up,
        },
        Migration {
            version: 7,
            description: "checkpoint_snapshots_and_task_handoffs",
            sql: include_str!("../migrations/007_checkpoints_and_handoffs.sql"),
            kind: MigrationKind::Up,
        },
        Migration {
            version: 8,
            description: "ai_conversations",
            sql: include_str!("../migrations/008_ai_conversations.sql"),
            kind: MigrationKind::Up,
        },
        Migration {
            version: 9,
            description: "ai_requests_and_approved_actions",
            sql: include_str!("../migrations/009_ai_actions.sql"),
            kind: MigrationKind::Up,
        },
    ];
    let latest_migration = migrations.iter().map(|m| m.version).max().unwrap_or(0);
    let database_url = format!("sqlite:{}", db::DATABASE_FILE);
    tauri::Builder::default()
        // Plugins initialize in registration order: logging starts first, then
        // the guard backs up the database before the SQL plugin migrates it.
        .plugin(log_plugin())
        .plugin(db::guard_plugin(latest_migration))
        .plugin(
            tauri_plugin_sql::Builder::default()
                .add_migrations(&database_url, migrations)
                .build(),
        )
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .manage(db::DbState::default())
        .manage(reminders::ReminderSchedulerState::default())
        .invoke_handler(tauri::generate_handler![
            browser::detect_browsers,
            browser::validate_browser_path,
            browser::discover_profiles,
            browser::register_browser_profile,
            browser::refresh_profile_availability,
            browser::launch_account,
            browser::test_profile_launch,
            browser::save_account,
            browser::archive_account,
            workforce::save_project,
            workforce::archive_project,
            workforce::save_task,
            workforce::archive_task,
            workforce::assign_task,
            workforce::assign_next_eligible,
            workforce::release_task_assignment,
            workforce::set_auto_assignment,
            workforce::get_auto_assignment,
            workforce::mark_usage_limited,
            workforce::resume_usage_limited,
            handoffs::save_task_checkpoint,
            handoffs::report_task_completion,
            handoffs::record_review_feedback,
            handoffs::record_implementation_report,
            handoffs::record_task_verification,
            handoffs::mark_handoff_started,
            handoffs::resume_from_checkpoint,
            intelligence::get_ai_config,
            intelligence::get_ai_secondary_config,
            intelligence::save_ai_config,
            intelligence::set_ai_fallback_enabled,
            intelligence::remove_ai_api_key,
            intelligence::test_ai_connection,
            intelligence::list_ai_models,
            intelligence::cancel_ai_request,
            intelligence::create_ai_conversation,
            intelligence::list_ai_conversations,
            intelligence::get_ai_messages,
            intelligence::send_ai_message,
            intelligence::get_ai_action_proposals,
            intelligence::recover_interrupted_ai_requests,
            intelligence::resolve_ai_action_proposal,
            intelligence::add_local_workforce_summary,
            intelligence::add_handoff_prompt,
            reminders::save_reminder,
            reminders::update_reminder_state,
            reminders::set_reminder_notifications_enabled,
            reminders::get_reminder_notifications_enabled,
            reminders::start_reminder_scheduler,
        ])
        .run(tauri::generate_context!())
        .expect("error while running LEVI Command Center");
}
