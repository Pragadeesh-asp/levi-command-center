#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use tauri_plugin_sql::{Migration, MigrationKind};
mod browser;

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
    ];
    tauri::Builder::default()
        .plugin(
            tauri_plugin_sql::Builder::default()
                .add_migrations("sqlite:command-center.db", migrations)
                .build(),
        )
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .invoke_handler(tauri::generate_handler![
            browser::detect_browsers,
            browser::validate_browser_path,
            browser::discover_profiles,
            browser::validate_profile,
            browser::refresh_profile_availability,
            browser::launch_account,
            browser::test_profile_launch,
            browser::save_account,
            browser::archive_account,
        ])
        .run(tauri::generate_context!())
        .expect("error while running LEVI Command Center");
}
