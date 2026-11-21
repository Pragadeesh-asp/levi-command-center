use crate::db::database;
use serde::Deserialize;
use sqlx::{SqlitePool, Transaction};
use std::sync::atomic::{AtomicBool, Ordering};
use tauri::{AppHandle, State};
use tauri_plugin_notification::NotificationExt;
use uuid::Uuid;

#[derive(Default)]
pub struct ReminderSchedulerState(AtomicBool);

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReminderInput {
    id: String,
    title: String,
    body: String,
    kind: String,
    due_at: String,
    account_id: Option<String>,
    project_id: Option<String>,
    task_id: Option<String>,
}

#[derive(Debug, sqlx::FromRow)]
struct DueReminder {
    id: String,
    title: String,
    body: String,
}

#[tauri::command]
pub async fn save_reminder(app: AppHandle, reminder: ReminderInput) -> Result<(), String> {
    let pool = database(&app).await?;
    save_reminder_transaction(&pool, &reminder).await
}

#[cfg(test)]
pub async fn create_ai_reminder_in_pool(
    pool: &SqlitePool,
    id: String,
    title: String,
    body: String,
    due_at: String,
    project_id: Option<String>,
    task_id: Option<String>,
) -> Result<String, String> {
    let mut tx = pool.begin_with("BEGIN IMMEDIATE").await.map_err(db_error)?;
    let id =
        create_ai_reminder_in_transaction(&mut tx, id, title, body, due_at, project_id, task_id)
            .await?;
    tx.commit().await.map_err(db_error)?;
    Ok(id)
}

pub async fn create_ai_reminder_in_transaction(
    tx: &mut Transaction<'_, sqlx::Sqlite>,
    id: String,
    title: String,
    body: String,
    due_at: String,
    project_id: Option<String>,
    task_id: Option<String>,
) -> Result<String, String> {
    let existing: Option<(String, String, String, Option<String>, Option<String>)> =
        sqlx::query_as("SELECT title,body,due_at,project_id,task_id FROM reminders WHERE id=?")
            .bind(&id)
            .fetch_optional(&mut **tx)
            .await
            .map_err(db_error)?;
    if let Some((old_title, old_body, old_due, old_project, old_task)) = existing {
        let normalized_due: Option<String> = sqlx::query_scalar("SELECT datetime(?)")
            .bind(due_at.trim())
            .fetch_one(&mut **tx)
            .await
            .map_err(db_error)?;
        if old_title == title.trim()
            && old_body == body.trim()
            && Some(old_due) == normalized_due
            && old_project == project_id
            && old_task == task_id
        {
            return Ok(id);
        }
        return Err(
            "The approved reminder action ID already belongs to a different reminder.".into(),
        );
    }
    let reminder = ReminderInput {
        id: id.clone(),
        title: title.clone(),
        body: body.clone(),
        kind: "one_time".into(),
        due_at: due_at.clone(),
        account_id: None,
        project_id: project_id.clone(),
        task_id: task_id.clone(),
    };
    save_reminder_in_transaction(tx, &reminder).await?;
    Ok(id)
}

#[tauri::command]
pub async fn update_reminder_state(
    app: AppHandle,
    reminder_id: String,
    action: String,
    snooze_until: Option<String>,
) -> Result<(), String> {
    let pool = database(&app).await?;
    update_reminder_state_transaction(&pool, &reminder_id, &action, snooze_until.as_deref()).await
}

#[tauri::command]
pub async fn set_reminder_notifications_enabled(
    app: AppHandle,
    enabled: bool,
) -> Result<(), String> {
    let pool = database(&app).await?;
    sqlx::query("INSERT INTO settings(key,value) VALUES('reminder_notifications_enabled',?) ON CONFLICT(key) DO UPDATE SET value=excluded.value,updated_at=datetime('now')")
        .bind(if enabled { "true" } else { "false" })
        .execute(&pool).await.map_err(db_error)?;
    Ok(())
}

#[tauri::command]
pub async fn get_reminder_notifications_enabled(app: AppHandle) -> Result<bool, String> {
    let pool = database(&app).await?;
    let enabled: Option<String> =
        sqlx::query_scalar("SELECT value FROM settings WHERE key='reminder_notifications_enabled'")
            .fetch_optional(&pool)
            .await
            .map_err(db_error)?;
    Ok(enabled.as_deref() == Some("true"))
}

#[tauri::command]
pub async fn start_reminder_scheduler(
    app: AppHandle,
    state: State<'_, ReminderSchedulerState>,
) -> Result<(), String> {
    if state
        .0
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .is_err()
    {
        return Ok(());
    }
    tauri::async_runtime::spawn(async move {
        loop {
            if let Err(error) = deliver_due_reminders(&app).await {
                log::error!("Reminder scheduler check failed: {error}");
            }
            tokio::time::sleep(std::time::Duration::from_secs(30)).await;
        }
    });
    Ok(())
}

async fn save_reminder_transaction(
    pool: &SqlitePool,
    reminder: &ReminderInput,
) -> Result<(), String> {
    let mut tx = pool.begin_with("BEGIN IMMEDIATE").await.map_err(db_error)?;
    save_reminder_in_transaction(&mut tx, reminder).await?;
    tx.commit().await.map_err(db_error)
}

async fn save_reminder_in_transaction(
    tx: &mut Transaction<'_, sqlx::Sqlite>,
    reminder: &ReminderInput,
) -> Result<(), String> {
    let title = reminder.title.trim();
    if title.is_empty() || title.chars().count() > 120 {
        return Err("Reminder title must contain 1–120 characters.".into());
    }
    if reminder.body.chars().count() > 4000 {
        return Err("Reminder details must be 4,000 characters or fewer.".into());
    }
    if !matches!(reminder.kind.as_str(), "one_time" | "deadline" | "check_in") {
        return Err(
            "Choose a supported reminder type. Recurring reminders are not available yet.".into(),
        );
    }
    let valid_due: Option<String> = sqlx::query_scalar("SELECT datetime(?)")
        .bind(reminder.due_at.trim())
        .fetch_one(&mut **tx)
        .await
        .map_err(db_error)?;
    if valid_due.is_none() {
        return Err("Enter a valid reminder date and time.".into());
    }
    validate_link(tx, "accounts", reminder.account_id.as_deref()).await?;
    validate_link(tx, "projects", reminder.project_id.as_deref()).await?;
    validate_link(tx, "tasks", reminder.task_id.as_deref()).await?;
    if reminder.id.trim().is_empty() {
        return Err("Reminder identifier is required.".into());
    }
    sqlx::query("INSERT INTO reminders(id,title,body,kind,due_at,account_id,project_id,task_id) VALUES(?,?,?,?,?,?,?,?)")
        .bind(reminder.id.trim()).bind(title).bind(reminder.body.trim()).bind(&reminder.kind)
        .bind(valid_due).bind(nonempty(reminder.account_id.as_deref()))
        .bind(nonempty(reminder.project_id.as_deref())).bind(nonempty(reminder.task_id.as_deref()))
        .execute(&mut **tx).await.map_err(db_error)?;
    insert_activity(
        tx,
        "reminder_created",
        &format!("Created reminder: {title}"),
        nonempty(reminder.account_id.as_deref()),
        nonempty(reminder.project_id.as_deref()),
        nonempty(reminder.task_id.as_deref()),
    )
    .await?;
    Ok(())
}

async fn validate_link(
    tx: &mut Transaction<'_, sqlx::Sqlite>,
    table: &str,
    id: Option<&str>,
) -> Result<(), String> {
    let Some(id) = nonempty(id) else {
        return Ok(());
    };
    let (table, filter) = match table {
        "accounts" => ("accounts", "archived_at IS NULL"),
        "projects" => ("projects", "archived_at IS NULL"),
        "tasks" => ("tasks", "archived_at IS NULL"),
        _ => return Err("Invalid reminder link type.".into()),
    };
    let query = format!("SELECT 1 FROM {table} WHERE id=? AND {filter}");
    let exists: Option<i64> = sqlx::query_scalar(&query)
        .bind(id)
        .fetch_optional(&mut **tx)
        .await
        .map_err(db_error)?;
    if exists.is_none() {
        return Err(format!(
            "The linked {table} record is missing or archived. Refresh the form and try again."
        ));
    }
    Ok(())
}

async fn update_reminder_state_transaction(
    pool: &SqlitePool,
    reminder_id: &str,
    action: &str,
    snooze_until: Option<&str>,
) -> Result<(), String> {
    let mut tx = pool.begin_with("BEGIN IMMEDIATE").await.map_err(db_error)?;
    let row: Option<(String, Option<String>, Option<String>, Option<String>)> = sqlx::query_as(
        "SELECT title,account_id,project_id,task_id FROM reminders WHERE id=? AND status IN ('pending','snoozed')",
    )
    .bind(reminder_id).fetch_optional(&mut *tx).await.map_err(db_error)?;
    let Some((title, account_id, project_id, task_id)) = row else {
        return Err("This reminder is no longer pending.".into());
    };
    let (sql, detail) = match action {
        "complete" => ("UPDATE reminders SET status='completed',completed_at=datetime('now'),delivery_claimed_at=NULL WHERE id=? AND status IN ('pending','snoozed')", "Completed"),
        "cancel" => ("UPDATE reminders SET status='cancelled',delivery_claimed_at=NULL WHERE id=? AND status IN ('pending','snoozed')", "Cancelled"),
        "snooze" => {
            let Some(until) = nonempty(snooze_until) else { return Err("Choose when to show this reminder again.".into()); };
            let valid: Option<i64> = sqlx::query_scalar("SELECT CASE WHEN datetime(?) IS NOT NULL AND julianday(?) > julianday('now') AND julianday(?) <= julianday('now','+30 days') THEN 1 ELSE NULL END")
                .bind(until).bind(until).bind(until).fetch_one(&mut *tx).await.map_err(db_error)?;
            if valid.is_none() { return Err("Snooze time must be in the future and within 30 days.".into()); }
            sqlx::query("UPDATE reminders SET status='snoozed',snoozed_until=datetime(?),delivered_at=NULL,delivery_claimed_at=NULL,delivery_retry_at=NULL,last_delivery_error=NULL WHERE id=? AND status IN ('pending','snoozed')")
                .bind(until).bind(reminder_id).execute(&mut *tx).await.map_err(db_error)?;
            insert_activity(&mut tx,"reminder_snoozed",&format!("Snoozed reminder: {title}"),account_id.as_deref(),project_id.as_deref(),task_id.as_deref()).await?;
            tx.commit().await.map_err(db_error)?;
            return Ok(());
        }
        _ => return Err("Choose complete, cancel or snooze.".into()),
    };
    sqlx::query(sql)
        .bind(reminder_id)
        .execute(&mut *tx)
        .await
        .map_err(db_error)?;
    insert_activity(
        &mut tx,
        if action == "complete" {
            "reminder_completed"
        } else {
            "reminder_cancelled"
        },
        &format!("{detail} reminder: {title}"),
        account_id.as_deref(),
        project_id.as_deref(),
        task_id.as_deref(),
    )
    .await?;
    tx.commit().await.map_err(db_error)
}

async fn deliver_due_reminders(app: &AppHandle) -> Result<(), String> {
    if !get_reminder_notifications_enabled(app.clone()).await? {
        return Ok(());
    }
    let pool = database(app).await?;
    let reminders = claim_due_reminders(&pool).await?;
    for reminder in reminders {
        let notification_id = stable_notification_id(&reminder.id);
        match app
            .notification()
            .builder()
            .id(notification_id)
            .title(&reminder.title)
            .body(&reminder.body)
            .show()
        {
            Ok(_) => mark_delivered(&pool, &reminder.id).await?,
            Err(error) => mark_delivery_failed(&pool, &reminder.id, &error.to_string()).await?,
        }
    }
    Ok(())
}

async fn claim_due_reminders(pool: &SqlitePool) -> Result<Vec<DueReminder>, String> {
    let mut tx = pool.begin_with("BEGIN IMMEDIATE").await.map_err(db_error)?;
    let due: Vec<DueReminder> = sqlx::query_as("SELECT id,title,body FROM reminders WHERE status IN ('pending','snoozed') AND delivered_at IS NULL AND (delivery_retry_at IS NULL OR delivery_retry_at<=datetime('now')) AND (delivery_claimed_at IS NULL OR delivery_claimed_at<=datetime('now','-10 minutes')) AND ((status='pending' AND due_at<=datetime('now')) OR (status='snoozed' AND snoozed_until<=datetime('now'))) ORDER BY CASE kind WHEN 'deadline' THEN 0 WHEN 'check_in' THEN 1 ELSE 2 END,due_at,created_at LIMIT 20")
        .fetch_all(&mut *tx).await.map_err(db_error)?;
    for reminder in &due {
        sqlx::query("UPDATE reminders SET delivery_claimed_at=datetime('now'),delivery_attempts=delivery_attempts+1 WHERE id=? AND delivered_at IS NULL")
            .bind(&reminder.id).execute(&mut *tx).await.map_err(db_error)?;
    }
    tx.commit().await.map_err(db_error)?;
    Ok(due)
}

async fn mark_delivered(pool: &SqlitePool, id: &str) -> Result<(), String> {
    sqlx::query("UPDATE reminders SET status='pending',snoozed_until=NULL,delivered_at=datetime('now'),delivery_claimed_at=NULL,delivery_retry_at=NULL,last_delivery_error=NULL WHERE id=? AND delivery_claimed_at IS NOT NULL AND status IN ('pending','snoozed')")
        .bind(id).execute(pool).await.map_err(db_error)?;
    Ok(())
}

async fn mark_delivery_failed(pool: &SqlitePool, id: &str, error: &str) -> Result<(), String> {
    let detail = error.chars().take(1000).collect::<String>();
    sqlx::query("UPDATE reminders SET delivery_claimed_at=NULL,delivery_retry_at=datetime('now','+5 minutes'),last_delivery_error=? WHERE id=? AND delivery_claimed_at IS NOT NULL AND status IN ('pending','snoozed')")
        .bind(detail).bind(id).execute(pool).await.map_err(db_error)?;
    Ok(())
}

fn stable_notification_id(id: &str) -> i32 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    id.hash(&mut hasher);
    ((hasher.finish() & 0x7fff_ffff).max(1)) as i32
}

async fn insert_activity(
    tx: &mut Transaction<'_, sqlx::Sqlite>,
    kind: &str,
    description: &str,
    account_id: Option<&str>,
    project_id: Option<&str>,
    task_id: Option<&str>,
) -> Result<(), String> {
    sqlx::query("INSERT INTO activity_logs(id,activity_type,description,source,account_id,project_id,task_id) VALUES(?,?,?,'manual',?,?,?)")
        .bind(Uuid::new_v4().to_string()).bind(kind).bind(description).bind(account_id).bind(project_id).bind(task_id)
        .execute(&mut **tx).await.map_err(db_error)?;
    Ok(())
}

fn nonempty(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|value| !value.is_empty())
}

fn db_error(error: sqlx::Error) -> String {
    format!("Database operation failed: {error}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};

    async fn pool(path: &std::path::Path) -> SqlitePool {
        let pool = SqlitePoolOptions::new()
            .max_connections(4)
            .connect_with(
                SqliteConnectOptions::new()
                    .filename(path)
                    .create_if_missing(true)
                    .foreign_keys(true),
            )
            .await
            .unwrap();
        sqlx::raw_sql(include_str!("../migrations/001_initial.sql"))
            .execute(&pool)
            .await
            .unwrap();
        sqlx::raw_sql(include_str!("../migrations/002_browser_registry.sql"))
            .execute(&pool)
            .await
            .unwrap();
        sqlx::raw_sql(include_str!("../migrations/003_workforce_checkpoints.sql"))
            .execute(&pool)
            .await
            .unwrap();
        sqlx::raw_sql(include_str!(
            "../migrations/004_task_queue_dependencies_and_claims.sql"
        ))
        .execute(&pool)
        .await
        .unwrap();
        sqlx::raw_sql(include_str!("../migrations/005_task_file_releases.sql"))
            .execute(&pool)
            .await
            .unwrap();
        sqlx::raw_sql(include_str!("../migrations/006_reminder_delivery.sql"))
            .execute(&pool)
            .await
            .unwrap();
        pool
    }

    fn reminder(id: &str) -> ReminderInput {
        ReminderInput {
            id: id.into(),
            title: "Review deliverable".into(),
            body: "Check the project output".into(),
            kind: "one_time".into(),
            due_at: "2020-01-01T10:00:00Z".into(),
            account_id: None,
            project_id: None,
            task_id: None,
        }
    }

    #[tokio::test]
    async fn approved_ai_reminder_is_idempotent_and_audited_once() {
        let dir = tempfile::tempdir().unwrap();
        let db = pool(&dir.path().join("ai-reminder.db")).await;
        let first = create_ai_reminder_in_pool(
            &db,
            "ai-action-test".into(),
            "Review output".into(),
            "Inspect the saved deliverable".into(),
            "2030-01-02T12:00:00Z".into(),
            None,
            None,
        )
        .await
        .unwrap();
        let replay = create_ai_reminder_in_pool(
            &db,
            "ai-action-test".into(),
            "Review output".into(),
            "Inspect the saved deliverable".into(),
            "2030-01-02T12:00:00Z".into(),
            None,
            None,
        )
        .await
        .unwrap();
        assert_eq!(first, replay);
        assert!(create_ai_reminder_in_pool(
            &db,
            "ai-action-test".into(),
            "Different action".into(),
            "Inspect the saved deliverable".into(),
            "2030-01-02T12:00:00Z".into(),
            None,
            None,
        )
        .await
        .is_err());
        let count: i64 =
            sqlx::query_scalar("SELECT count(*) FROM reminders WHERE id='ai-action-test'")
                .fetch_one(&db)
                .await
                .unwrap();
        let activity: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM activity_logs WHERE activity_type='reminder_created'",
        )
        .fetch_one(&db)
        .await
        .unwrap();
        assert_eq!(count, 1);
        assert_eq!(activity, 1);
    }

    #[tokio::test]
    async fn due_reminder_is_claimed_once_and_marked_delivered() {
        let dir = tempfile::tempdir().unwrap();
        let db = pool(&dir.path().join("due.db")).await;
        save_reminder_transaction(&db, &reminder("r1"))
            .await
            .unwrap();
        let claimed = claim_due_reminders(&db).await.unwrap();
        assert_eq!(claimed.len(), 1);
        assert!(claim_due_reminders(&db).await.unwrap().is_empty());
        sqlx::query(
            "UPDATE reminders SET delivery_claimed_at=datetime('now','-11 minutes') WHERE id='r1'",
        )
        .execute(&db)
        .await
        .unwrap();
        assert_eq!(claim_due_reminders(&db).await.unwrap().len(), 1);
        mark_delivered(&db, "r1").await.unwrap();
        assert!(claim_due_reminders(&db).await.unwrap().is_empty());
        let attempts: i64 =
            sqlx::query_scalar("SELECT delivery_attempts FROM reminders WHERE id='r1'")
                .fetch_one(&db)
                .await
                .unwrap();
        assert_eq!(attempts, 2);
    }

    #[tokio::test]
    async fn delivery_failure_is_recorded_and_retried_after_backoff() {
        let dir = tempfile::tempdir().unwrap();
        let db = pool(&dir.path().join("retry.db")).await;
        save_reminder_transaction(&db, &reminder("r2"))
            .await
            .unwrap();
        claim_due_reminders(&db).await.unwrap();
        mark_delivery_failed(&db, "r2", "permission denied")
            .await
            .unwrap();
        assert!(claim_due_reminders(&db).await.unwrap().is_empty());
        let row: (String, String) = sqlx::query_as(
            "SELECT last_delivery_error,delivery_retry_at FROM reminders WHERE id='r2'",
        )
        .fetch_one(&db)
        .await
        .unwrap();
        assert_eq!(row.0, "permission denied");
        sqlx::query(
            "UPDATE reminders SET delivery_retry_at=datetime('now','-1 minute') WHERE id='r2'",
        )
        .execute(&db)
        .await
        .unwrap();
        assert_eq!(claim_due_reminders(&db).await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn reminder_state_updates_are_persistent_and_snooze_resets_delivery() {
        let dir = tempfile::tempdir().unwrap();
        let db = pool(&dir.path().join("state.db")).await;
        save_reminder_transaction(&db, &reminder("r3"))
            .await
            .unwrap();
        claim_due_reminders(&db).await.unwrap();
        mark_delivered(&db, "r3").await.unwrap();
        let snooze_until: String =
            sqlx::query_scalar("SELECT strftime('%Y-%m-%dT%H:%M:%SZ','now','+5 minutes')")
                .fetch_one(&db)
                .await
                .unwrap();
        update_reminder_state_transaction(&db, "r3", "snooze", Some(&snooze_until))
            .await
            .unwrap();
        let row: (String, Option<String>, Option<String>) =
            sqlx::query_as("SELECT status,delivered_at,snoozed_until FROM reminders WHERE id='r3'")
                .fetch_one(&db)
                .await
                .unwrap();
        assert_eq!(row.0, "snoozed");
        assert_eq!(row.1, None);
        assert!(row.2.is_some());
        update_reminder_state_transaction(&db, "r3", "complete", None)
            .await
            .unwrap();
        let status: String = sqlx::query_scalar("SELECT status FROM reminders WHERE id='r3'")
            .fetch_one(&db)
            .await
            .unwrap();
        assert_eq!(status, "completed");
        assert!(update_reminder_state_transaction(&db, "r3", "cancel", None)
            .await
            .is_err());
    }

    #[tokio::test]
    async fn invalid_links_and_activity_failure_do_not_leave_partial_reminders() {
        let dir = tempfile::tempdir().unwrap();
        let db = pool(&dir.path().join("rollback.db")).await;
        let mut invalid = reminder("r4");
        invalid.account_id = Some("missing-account".into());
        assert!(save_reminder_transaction(&db, &invalid)
            .await
            .unwrap_err()
            .contains("missing or archived"));
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM reminders")
                .fetch_one(&db)
                .await
                .unwrap(),
            0
        );

        sqlx::query("CREATE TRIGGER reject_reminder_activity BEFORE INSERT ON activity_logs WHEN NEW.activity_type='reminder_created' BEGIN SELECT RAISE(ABORT,'forced activity failure'); END")
            .execute(&db).await.unwrap();
        assert!(save_reminder_transaction(&db, &reminder("r5"))
            .await
            .is_err());
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM reminders")
                .fetch_one(&db)
                .await
                .unwrap(),
            0
        );
    }
}
