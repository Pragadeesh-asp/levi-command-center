use crate::browser::database;
use serde::Deserialize;
use sqlx::{Sqlite, SqlitePool, Transaction};
use tauri::AppHandle;
use uuid::Uuid;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectInput {
    id: String,
    name: String,
    description: String,
    status: String,
    priority: i64,
    start_date: Option<String>,
    deadline: Option<String>,
    notes: String,
    #[serde(default)]
    account_ids: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskInput {
    id: String,
    project_id: Option<String>,
    account_id: Option<String>,
    title: String,
    description: String,
    priority: i64,
    status: String,
    deadline: Option<String>,
    expected_minutes: Option<i64>,
    instructions: String,
    completion_notes: String,
    blocker: String,
    #[serde(default)]
    depends_on: Vec<String>,
    #[serde(default)]
    intended_files: String,
    #[serde(default)]
    modified_files: String,
    #[serde(default)]
    released_files: String,
}

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QueueAssignmentResult {
    pub task_id: String,
    pub account_id: String,
    pub task_title: String,
    pub account_name: String,
}

#[tauri::command]
pub async fn assign_task(
    app: AppHandle,
    task_id: String,
    account_id: String,
) -> Result<QueueAssignmentResult, String> {
    let pool = database(&app).await?;
    match assign_task_transaction(&pool, &task_id, &account_id).await {
        Ok(result) => Ok(result),
        Err(reason) => {
            record_assignment_failure(&pool, &task_id, &account_id, &reason).await?;
            Err(reason)
        }
    }
}

async fn record_assignment_failure(
    pool: &SqlitePool,
    task_id: &str,
    account_id: &str,
    reason: &str,
) -> Result<(), String> {
    let mut tx = pool.begin_with("BEGIN IMMEDIATE").await.map_err(db_error)?;
    let task: Option<(Option<String>, String)> =
        sqlx::query_as("SELECT project_id,title FROM tasks WHERE id=?")
            .bind(task_id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(db_error)?;
    let Some((project, title)) = task else {
        return Ok(());
    };
    let account: Option<String> = sqlx::query_scalar("SELECT id FROM accounts WHERE id=?")
        .bind(account_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(db_error)?;
    insert_activity(
        &mut tx,
        "task_assignment_failed",
        &format!("Assignment of '{title}' rejected: {reason}"),
        account.as_deref(),
        project.as_deref(),
        Some(task_id),
    )
    .await?;
    tx.commit().await.map_err(db_error)
}

#[tauri::command]
pub async fn assign_next_eligible(app: AppHandle) -> Result<Option<QueueAssignmentResult>, String> {
    let pool = database(&app).await?;
    assign_next_eligible_transaction(&pool).await
}

#[tauri::command]
pub async fn release_task_assignment(app: AppHandle, task_id: String) -> Result<(), String> {
    let pool = database(&app).await?;
    release_task_assignment_transaction(&pool, &task_id).await
}

#[tauri::command]
pub async fn set_auto_assignment(app: AppHandle, enabled: bool) -> Result<(), String> {
    let pool = database(&app).await?;
    sqlx::query("INSERT INTO settings(key,value) VALUES('auto_assignment_enabled',?) ON CONFLICT(key) DO UPDATE SET value=excluded.value,updated_at=datetime('now')")
        .bind(if enabled { "true" } else { "false" }).execute(&pool).await.map_err(db_error)?;
    Ok(())
}

#[tauri::command]
pub async fn get_auto_assignment(app: AppHandle) -> Result<bool, String> {
    let pool = database(&app).await?;
    let value: Option<String> =
        sqlx::query_scalar("SELECT value FROM settings WHERE key='auto_assignment_enabled'")
            .fetch_optional(&pool)
            .await
            .map_err(db_error)?;
    Ok(value.as_deref() == Some("true"))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageLimitInput {
    account_id: String,
    exact_message: String,
    reset_at: Option<String>,
    checkpoint_summary: String,
    remaining_work: String,
    files_changed: String,
    last_successful_step: String,
    next_action: String,
    blockers: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResumeUsageInput {
    account_id: String,
    reset_verified: bool,
    user_authorized: bool,
}

#[tauri::command]
pub async fn mark_usage_limited(app: AppHandle, input: UsageLimitInput) -> Result<(), String> {
    let pool = database(&app).await?;
    mark_usage_limited_transaction(&pool, &input).await
}

#[tauri::command]
pub async fn resume_usage_limited(app: AppHandle, input: ResumeUsageInput) -> Result<(), String> {
    let pool = database(&app).await?;
    resume_usage_limited_transaction(&pool, &input).await
}

async fn mark_usage_limited_transaction(
    pool: &SqlitePool,
    input: &UsageLimitInput,
) -> Result<(), String> {
    let message = input.exact_message.trim();
    if message.is_empty() || message.chars().count() > 4000 {
        return Err("Enter the exact usage-limit message (maximum 4,000 characters).".into());
    }
    if input.checkpoint_summary.chars().count() > 8000
        || input.remaining_work.chars().count() > 8000
        || input.files_changed.chars().count() > 4000
        || input.last_successful_step.chars().count() > 2000
        || input.next_action.chars().count() > 2000
        || input.blockers.chars().count() > 2000
    {
        return Err("Checkpoint details exceed the supported length.".into());
    }
    let reset_at = nonempty(input.reset_at.as_deref());
    let mut tx = pool.begin_with("BEGIN IMMEDIATE").await.map_err(db_error)?;
    let account: Option<(String, String, Option<String>)> =
        sqlx::query_as("SELECT name,status,archived_at FROM accounts WHERE id=?")
            .bind(input.account_id.trim())
            .fetch_optional(&mut *tx)
            .await
            .map_err(db_error)?;
    let Some((account_name, account_status, archived_at)) = account else {
        return Err("The selected account no longer exists.".into());
    };
    if archived_at.is_some() {
        return Err("This account is archived. Restore it before recording a usage limit.".into());
    }
    if account_status == "waiting" {
        return Err("This account is already waiting. Resolve its current usage-limit event before creating another.".into());
    }
    let unresolved: Option<String> = sqlx::query_scalar(
        "SELECT id FROM usage_limit_events WHERE account_id=? AND status='waiting' LIMIT 1",
    )
    .bind(input.account_id.trim())
    .fetch_optional(&mut *tx)
    .await
    .map_err(db_error)?;
    if unresolved.is_some() {
        return Err("This account already has an unresolved usage-limit event. Verify and resume that event before recording another.".into());
    }
    if let Some(reset) = reset_at {
        let valid: Option<String> = sqlx::query_scalar("SELECT datetime(?)")
            .bind(reset)
            .fetch_one(&mut *tx)
            .await
            .map_err(db_error)?;
        if valid.is_none() {
            return Err(
                "Reset time must be a valid date and time, or leave it blank if unknown.".into(),
            );
        }
    }
    let active_task: Option<(String, String, String, String, String)> = sqlx::query_as(
        "SELECT id,title,instructions,blocker,status FROM tasks WHERE account_id=? AND archived_at IS NULL AND status IN ('ready','in_progress') ORDER BY CASE status WHEN 'in_progress' THEN 0 ELSE 1 END,created_at LIMIT 1",
    )
    .bind(input.account_id.trim())
    .fetch_optional(&mut *tx)
    .await
    .map_err(db_error)?;

    let checkpoint_id = if let Some((
        task_id,
        task_title,
        task_instructions,
        task_blocker,
        task_status,
    )) = active_task.as_ref()
    {
        let id = Uuid::new_v4().to_string();
        let summary = if input.checkpoint_summary.trim().is_empty() {
            format!("Usage limit reached while assigned to: {task_title}")
        } else {
            input.checkpoint_summary.trim().to_owned()
        };
        let remaining = if input.remaining_work.trim().is_empty() {
            task_instructions.as_str()
        } else {
            input.remaining_work.trim()
        };
        let blockers = if input.blockers.trim().is_empty() {
            task_blocker.as_str()
        } else {
            input.blockers.trim()
        };
        sqlx::query("INSERT INTO task_checkpoints(id,task_id,account_id,summary,remaining_work,files_changed,last_successful_step,next_action,blockers) VALUES(?,?,?,?,?,?,?,?,?)")
            .bind(&id).bind(task_id).bind(input.account_id.trim()).bind(summary).bind(remaining).bind(input.files_changed.trim()).bind(input.last_successful_step.trim()).bind(input.next_action.trim()).bind(blockers)
            .execute(&mut *tx).await.map_err(db_error)?;
        sqlx::query("UPDATE tasks SET status='waiting',updated_at=datetime('now') WHERE id=?")
            .bind(task_id)
            .execute(&mut *tx)
            .await
            .map_err(db_error)?;
        insert_task_history(
            &mut tx,
            task_id,
            "status",
            Some(task_status),
            Some("waiting"),
        )
        .await?;
        Some(id)
    } else {
        None
    };

    let event_id = Uuid::new_v4().to_string();
    sqlx::query("INSERT INTO usage_limit_events(id,account_id,task_id,checkpoint_id,exact_message,reset_at,reset_status) VALUES(?,?,?,?,?,?,?)")
        .bind(&event_id)
        .bind(input.account_id.trim())
        .bind(active_task.as_ref().map(|task| task.0.as_str()))
        .bind(checkpoint_id.as_deref())
        .bind(message)
        .bind(reset_at)
        .bind(if reset_at.is_some() { "confirmed" } else { "unknown" })
        .execute(&mut *tx).await.map_err(db_error)?;
    sqlx::query("UPDATE accounts SET status='waiting',updated_at=datetime('now') WHERE id=?")
        .bind(input.account_id.trim())
        .execute(&mut *tx)
        .await
        .map_err(db_error)?;
    let reminder_title = if reset_at.is_some() {
        format!("Check {account_name} usage reset")
    } else {
        format!("Check {account_name} usage availability")
    };
    let reminder_body = if reset_at.is_some() {
        format!("Recorded the reported reset time. Verify availability before resuming. Message: {message}")
    } else {
        format!("Reset time was not provided. Check availability manually; no reset time has been inferred. Message: {message}")
    };
    sqlx::query("INSERT INTO reminders(id,title,body,kind,due_at,account_id,task_id) VALUES(?,?,?,'one_time',COALESCE(datetime(?),datetime('now','+1 day')),?,?)")
        .bind(Uuid::new_v4().to_string()).bind(reminder_title).bind(reminder_body).bind(reset_at).bind(input.account_id.trim()).bind(active_task.as_ref().map(|task| task.0.as_str()))
        .execute(&mut *tx).await.map_err(db_error)?;
    let task_id = active_task.as_ref().map(|task| task.0.as_str());
    insert_activity(
        &mut tx,
        "usage_limit_recorded",
        "Recorded a user-reported usage limit; task and account moved to waiting",
        Some(input.account_id.trim()),
        None,
        task_id,
    )
    .await?;
    tx.commit().await.map_err(db_error)
}

async fn resume_usage_limited_transaction(
    pool: &SqlitePool,
    input: &ResumeUsageInput,
) -> Result<(), String> {
    if !input.reset_verified || !input.user_authorized {
        return Err("Confirm that account availability has been verified and that you authorize resuming this same account.".into());
    }
    let mut tx = pool.begin_with("BEGIN IMMEDIATE").await.map_err(db_error)?;
    let account: Option<(String, String, String, String, Option<String>, Option<String>)> = sqlx::query_as(
        "SELECT a.name,a.status,b.availability_status,bp.availability_status,bp.archived_at,a.archived_at FROM accounts a JOIN browser_profiles bp ON bp.id=a.browser_profile_id JOIN browsers b ON b.id=bp.browser_id WHERE a.id=?",
    )
    .bind(input.account_id.trim())
    .fetch_optional(&mut *tx)
    .await
    .map_err(db_error)?;
    let Some((
        account_name,
        account_status,
        browser_status,
        profile_status,
        profile_archived,
        account_archived,
    )) = account
    else {
        return Err("The selected account or browser profile is unavailable.".into());
    };
    if account_status != "waiting"
        || profile_archived.is_some()
        || account_archived.is_some()
        || browser_status != "available"
        || profile_status != "available"
    {
        return Err("The account must be waiting with an active profile and available browser before resuming.".into());
    }
    let event: Option<(String, Option<String>, Option<String>)> = sqlx::query_as(
        "SELECT id,task_id,checkpoint_id FROM usage_limit_events WHERE account_id=? AND status='waiting' ORDER BY detected_at DESC LIMIT 1",
    )
    .bind(input.account_id.trim())
    .fetch_optional(&mut *tx)
    .await
    .map_err(db_error)?;
    let Some((event_id, task_id, _checkpoint_id)) = event else {
        return Err("No unresolved usage-limit event exists for this account.".into());
    };
    if let Some(task_id) = task_id.as_deref() {
        let old_status: Option<String> = sqlx::query_scalar(
            "SELECT status FROM tasks WHERE id=? AND account_id=? AND archived_at IS NULL",
        )
        .bind(task_id)
        .bind(input.account_id.trim())
        .fetch_optional(&mut *tx)
        .await
        .map_err(db_error)?;
        if old_status.as_deref() != Some("waiting") {
            return Err("The original task is no longer waiting under this account. Review its assignment before resuming.".into());
        }
        sqlx::query("UPDATE tasks SET status='ready',updated_at=datetime('now') WHERE id=?")
            .bind(task_id)
            .execute(&mut *tx)
            .await
            .map_err(db_error)?;
        insert_task_history(&mut tx, task_id, "status", Some("waiting"), Some("ready")).await?;
    }
    sqlx::query("UPDATE usage_limit_events SET status='resolved' WHERE id=? AND status='waiting'")
        .bind(&event_id)
        .execute(&mut *tx)
        .await
        .map_err(db_error)?;
    sqlx::query("UPDATE reminders SET status='completed',completed_at=datetime('now') WHERE account_id=? AND status IN ('pending','snoozed') AND title IN (?,?)")
        .bind(input.account_id.trim()).bind(format!("Check {account_name} usage reset")).bind(format!("Check {account_name} usage availability"))
        .execute(&mut *tx)
        .await
        .map_err(db_error)?;
    sqlx::query("UPDATE accounts SET status='idle',updated_at=datetime('now') WHERE id=?")
        .bind(input.account_id.trim())
        .execute(&mut *tx)
        .await
        .map_err(db_error)?;
    insert_activity(
        &mut tx,
        "usage_limit_resumed",
        &format!(
            "User verified availability and authorized resuming {account_name} on the same account"
        ),
        Some(input.account_id.trim()),
        None,
        task_id.as_deref(),
    )
    .await?;
    tx.commit().await.map_err(db_error)
}

#[tauri::command]
pub async fn save_project(
    app: AppHandle,
    project: ProjectInput,
    is_update: bool,
) -> Result<(), String> {
    let pool = database(&app).await?;
    save_project_transaction(&pool, &project, is_update).await
}

#[tauri::command]
pub async fn archive_project(app: AppHandle, project_id: String) -> Result<(), String> {
    let pool = database(&app).await?;
    let mut tx = pool.begin().await.map_err(db_error)?;
    let result = sqlx::query("UPDATE projects SET archived_at=datetime('now'),updated_at=datetime('now') WHERE id=? AND archived_at IS NULL")
        .bind(&project_id).execute(&mut *tx).await.map_err(db_error)?;
    if result.rows_affected() == 0 {
        return Err("That project no longer exists or is already archived.".into());
    }
    insert_activity(
        &mut tx,
        "project_archived",
        "Archived project",
        None,
        Some(&project_id),
        None,
    )
    .await?;
    tx.commit().await.map_err(db_error)
}

async fn save_project_transaction(
    pool: &SqlitePool,
    p: &ProjectInput,
    is_update: bool,
) -> Result<(), String> {
    let name = p.name.trim();
    if name.is_empty() || name.chars().count() > 120 {
        return Err("Project name must contain 1–120 characters.".into());
    }
    if p.description.chars().count() > 4000 || p.notes.chars().count() > 8000 {
        return Err("Project description or notes exceed the supported length.".into());
    }
    if !(1..=4).contains(&p.priority) {
        return Err("Priority must be between 1 and 4.".into());
    }
    if !matches!(p.status.as_str(), "active" | "paused" | "completed") {
        return Err("Choose a valid project status.".into());
    }
    validate_date(p.start_date.as_deref())?;
    validate_date(p.deadline.as_deref())?;
    let mut tx = pool.begin_with("BEGIN IMMEDIATE").await.map_err(db_error)?;
    let exists: Option<i64> =
        sqlx::query_scalar("SELECT 1 FROM projects WHERE id=? AND archived_at IS NULL")
            .bind(&p.id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(db_error)?;
    if is_update && exists.is_none() {
        return Err("That project no longer exists or is archived.".into());
    }
    if !is_update && exists.is_some() {
        return Err("A project with that ID already exists.".into());
    }
    if is_update {
        sqlx::query("UPDATE projects SET name=?,description=?,status=?,priority=?,start_date=?,deadline=?,notes=?,updated_at=datetime('now') WHERE id=?")
            .bind(name).bind(&p.description).bind(&p.status).bind(p.priority).bind(&p.start_date).bind(&p.deadline).bind(&p.notes).bind(&p.id).execute(&mut *tx).await.map_err(db_error)?;
    } else {
        sqlx::query("INSERT INTO projects(id,name,description,status,priority,start_date,deadline,notes) VALUES(?,?,?,?,?,?,?,?)")
            .bind(&p.id).bind(name).bind(&p.description).bind(&p.status).bind(p.priority).bind(&p.start_date).bind(&p.deadline).bind(&p.notes).execute(&mut *tx).await.map_err(db_error)?;
    }
    let mut requested_accounts = p
        .account_ids
        .iter()
        .map(|id| id.trim())
        .filter(|id| !id.is_empty())
        .collect::<Vec<_>>();
    requested_accounts.sort_unstable();
    requested_accounts.dedup();
    for account_id in &requested_accounts {
        let active: Option<i64> =
            sqlx::query_scalar("SELECT 1 FROM accounts WHERE id=? AND archived_at IS NULL")
                .bind(account_id)
                .fetch_optional(&mut *tx)
                .await
                .map_err(db_error)?;
        if active.is_none() {
            return Err("A selected account no longer exists or is archived. Refresh the account list and try again.".into());
        }
    }
    let existing_accounts: Vec<String> = sqlx::query_scalar(
        "SELECT account_id FROM project_accounts WHERE project_id=? ORDER BY account_id",
    )
    .bind(&p.id)
    .fetch_all(&mut *tx)
    .await
    .map_err(db_error)?;
    for account_id in &existing_accounts {
        if !requested_accounts.contains(&account_id.as_str()) {
            sqlx::query("DELETE FROM project_accounts WHERE project_id=? AND account_id=?")
                .bind(&p.id)
                .bind(account_id)
                .execute(&mut *tx)
                .await
                .map_err(db_error)?;
            insert_activity(
                &mut tx,
                "account_removed_from_project",
                &format!("Removed account from project {name}"),
                Some(account_id),
                Some(&p.id),
                None,
            )
            .await?;
        }
    }
    for account_id in &requested_accounts {
        if !existing_accounts
            .iter()
            .any(|existing| existing == account_id)
        {
            sqlx::query("INSERT INTO project_accounts(project_id,account_id) VALUES(?,?)")
                .bind(&p.id)
                .bind(account_id)
                .execute(&mut *tx)
                .await
                .map_err(db_error)?;
            insert_activity(
                &mut tx,
                "account_added_to_project",
                &format!("Added account to project {name}"),
                Some(account_id),
                Some(&p.id),
                None,
            )
            .await?;
        }
    }
    insert_activity(
        &mut tx,
        if is_update {
            "project_updated"
        } else {
            "project_created"
        },
        &format!(
            "{} project {name}",
            if is_update { "Updated" } else { "Created" }
        ),
        None,
        Some(&p.id),
        None,
    )
    .await?;
    tx.commit().await.map_err(db_error)
}

#[tauri::command]
pub async fn save_task(app: AppHandle, task: TaskInput, is_update: bool) -> Result<(), String> {
    let pool = database(&app).await?;
    save_task_transaction(&pool, &task, is_update).await
}

#[cfg(test)]
pub async fn create_ai_task_in_pool(
    pool: &SqlitePool,
    id: String,
    project_id: String,
    title: String,
    description: String,
    priority: i64,
) -> Result<String, String> {
    let mut tx = pool.begin_with("BEGIN IMMEDIATE").await.map_err(db_error)?;
    let id = create_ai_task_in_transaction(&mut tx, id, project_id, title, description, priority)
        .await?;
    tx.commit().await.map_err(db_error)?;
    Ok(id)
}

pub async fn create_ai_task_in_transaction(
    tx: &mut Transaction<'_, Sqlite>,
    id: String,
    project_id: String,
    title: String,
    description: String,
    priority: i64,
) -> Result<String, String> {
    let existing: Option<(String, String, String, i64)> = sqlx::query_as(
        "SELECT project_id,title,description,priority FROM tasks WHERE id=? AND archived_at IS NULL",
    )
    .bind(&id)
    .fetch_optional(&mut **tx)
    .await
    .map_err(db_error)?;
    if let Some((existing_project, existing_title, existing_description, existing_priority)) =
        existing
    {
        if existing_project == project_id
            && existing_title == title.trim()
            && existing_description == description.trim()
            && existing_priority == priority
        {
            return Ok(id);
        }
        return Err("The approved task action ID already belongs to a different task.".into());
    }
    let task = TaskInput {
        id: id.clone(),
        project_id: Some(project_id.clone()),
        account_id: None,
        title: title.clone(),
        description: description.clone(),
        priority,
        status: "backlog".into(),
        deadline: None,
        expected_minutes: None,
        instructions: String::new(),
        completion_notes: String::new(),
        blocker: String::new(),
        depends_on: Vec::new(),
        intended_files: String::new(),
        modified_files: String::new(),
        released_files: String::new(),
    };
    save_task_transaction_in_tx(tx, &task, false, false).await?;
    Ok(id)
}

#[tauri::command]
pub async fn archive_task(app: AppHandle, task_id: String) -> Result<(), String> {
    let pool = database(&app).await?;
    let mut tx = pool.begin().await.map_err(db_error)?;
    let row: Option<(Option<String>, Option<String>)> = sqlx::query_as(
        "SELECT project_id,account_id FROM tasks WHERE id=? AND archived_at IS NULL",
    )
    .bind(&task_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(db_error)?;
    let Some((project_id, account_id)) = row else {
        return Err("That task no longer exists or is already archived.".into());
    };
    release_claims(&mut tx, &task_id).await?;
    sqlx::query(
        "UPDATE tasks SET archived_at=datetime('now'),updated_at=datetime('now') WHERE id=?",
    )
    .bind(&task_id)
    .execute(&mut *tx)
    .await
    .map_err(db_error)?;
    insert_task_history(&mut tx, &task_id, "archived_at", None, Some("archived")).await?;
    insert_activity(
        &mut tx,
        "task_archived",
        "Archived task",
        account_id.as_deref(),
        project_id.as_deref(),
        Some(&task_id),
    )
    .await?;
    tx.commit().await.map_err(db_error)
}

async fn save_task_transaction(
    pool: &SqlitePool,
    t: &TaskInput,
    is_update: bool,
) -> Result<(), String> {
    save_task_transaction_with_guard(pool, t, is_update, false).await
}

async fn save_task_transaction_with_guard(
    pool: &SqlitePool,
    task: &TaskInput,
    is_update: bool,
    require_unassigned: bool,
) -> Result<(), String> {
    let mut tx = pool.begin_with("BEGIN IMMEDIATE").await.map_err(db_error)?;
    save_task_transaction_in_tx(&mut tx, task, is_update, require_unassigned).await?;
    tx.commit().await.map_err(db_error)
}

async fn save_task_transaction_in_tx(
    tx: &mut Transaction<'_, Sqlite>,
    t: &TaskInput,
    is_update: bool,
    require_unassigned: bool,
) -> Result<(), String> {
    let title = t.title.trim();
    if title.is_empty() || title.chars().count() > 180 {
        return Err("Task title must contain 1–180 characters.".into());
    }
    if t.description.chars().count() > 8000
        || t.instructions.chars().count() > 8000
        || t.completion_notes.chars().count() > 8000
        || t.blocker.chars().count() > 2000
    {
        return Err("Task details exceed the supported length.".into());
    }
    if !(1..=4).contains(&t.priority) {
        return Err("Priority must be between 1 and 4.".into());
    }
    if !matches!(
        t.status.as_str(),
        "backlog" | "ready" | "in_progress" | "waiting" | "review" | "blocked" | "completed"
    ) {
        return Err("Choose a valid task status.".into());
    }
    if t.expected_minutes.is_some_and(|v| v < 0) {
        return Err("Expected minutes cannot be negative.".into());
    }
    validate_date(t.deadline.as_deref())?;
    let project_id = nonempty(t.project_id.as_deref());
    let account_id = nonempty(t.account_id.as_deref());
    // Serialize assignment eligibility checks and project file claims with the write so two concurrent
    // saves cannot both claim the same available worker.
    let old: Option<TaskSnapshot> = if is_update {
        sqlx::query_as("SELECT project_id,account_id,title,description,priority,status,deadline,expected_minutes,instructions,completion_notes,blocker FROM tasks WHERE id=? AND archived_at IS NULL")
            .bind(&t.id).fetch_optional(&mut **tx).await.map_err(db_error)?
    } else {
        None
    };
    if is_update && old.is_none() {
        return Err("That task no longer exists or is archived.".into());
    }
    if require_unassigned {
        let Some(current) = old.as_ref() else {
            return Err("That task no longer exists or is archived.".into());
        };
        if current.account_id.is_some()
            || !matches!(current.status.as_str(), "ready" | "backlog")
            || current.project_id.as_deref() != project_id
        {
            return Err("This task was changed or assigned by another action. Refresh the queue before assigning it.".into());
        }
    }
    validate_dependencies(tx, &t.id, project_id, &t.depends_on).await?;
    if !is_update
        && sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM tasks WHERE id=?")
            .bind(&t.id)
            .fetch_one(&mut **tx)
            .await
            .map_err(db_error)?
            > 0
    {
        return Err("A task with that ID already exists.".into());
    }
    if let Some(pid) = project_id {
        let active: Option<i64> = sqlx::query_scalar(
            "SELECT 1 FROM projects WHERE id=? AND archived_at IS NULL AND status='active'",
        )
        .bind(pid)
        .fetch_optional(&mut **tx)
        .await
        .map_err(db_error)?;
        if active.is_none() {
            return Err("Choose an active project, or clear the project field.".into());
        }
    }
    if let Some(aid) = account_id {
        if project_id.is_none() {
            return Err("Choose an active project before assigning this task to a worker.".into());
        }
        let account: Option<(String, String, String, Option<String>)> = sqlx::query_as(
            "SELECT a.status,b.availability_status,bp.availability_status,a.archived_at
             FROM accounts a JOIN browser_profiles bp ON bp.id=a.browser_profile_id
             JOIN browsers b ON b.id=bp.browser_id WHERE a.id=? AND bp.archived_at IS NULL",
        )
        .bind(aid)
        .fetch_optional(&mut **tx)
        .await
        .map_err(db_error)?;
        let Some((account_status, browser_status, profile_status, archived_at)) = account else {
            return Err("This account or its browser profile is unavailable. Choose an active registered account and profile.".into());
        };
        if archived_at.is_some() {
            return Err("This account is archived. Restore it or choose another account.".into());
        }
        let assignment_changed = old
            .as_ref()
            .is_none_or(|previous| previous.account_id.as_deref() != Some(aid));
        let entering_assigned_work = old.as_ref().is_some_and(|previous| {
            previous.account_id.as_deref() == Some(aid)
                && matches!(t.status.as_str(), "ready" | "in_progress")
                && matches!(previous.status.as_str(), "backlog" | "completed")
        });
        if assignment_changed || entering_assigned_work {
            if account_status != "idle" {
                return Err(format!("Account is {} and cannot receive a new task. Set it to Idle after resolving its current work, or choose another account.", account_status.replace('_', " ")));
            }
            if browser_status != "available" {
                return Err(format!("The account’s browser is {}. Restore browser availability before assigning this task.", browser_status.replace('_', " ")));
            }
            if profile_status != "available" {
                return Err(format!("The account’s browser profile is {}. Rescan or restore the profile before assigning this task.", profile_status.replace('_', " ")));
            }
            if matches!(t.status.as_str(), "ready" | "in_progress") {
                let conflict: Option<String> = sqlx::query_scalar(
                    "SELECT title FROM tasks WHERE account_id=? AND id<>? AND archived_at IS NULL AND status IN ('ready','in_progress','waiting','review','blocked') LIMIT 1",
                )
                .bind(aid)
                .bind(&t.id)
                .fetch_optional(&mut **tx)
                .await
                .map_err(db_error)?;
                if let Some(title) = conflict {
                    return Err(format!("This account already has active task “{title}”. Complete or reassign that task before assigning another active task."));
                }
            }
        }
        if let Some(pid) = project_id {
            let linked: Option<i64> = sqlx::query_scalar(
                "SELECT 1 FROM project_accounts WHERE project_id=? AND account_id=?",
            )
            .bind(pid)
            .bind(aid)
            .fetch_optional(&mut **tx)
            .await
            .map_err(db_error)?;
            if linked.is_none() {
                return Err(
                    "Assign this account to the project in Accounts before assigning its task."
                        .into(),
                );
            }
        }
    }
    let mut declared_files = normalize_file_declarations(&t.intended_files, &t.modified_files)?;
    let released_files = normalize_file_declarations(&t.released_files, "")?;
    for (key, display, _) in &released_files {
        let declared: Option<i64> = sqlx::query_scalar(
            "SELECT 1 FROM task_file_declarations WHERE task_id=? AND path_key=? LIMIT 1",
        )
        .bind(&t.id)
        .bind(key)
        .fetch_optional(&mut **tx)
        .await
        .map_err(db_error)?;
        if declared.is_none()
            && !declared_files
                .iter()
                .any(|(existing, _, _)| existing == key)
        {
            return Err(format!(
                "Cannot release '{display}' because this task does not declare or claim that path."
            ));
        }
        declared_files.retain(|(declared_key, _, _)| declared_key != key);
    }
    if account_id.is_some() || t.status == "completed" {
        validate_dependencies_complete(tx, &t.id).await?;
        if account_id.is_some()
            && matches!(
                t.status.as_str(),
                "ready" | "in_progress" | "waiting" | "review" | "blocked"
            )
        {
            if let Some(pid) = project_id {
                validate_file_claims(tx, &t.id, pid, &declared_files).await?;
            }
        }
    }
    if t.status == "completed" && !is_update { /* completion is an explicit user state, recorded below */
    }
    sqlx::query("INSERT INTO tasks(id,project_id,account_id,title,description,priority,status,deadline,expected_minutes,instructions,completion_notes,blocker,completed_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,CASE WHEN ?='completed' THEN datetime('now') ELSE NULL END) ON CONFLICT(id) DO UPDATE SET project_id=excluded.project_id,account_id=excluded.account_id,title=excluded.title,description=excluded.description,priority=excluded.priority,status=excluded.status,deadline=excluded.deadline,expected_minutes=excluded.expected_minutes,instructions=excluded.instructions,completion_notes=excluded.completion_notes,blocker=excluded.blocker,completed_at=CASE WHEN excluded.status='completed' THEN COALESCE(tasks.completed_at,datetime('now')) ELSE NULL END,updated_at=datetime('now')")
        .bind(&t.id).bind(project_id).bind(account_id).bind(title).bind(&t.description).bind(t.priority).bind(&t.status).bind(&t.deadline).bind(t.expected_minutes).bind(&t.instructions).bind(&t.completion_notes).bind(&t.blocker).bind(&t.status)
        .execute(&mut **tx).await.map_err(db_error)?;
    save_dependencies(tx, &t.id, &t.depends_on).await?;
    save_file_claims(
        tx,
        &t.id,
        project_id,
        &declared_files,
        account_id.is_some()
            && matches!(
                t.status.as_str(),
                "ready" | "in_progress" | "waiting" | "review" | "blocked"
            ),
    )
    .await?;
    for (key, display, _) in &released_files {
        sqlx::query(
            "INSERT INTO task_file_releases(id,task_id,path_key,path_display) VALUES(?,?,?,?)",
        )
        .bind(Uuid::new_v4().to_string())
        .bind(&t.id)
        .bind(key)
        .bind(display)
        .execute(&mut **tx)
        .await
        .map_err(db_error)?;
        insert_task_history(tx, &t.id, "file_released", None, Some(display)).await?;
    }
    if let Some(old) = old {
        history_if_changed(
            tx,
            &t.id,
            "project_id",
            old.project_id.as_deref(),
            project_id,
        )
        .await?;
        history_if_changed(
            tx,
            &t.id,
            "account_id",
            old.account_id.as_deref(),
            account_id,
        )
        .await?;
        history_if_changed(tx, &t.id, "title", Some(&old.title), Some(title)).await?;
        history_if_changed(
            tx,
            &t.id,
            "description",
            Some(&old.description),
            Some(&t.description),
        )
        .await?;
        history_if_changed(
            tx,
            &t.id,
            "priority",
            Some(&old.priority.to_string()),
            Some(&t.priority.to_string()),
        )
        .await?;
        history_if_changed(tx, &t.id, "status", Some(&old.status), Some(&t.status)).await?;
        history_if_changed(
            tx,
            &t.id,
            "deadline",
            old.deadline.as_deref(),
            t.deadline.as_deref(),
        )
        .await?;
        let old_minutes = old.expected_minutes.map(|v| v.to_string());
        let new_minutes = t.expected_minutes.map(|v| v.to_string());
        history_if_changed(
            tx,
            &t.id,
            "expected_minutes",
            old_minutes.as_deref(),
            new_minutes.as_deref(),
        )
        .await?;
        history_if_changed(
            tx,
            &t.id,
            "instructions",
            Some(&old.instructions),
            Some(&t.instructions),
        )
        .await?;
        history_if_changed(
            tx,
            &t.id,
            "completion_notes",
            Some(&old.completion_notes),
            Some(&t.completion_notes),
        )
        .await?;
        history_if_changed(tx, &t.id, "blocker", Some(&old.blocker), Some(&t.blocker)).await?;
    } else {
        insert_task_history(tx, &t.id, "created", None, Some(title)).await?;
        for (field, value) in [
            ("status", t.status.as_str()),
            ("account_id", account_id.unwrap_or("")),
            ("project_id", project_id.unwrap_or("")),
        ] {
            if !value.is_empty() {
                insert_task_history(tx, &t.id, field, None, Some(value)).await?;
            }
        }
    }
    insert_activity(
        tx,
        if is_update {
            "task_updated"
        } else {
            "task_created"
        },
        &format!(
            "{} task {title}",
            if is_update { "Updated" } else { "Created" }
        ),
        account_id,
        project_id,
        Some(&t.id),
    )
    .await?;
    Ok(())
}

fn normalize_file_declarations(
    intended: &str,
    modified: &str,
) -> Result<Vec<(String, String, String)>, String> {
    let mut output = Vec::new();
    for (kind, source) in [("intended", intended), ("modified", modified)] {
        for raw in source
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
        {
            if raw.chars().count() > 500 || raw.contains('\0') {
                return Err(
                    "File paths must be under 500 characters and cannot contain null characters."
                        .into(),
                );
            }
            let replaced = raw.replace('\\', "/");
            if replaced.starts_with('/') || replaced.starts_with("//") || replaced.contains(':') {
                return Err("File claims must use project-relative paths (for example src/App.tsx), not absolute paths.".into());
            }
            let mut parts = Vec::new();
            for part in replaced.split('/') {
                let without_spaces = part.trim_end_matches(' ');
                if without_spaces.is_empty() || without_spaces == "." {
                    continue;
                }
                if without_spaces == ".." {
                    if parts.pop().is_none() {
                        return Err(
                            "File claim paths cannot go above the project directory.".into()
                        );
                    }
                    continue;
                }
                let component = without_spaces.trim_end_matches('.');
                if component.is_empty() {
                    continue;
                }
                parts.push(component);
            }
            if parts.is_empty() {
                return Err(
                    "A file claim must name a file or directory inside the project.".into(),
                );
            }
            let display = parts.join("/");
            let key = display.to_lowercase(); // Windows project paths are case-insensitive.
            if !output.iter().any(|(existing_key, _, existing_kind)| {
                existing_key == &key && existing_kind == kind
            }) {
                output.push((key, display, kind.to_owned()));
            }
        }
    }
    Ok(output)
}

async fn validate_dependencies(
    tx: &mut Transaction<'_, Sqlite>,
    task_id: &str,
    project_id: Option<&str>,
    dependencies: &[String],
) -> Result<(), String> {
    let mut unique = std::collections::HashSet::new();
    for dependency in dependencies {
        if dependency == task_id {
            return Err("A task cannot depend on itself.".into());
        }
        if !unique.insert(dependency) {
            continue;
        }
        let dependency_project: Option<Option<String>> =
            sqlx::query_scalar("SELECT project_id FROM tasks WHERE id=? AND archived_at IS NULL")
                .bind(dependency)
                .fetch_optional(&mut **tx)
                .await
                .map_err(db_error)?;
        let Some(dependency_project) = dependency_project else {
            return Err("A dependency must reference an existing, active task.".into());
        };
        if dependency_project.as_deref() != project_id {
            return Err("Task dependencies must belong to the same project.".into());
        }
        let reaches_task: Option<String> = sqlx::query_scalar("WITH RECURSIVE chain(id) AS (SELECT depends_on_task_id FROM task_dependencies WHERE task_id=? UNION SELECT d.depends_on_task_id FROM task_dependencies d JOIN chain c ON d.task_id=c.id) SELECT id FROM chain WHERE id=? LIMIT 1")
            .bind(dependency).bind(task_id).fetch_optional(&mut **tx).await.map_err(db_error)?;
        if reaches_task.is_some() {
            return Err("This dependency would create a circular task dependency.".into());
        }
    }
    Ok(())
}

async fn validate_dependencies_complete(
    tx: &mut Transaction<'_, Sqlite>,
    task_id: &str,
) -> Result<(), String> {
    let incomplete: Vec<String> = sqlx::query_scalar("WITH RECURSIVE ancestors(id) AS (SELECT depends_on_task_id FROM task_dependencies WHERE task_id=? UNION SELECT d.depends_on_task_id FROM task_dependencies d JOIN ancestors a ON d.task_id=a.id) SELECT t.title FROM ancestors a JOIN tasks t ON t.id=a.id WHERE t.status<>'completed' OR t.archived_at IS NOT NULL ORDER BY t.title")
        .bind(task_id).fetch_all(&mut **tx).await.map_err(db_error)?;
    if !incomplete.is_empty() {
        return Err(format!(
            "Task is blocked by incomplete dependencies: {}.",
            incomplete.join(", ")
        ));
    }
    Ok(())
}

async fn save_dependencies(
    tx: &mut Transaction<'_, Sqlite>,
    task_id: &str,
    dependencies: &[String],
) -> Result<(), String> {
    let old: Vec<String> = sqlx::query_scalar("SELECT depends_on_task_id FROM task_dependencies WHERE task_id=? ORDER BY depends_on_task_id").bind(task_id).fetch_all(&mut **tx).await.map_err(db_error)?;
    let mut new = dependencies.to_vec();
    new.sort();
    new.dedup();
    if old != new {
        sqlx::query("DELETE FROM task_dependencies WHERE task_id=?")
            .bind(task_id)
            .execute(&mut **tx)
            .await
            .map_err(db_error)?;
        for dep in &new {
            sqlx::query("INSERT INTO task_dependencies(task_id,depends_on_task_id) VALUES(?,?)")
                .bind(task_id)
                .bind(dep)
                .execute(&mut **tx)
                .await
                .map_err(db_error)?;
        }
        history_if_changed(
            tx,
            task_id,
            "dependencies",
            Some(&old.join(",")),
            Some(&new.join(",")),
        )
        .await?;
    }
    Ok(())
}

fn paths_overlap(a: &str, b: &str) -> bool {
    a == b
        || a.strip_prefix(b).is_some_and(|rest| rest.starts_with('/'))
        || b.strip_prefix(a).is_some_and(|rest| rest.starts_with('/'))
}

async fn validate_file_claims(
    tx: &mut Transaction<'_, Sqlite>,
    task_id: &str,
    project_id: &str,
    declared: &[(String, String, String)],
) -> Result<(), String> {
    let active: Vec<(String, String)> = sqlx::query_as("SELECT path_key,task_id FROM task_file_claims WHERE project_id=? AND status='active' AND task_id<>?")
        .bind(project_id).bind(task_id).fetch_all(&mut **tx).await.map_err(db_error)?;
    for (candidate, display, _) in declared {
        if let Some((_, owner)) = active
            .iter()
            .find(|(existing, _)| paths_overlap(candidate, existing))
        {
            let owner_title: String = sqlx::query_scalar("SELECT title FROM tasks WHERE id=?")
                .bind(owner)
                .fetch_one(&mut **tx)
                .await
                .map_err(db_error)?;
            return Err(format!("File claim '{display}' overlaps an active claim owned by task '{owner_title}'. Release or complete that task first."));
        }
    }
    Ok(())
}

async fn save_file_claims(
    tx: &mut Transaction<'_, Sqlite>,
    task_id: &str,
    project_id: Option<&str>,
    declared: &[(String, String, String)],
    active: bool,
) -> Result<(), String> {
    if !declared.is_empty() && project_id.is_none() {
        return Err("Set a project before declaring file ownership.".into());
    }
    let project_id = project_id.unwrap_or_default();
    let old_declarations: Vec<(String, String)> = sqlx::query_as(
        "SELECT path_display,claim_kind FROM task_file_declarations WHERE task_id=? ORDER BY claim_kind,path_key",
    )
    .bind(task_id)
    .fetch_all(&mut **tx)
    .await
    .map_err(db_error)?;
    for kind in ["intended", "modified"] {
        let old: Vec<&str> = old_declarations
            .iter()
            .filter(|(_, old_kind)| old_kind == kind)
            .map(|(path, _)| path.as_str())
            .collect();
        let new: Vec<&str> = declared
            .iter()
            .filter(|(_, _, new_kind)| new_kind == kind)
            .map(|(_, path, _)| path.as_str())
            .collect();
        let old_value = (!old.is_empty()).then(|| old.join("\n"));
        let new_value = (!new.is_empty()).then(|| new.join("\n"));
        history_if_changed(
            tx,
            task_id,
            if kind == "intended" {
                "intended_files"
            } else {
                "modified_files"
            },
            old_value.as_deref(),
            new_value.as_deref(),
        )
        .await?;
    }
    if !active {
        release_claims(tx, task_id).await?;
    }
    sqlx::query("DELETE FROM task_file_declarations WHERE task_id=?")
        .bind(task_id)
        .execute(&mut **tx)
        .await
        .map_err(db_error)?;
    for (key, display, kind) in declared {
        sqlx::query("INSERT INTO task_file_declarations(task_id,path_key,path_display,claim_kind) VALUES(?,?,?,?)").bind(task_id).bind(key).bind(display).bind(kind).execute(&mut **tx).await.map_err(db_error)?;
    }
    let existing: Vec<(String,String,String,String)> = sqlx::query_as("SELECT id,path_key,path_display,claim_kind FROM task_file_claims WHERE task_id=? AND status='active'")
        .bind(task_id).fetch_all(&mut **tx).await.map_err(db_error)?;
    for (id, key, _, kind) in &existing {
        if !declared
            .iter()
            .any(|(new_key, _, new_kind)| new_key == key && new_kind == kind)
        {
            sqlx::query("UPDATE task_file_claims SET status='released',released_at=datetime('now') WHERE id=? AND status='active'").bind(id).execute(&mut **tx).await.map_err(db_error)?;
            insert_task_history(
                tx,
                task_id,
                "file_claim_released",
                Some(&format!("{kind}:{key}")),
                None,
            )
            .await?;
        }
    }
    for (key, display, kind) in declared {
        let prior: Option<String> = sqlx::query_scalar("SELECT id FROM task_file_claims WHERE task_id=? AND path_key=? AND claim_kind=? AND status='active' ORDER BY claimed_at DESC LIMIT 1")
            .bind(task_id).bind(key).bind(kind).fetch_optional(&mut **tx).await.map_err(db_error)?;
        if active {
            if let Some(id) = prior {
                sqlx::query("UPDATE task_file_claims SET project_id=?,path_display=?,status='active',released_at=NULL WHERE id=?").bind(project_id).bind(display).bind(id).execute(&mut **tx).await.map_err(db_error)?;
            } else {
                sqlx::query("INSERT INTO task_file_claims(id,project_id,task_id,path_key,path_display,claim_kind,status) VALUES(?,?,?,?,?,?, 'active')").bind(Uuid::new_v4().to_string()).bind(project_id).bind(task_id).bind(key).bind(display).bind(kind).execute(&mut **tx).await.map_err(db_error)?;
                insert_task_history(
                    tx,
                    task_id,
                    "file_claim_acquired",
                    None,
                    Some(&format!("{kind}:{display}")),
                )
                .await?;
            }
        } else if prior.is_none() {
            sqlx::query("INSERT INTO task_file_claims(id,project_id,task_id,path_key,path_display,claim_kind,status,released_at) VALUES(?,?,?,?,?,?, 'released',datetime('now'))")
                .bind(Uuid::new_v4().to_string()).bind(project_id).bind(task_id).bind(key).bind(display).bind(kind).execute(&mut **tx).await.map_err(db_error)?;
        }
    }
    Ok(())
}

pub(crate) async fn release_claims(
    tx: &mut Transaction<'_, Sqlite>,
    task_id: &str,
) -> Result<(), String> {
    let claims: Vec<(String, String)> = sqlx::query_as(
        "SELECT path_key,claim_kind FROM task_file_claims WHERE task_id=? AND status='active'",
    )
    .bind(task_id)
    .fetch_all(&mut **tx)
    .await
    .map_err(db_error)?;
    sqlx::query("UPDATE task_file_claims SET status='released',released_at=datetime('now') WHERE task_id=? AND status='active'").bind(task_id).execute(&mut **tx).await.map_err(db_error)?;
    for (path, kind) in claims {
        insert_task_history(
            tx,
            task_id,
            "file_claim_released",
            Some(&format!("{kind}:{path}")),
            None,
        )
        .await?;
    }
    Ok(())
}

async fn assign_task_transaction(
    pool: &SqlitePool,
    task_id: &str,
    account_id: &str,
) -> Result<QueueAssignmentResult, String> {
    let row: Option<TaskInputRow> = sqlx::query_as("SELECT id,project_id,title,description,priority,status,deadline,expected_minutes,instructions,completion_notes,blocker FROM tasks WHERE id=? AND archived_at IS NULL")
        .bind(task_id).fetch_optional(pool).await.map_err(db_error)?;
    let Some(row) = row else {
        return Err("That task no longer exists or is archived.".into());
    };
    if !matches!(row.status.as_str(), "ready" | "backlog") {
        return Err("Only Ready or Backlog tasks can be newly assigned.".into());
    }
    let mut task = TaskInput {
        id: row.id,
        project_id: row.project_id,
        account_id: Some(account_id.into()),
        title: row.title,
        description: row.description,
        priority: row.priority,
        status: "ready".into(),
        deadline: row.deadline,
        expected_minutes: row.expected_minutes,
        instructions: row.instructions,
        completion_notes: row.completion_notes,
        blocker: row.blocker,
        depends_on: Vec::new(),
        intended_files: String::new(),
        modified_files: String::new(),
        released_files: String::new(),
    };
    task.depends_on = sqlx::query_scalar("SELECT depends_on_task_id FROM task_dependencies WHERE task_id=? ORDER BY depends_on_task_id").bind(task_id).fetch_all(pool).await.map_err(db_error)?;
    let files: Vec<(String,String)> = sqlx::query_as("SELECT path_display,claim_kind FROM task_file_declarations WHERE task_id=? ORDER BY path_display").bind(task_id).fetch_all(pool).await.map_err(db_error)?;
    for (path, kind) in files {
        if kind == "intended" {
            task.intended_files.push_str(&path);
            task.intended_files.push('\n');
        } else {
            task.modified_files.push_str(&path);
            task.modified_files.push('\n');
        }
    }
    save_task_transaction_with_guard(pool, &task, true, true).await?;
    let account_name: String = sqlx::query_scalar("SELECT name FROM accounts WHERE id=?")
        .bind(account_id)
        .fetch_one(pool)
        .await
        .map_err(db_error)?;
    Ok(QueueAssignmentResult {
        task_id: task.id,
        task_title: task.title,
        account_id: account_id.into(),
        account_name,
    })
}

#[derive(sqlx::FromRow)]
struct TaskInputRow {
    id: String,
    project_id: Option<String>,
    title: String,
    description: String,
    priority: i64,
    status: String,
    deadline: Option<String>,
    expected_minutes: Option<i64>,
    instructions: String,
    completion_notes: String,
    blocker: String,
}

async fn assign_next_eligible_transaction(
    pool: &SqlitePool,
) -> Result<Option<QueueAssignmentResult>, String> {
    let candidates: Vec<String> = sqlx::query_scalar("SELECT t.id FROM tasks t JOIN projects p ON p.id=t.project_id WHERE t.archived_at IS NULL AND p.archived_at IS NULL AND p.status='active' AND t.status='ready' AND NOT EXISTS(WITH RECURSIVE ancestors(id) AS (SELECT depends_on_task_id FROM task_dependencies WHERE task_id=t.id UNION SELECT d.depends_on_task_id FROM task_dependencies d JOIN ancestors a ON d.task_id=a.id) SELECT 1 FROM ancestors a JOIN tasks prerequisite ON prerequisite.id=a.id WHERE prerequisite.status<>'completed' OR prerequisite.archived_at IS NOT NULL) ORDER BY t.priority DESC,COALESCE(t.deadline,'9999-12-31'),t.created_at")
        .fetch_all(pool).await.map_err(db_error)?;
    let workers: Vec<String> = sqlx::query_scalar("SELECT a.id FROM accounts a JOIN browser_profiles bp ON bp.id=a.browser_profile_id JOIN browsers b ON b.id=bp.browser_id WHERE a.archived_at IS NULL AND a.status='idle' AND bp.archived_at IS NULL AND bp.availability_status='available' AND b.availability_status='available' AND NOT EXISTS(SELECT 1 FROM tasks active WHERE active.account_id=a.id AND active.archived_at IS NULL AND active.status IN ('ready','in_progress','waiting','review','blocked')) ORDER BY a.priority DESC,a.name COLLATE NOCASE")
        .fetch_all(pool).await.map_err(db_error)?;
    for task_id in candidates {
        let project: String = sqlx::query_scalar("SELECT project_id FROM tasks WHERE id=?")
            .bind(&task_id)
            .fetch_one(pool)
            .await
            .map_err(db_error)?;
        let mut linked_workers = 0;
        for account_id in &workers {
            let linked: Option<i64> = sqlx::query_scalar(
                "SELECT 1 FROM project_accounts WHERE project_id=? AND account_id=?",
            )
            .bind(&project)
            .bind(account_id)
            .fetch_optional(pool)
            .await
            .map_err(db_error)?;
            if linked.is_none() {
                continue;
            }
            linked_workers += 1;
            match assign_task_transaction(pool, &task_id, account_id).await {
                Ok(result) => return Ok(Some(result)),
                Err(reason) => {
                    record_assignment_failure(pool, &task_id, account_id, &reason).await?;
                    continue;
                }
            }
        }
        if linked_workers == 0 {
            record_assignment_failure(
                pool,
                &task_id,
                "",
                "No currently available worker linked to this project can receive the task.",
            )
            .await?;
        }
    }
    Ok(None)
}

async fn release_task_assignment_transaction(
    pool: &SqlitePool,
    task_id: &str,
) -> Result<(), String> {
    let mut tx = pool.begin_with("BEGIN IMMEDIATE").await.map_err(db_error)?;
    let row: Option<(Option<String>, Option<String>, String)> = sqlx::query_as(
        "SELECT project_id,account_id,status FROM tasks WHERE id=? AND archived_at IS NULL",
    )
    .bind(task_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(db_error)?;
    let Some((project, account, status)) = row else {
        return Err("That task no longer exists or is archived.".into());
    };
    if status == "waiting" {
        return Err("A waiting task retains its original worker and file claims. Resolve the reset and explicitly resume or cancel it first.".into());
    }
    sqlx::query(
        "UPDATE tasks SET account_id=NULL,status='ready',updated_at=datetime('now') WHERE id=?",
    )
    .bind(task_id)
    .execute(&mut *tx)
    .await
    .map_err(db_error)?;
    insert_task_history(&mut tx, task_id, "account_id", account.as_deref(), None).await?;
    insert_task_history(&mut tx, task_id, "status", Some(&status), Some("ready")).await?;
    release_claims(&mut tx, task_id).await?;
    insert_activity(
        &mut tx,
        "task_assignment_released",
        "Released task assignment and file claims",
        account.as_deref(),
        project.as_deref(),
        Some(task_id),
    )
    .await?;
    tx.commit().await.map_err(db_error)
}

#[derive(sqlx::FromRow)]
struct TaskSnapshot {
    project_id: Option<String>,
    account_id: Option<String>,
    title: String,
    description: String,
    priority: i64,
    status: String,
    deadline: Option<String>,
    expected_minutes: Option<i64>,
    instructions: String,
    completion_notes: String,
    blocker: String,
}

async fn history_if_changed(
    tx: &mut Transaction<'_, Sqlite>,
    task_id: &str,
    field: &str,
    old: Option<&str>,
    new: Option<&str>,
) -> Result<(), String> {
    if old != new {
        insert_task_history(tx, task_id, field, old, new).await?;
    }
    Ok(())
}

pub(crate) async fn insert_task_history(
    tx: &mut Transaction<'_, Sqlite>,
    task_id: &str,
    field: &str,
    old: Option<&str>,
    new: Option<&str>,
) -> Result<(), String> {
    sqlx::query("INSERT INTO task_history(id,task_id,field,old_value,new_value,source) VALUES(?,?,?,?,?,'manual')")
        .bind(Uuid::new_v4().to_string()).bind(task_id).bind(field).bind(old).bind(new).execute(&mut **tx).await.map_err(db_error)?;
    Ok(())
}

pub(crate) async fn insert_activity(
    tx: &mut Transaction<'_, Sqlite>,
    kind: &str,
    description: &str,
    account_id: Option<&str>,
    project_id: Option<&str>,
    task_id: Option<&str>,
) -> Result<(), String> {
    sqlx::query("INSERT INTO activity_logs(id,activity_type,description,source,account_id,project_id,task_id) VALUES(?,?,?,'manual',?,?,?)")
        .bind(Uuid::new_v4().to_string()).bind(kind).bind(description).bind(account_id).bind(project_id).bind(task_id).execute(&mut **tx).await.map_err(db_error)?;
    Ok(())
}

fn nonempty(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|v| !v.is_empty())
}
fn validate_date(value: Option<&str>) -> Result<(), String> {
    if let Some(value) = nonempty(value) {
        let parts: Vec<_> = value.split('-').collect();
        if parts.len() != 3 || parts[0].len() != 4 || parts[1].len() != 2 || parts[2].len() != 2 {
            return Err("Dates must use YYYY-MM-DD format.".into());
        }
        let (Ok(year), Ok(month), Ok(day)) = (
            parts[0].parse::<i32>(),
            parts[1].parse::<u32>(),
            parts[2].parse::<u32>(),
        ) else {
            return Err("Dates must use YYYY-MM-DD format.".into());
        };
        let leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
        let max_day = match month {
            1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
            4 | 6 | 9 | 11 => 30,
            2 if leap => 29,
            2 => 28,
            _ => 0,
        };
        if day == 0 || day > max_day {
            return Err("Enter a valid calendar date in YYYY-MM-DD format.".into());
        }
    }
    Ok(())
}
fn db_error(error: sqlx::Error) -> String {
    format!("Database operation failed: {error}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
    use std::path::Path;

    async fn pool(path: &Path) -> SqlitePool {
        let pool = SqlitePoolOptions::new()
            .max_connections(4)
            .connect_with(
                SqliteConnectOptions::new()
                    .filename(path)
                    .create_if_missing(true)
                    .foreign_keys(true)
                    .busy_timeout(std::time::Duration::from_secs(5)),
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
        sqlx::query("INSERT INTO browsers(id,name,executable_path,family) VALUES('b','Test','test.exe','chrome')").execute(&pool).await.unwrap();
        sqlx::query("INSERT INTO browser_profiles(id,browser_id,profile_name,profile_path) VALUES('bp','b','Default','C:/test')").execute(&pool).await.unwrap();
        sqlx::query("INSERT INTO accounts(id,name,browser_profile_id) VALUES('a','Worker','bp')")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::raw_sql(include_str!("../migrations/005_task_file_releases.sql"))
            .execute(&pool)
            .await
            .unwrap();
        pool
    }
    fn project(id: &str, name: &str) -> ProjectInput {
        ProjectInput {
            id: id.into(),
            name: name.into(),
            description: "desc".into(),
            status: "active".into(),
            priority: 2,
            start_date: None,
            deadline: None,
            notes: String::new(),
            account_ids: Vec::new(),
        }
    }
    fn task(id: &str, title: &str) -> TaskInput {
        TaskInput {
            id: id.into(),
            project_id: Some("p".into()),
            account_id: Some("a".into()),
            title: title.into(),
            description: String::new(),
            priority: 2,
            status: "backlog".into(),
            deadline: None,
            expected_minutes: Some(30),
            instructions: String::new(),
            completion_notes: String::new(),
            blocker: String::new(),
            depends_on: Vec::new(),
            intended_files: String::new(),
            modified_files: String::new(),
            released_files: String::new(),
        }
    }
    fn usage_limit(account_id: &str) -> UsageLimitInput {
        UsageLimitInput {
            account_id: account_id.into(),
            exact_message: "You have reached your usage limit. Resets at 8:00 PM.".into(),
            reset_at: None,
            checkpoint_summary: "Completed the initial layout pass.".into(),
            remaining_work: "Finish validation and review.".into(),
            files_changed: "src/App.tsx".into(),
            last_successful_step: "Initial layout pass".into(),
            next_action: "Continue with validation after availability is confirmed".into(),
            blockers: String::new(),
        }
    }
    #[tokio::test]
    async fn project_create_edit_and_activity_are_persisted() {
        let dir = tempfile::tempdir().unwrap();
        let db = pool(&dir.path().join("projects.db")).await;
        save_project_transaction(&db, &project("p", "Sunshine"), false)
            .await
            .unwrap();
        let mut updated = project("p", "Sunshine Home");
        updated.status = "paused".into();
        save_project_transaction(&db, &updated, true).await.unwrap();
        let row: (String, String) = sqlx::query_as("SELECT name,status FROM projects WHERE id='p'")
            .fetch_one(&db)
            .await
            .unwrap();
        assert_eq!(row, ("Sunshine Home".into(), "paused".into()));
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM activity_logs WHERE project_id='p'")
                .fetch_one(&db)
                .await
                .unwrap(),
            2
        );
        sqlx::query("CREATE TRIGGER reject_project_activity BEFORE INSERT ON activity_logs BEGIN SELECT RAISE(ABORT,'forced activity failure'); END")
            .execute(&db).await.unwrap();
        assert!(
            save_project_transaction(&db, &project("p2", "Must roll back"), false)
                .await
                .is_err()
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM projects WHERE id='p2'")
                .fetch_one(&db)
                .await
                .unwrap(),
            0
        );
    }

    #[tokio::test]
    async fn approved_ai_task_is_idempotent_and_remains_unassigned_backlog() {
        let dir = tempfile::tempdir().unwrap();
        let db = pool(&dir.path().join("ai-task.db")).await;
        save_project_transaction(&db, &project("p-ai", "AI action test"), false)
            .await
            .unwrap();
        let first = create_ai_task_in_pool(
            &db,
            "ai-action-task".into(),
            "p-ai".into(),
            "Draft release notes".into(),
            "Summarize verified changes".into(),
            2,
        )
        .await
        .unwrap();
        let replay = create_ai_task_in_pool(
            &db,
            "ai-action-task".into(),
            "p-ai".into(),
            "Draft release notes".into(),
            "Summarize verified changes".into(),
            2,
        )
        .await
        .unwrap();
        assert_eq!(first, replay);
        assert!(create_ai_task_in_pool(
            &db,
            "ai-action-task".into(),
            "p-ai".into(),
            "Conflicting title".into(),
            "Summarize verified changes".into(),
            2,
        )
        .await
        .is_err());
        let (status, account): (String, Option<String>) =
            sqlx::query_as("SELECT status,account_id FROM tasks WHERE id='ai-action-task'")
                .fetch_one(&db)
                .await
                .unwrap();
        assert_eq!(status, "backlog");
        assert_eq!(account, None);
        let count: i64 = sqlx::query_scalar("SELECT count(*) FROM tasks WHERE id='ai-action-task'")
            .fetch_one(&db)
            .await
            .unwrap();
        assert_eq!(count, 1);
    }

    #[tokio::test]
    async fn project_account_mapping_is_validated_and_audited_atomically() {
        let dir = tempfile::tempdir().unwrap();
        let db = pool(&dir.path().join("project-membership.db")).await;
        let mut input = project("p", "Workforce verification");
        input.account_ids = vec!["a".into()];
        save_project_transaction(&db, &input, false).await.unwrap();
        assert_eq!(
            sqlx::query_scalar::<_, i64>(
                "SELECT COUNT(*) FROM project_accounts WHERE project_id='p' AND account_id='a'"
            )
            .fetch_one(&db)
            .await
            .unwrap(),
            1
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM activity_logs WHERE project_id='p' AND account_id='a' AND activity_type='account_added_to_project'")
                .fetch_one(&db).await.unwrap(), 1
        );

        input.account_ids.clear();
        save_project_transaction(&db, &input, true).await.unwrap();
        assert_eq!(
            sqlx::query_scalar::<_, i64>(
                "SELECT COUNT(*) FROM project_accounts WHERE project_id='p'"
            )
            .fetch_one(&db)
            .await
            .unwrap(),
            0
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM activity_logs WHERE project_id='p' AND account_id='a' AND activity_type='account_removed_from_project'")
                .fetch_one(&db).await.unwrap(), 1
        );

        input.account_ids = vec!["missing".into()];
        let before: String = sqlx::query_scalar("SELECT name FROM projects WHERE id='p'")
            .fetch_one(&db)
            .await
            .unwrap();
        assert!(save_project_transaction(&db, &input, true)
            .await
            .unwrap_err()
            .contains("no longer exists or is archived"));
        let after: String = sqlx::query_scalar("SELECT name FROM projects WHERE id='p'")
            .fetch_one(&db)
            .await
            .unwrap();
        assert_eq!(before, after);
    }
    #[tokio::test]
    async fn task_assignment_history_and_activity_commit_atomically() {
        let dir = tempfile::tempdir().unwrap();
        let db = pool(&dir.path().join("tasks.db")).await;
        save_project_transaction(&db, &project("p", "Sunshine"), false)
            .await
            .unwrap();
        sqlx::query("INSERT INTO project_accounts(project_id,account_id) VALUES('p','a')")
            .execute(&db)
            .await
            .unwrap();
        save_task_transaction(&db, &task("t", "Build page"), false)
            .await
            .unwrap();
        let mut update = task("t", "Build homepage");
        update.status = "in_progress".into();
        save_task_transaction(&db, &update, true).await.unwrap();
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM task_history WHERE task_id='t'")
                .fetch_one(&db)
                .await
                .unwrap(),
            6
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM activity_logs WHERE task_id='t'")
                .fetch_one(&db)
                .await
                .unwrap(),
            2
        );
        sqlx::query("CREATE TRIGGER reject_task_history BEFORE INSERT ON task_history BEGIN SELECT RAISE(ABORT,'forced history failure'); END").execute(&db).await.unwrap();
        let mut failed = task("t", "Should rollback");
        failed.status = "blocked".into();
        assert!(save_task_transaction(&db, &failed, true).await.is_err());
        let current: (String, String) =
            sqlx::query_as("SELECT title,status FROM tasks WHERE id='t'")
                .fetch_one(&db)
                .await
                .unwrap();
        assert_eq!(current, ("Build homepage".into(), "in_progress".into()));
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM activity_logs WHERE task_id='t'")
                .fetch_one(&db)
                .await
                .unwrap(),
            2
        );
    }
    #[tokio::test]
    async fn task_assignment_requires_project_membership() {
        let dir = tempfile::tempdir().unwrap();
        let db = pool(&dir.path().join("membership.db")).await;
        save_project_transaction(&db, &project("p", "Sunshine"), false)
            .await
            .unwrap();
        let error = save_task_transaction(&db, &task("t", "Build page"), false)
            .await
            .unwrap_err();
        assert!(error.contains("Assign this account to the project"));
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM tasks")
                .fetch_one(&db)
                .await
                .unwrap(),
            0
        );
    }

    #[tokio::test]
    async fn task_assignment_requires_idle_account_and_available_browser_profile() {
        let dir = tempfile::tempdir().unwrap();
        let db = pool(&dir.path().join("eligibility.db")).await;
        save_project_transaction(&db, &project("p", "Sunshine"), false)
            .await
            .unwrap();
        sqlx::query("INSERT INTO project_accounts(project_id,account_id) VALUES('p','a')")
            .execute(&db)
            .await
            .unwrap();

        sqlx::query("UPDATE accounts SET status='waiting' WHERE id='a'")
            .execute(&db)
            .await
            .unwrap();
        let error = save_task_transaction(&db, &task("waiting-worker", "Work"), false)
            .await
            .unwrap_err();
        assert!(error.contains("Account is waiting"));

        sqlx::query("UPDATE accounts SET status='idle',archived_at=datetime('now') WHERE id='a'")
            .execute(&db)
            .await
            .unwrap();
        let error = save_task_transaction(&db, &task("archived-worker", "Work"), false)
            .await
            .unwrap_err();
        assert!(error.contains("archived"));

        sqlx::query("UPDATE accounts SET archived_at=NULL")
            .execute(&db)
            .await
            .unwrap();
        sqlx::query("UPDATE browser_profiles SET availability_status='unavailable' WHERE id='bp'")
            .execute(&db)
            .await
            .unwrap();
        let error = save_task_transaction(&db, &task("unavailable-profile", "Work"), false)
            .await
            .unwrap_err();
        assert!(error.contains("browser profile is unavailable"));

        sqlx::query("UPDATE browser_profiles SET availability_status='available'")
            .execute(&db)
            .await
            .unwrap();
        sqlx::query("UPDATE browsers SET availability_status='unavailable'")
            .execute(&db)
            .await
            .unwrap();
        let error = save_task_transaction(&db, &task("unavailable-browser", "Work"), false)
            .await
            .unwrap_err();
        assert!(error.contains("browser is unavailable"));
        let counts: (i64, i64, i64) = sqlx::query_as(
            "SELECT (SELECT COUNT(*) FROM tasks),(SELECT COUNT(*) FROM task_history),(SELECT COUNT(*) FROM activity_logs)",
        ).fetch_one(&db).await.unwrap();
        assert_eq!(counts, (0, 0, 1)); // only project creation activity exists
    }

    #[tokio::test]
    async fn concurrent_active_assignments_cannot_claim_one_worker_twice() {
        let dir = tempfile::tempdir().unwrap();
        let db = pool(&dir.path().join("concurrent-assignments.db")).await;
        save_project_transaction(&db, &project("p", "Sunshine"), false)
            .await
            .unwrap();
        sqlx::query("INSERT INTO project_accounts(project_id,account_id) VALUES('p','a')")
            .execute(&db)
            .await
            .unwrap();
        let mut first = task("first", "First task");
        first.status = "in_progress".into();
        let mut second = task("second", "Second task");
        second.status = "in_progress".into();
        let (first_result, second_result) = tokio::join!(
            save_task_transaction(&db, &first, false),
            save_task_transaction(&db, &second, false),
        );
        assert_ne!(first_result.is_ok(), second_result.is_ok());
        let assigned: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM tasks WHERE account_id='a' AND status='in_progress'",
        )
        .fetch_one(&db)
        .await
        .unwrap();
        assert_eq!(assigned, 1);
        let rejected = if first_result.is_err() {
            first_result.unwrap_err()
        } else {
            second_result.unwrap_err()
        };
        assert!(rejected.contains("already has active task"));
    }

    #[tokio::test]
    async fn invalid_calendar_dates_are_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let db = pool(&dir.path().join("dates.db")).await;
        let mut p = project("p", "Bad date");
        p.deadline = Some("2026-02-30".into());
        assert!(save_project_transaction(&db, &p, false)
            .await
            .unwrap_err()
            .contains("valid calendar date"));
    }

    #[tokio::test]
    async fn usage_limit_saves_checkpoint_waiting_state_reminder_and_history_atomically() {
        let dir = tempfile::tempdir().unwrap();
        let db = pool(&dir.path().join("usage-limit.db")).await;
        save_project_transaction(&db, &project("p", "Sunshine"), false)
            .await
            .unwrap();
        sqlx::query("INSERT INTO project_accounts(project_id,account_id) VALUES('p','a')")
            .execute(&db)
            .await
            .unwrap();
        let mut active = task("t", "Implement controls");
        active.status = "in_progress".into();
        save_task_transaction(&db, &active, false).await.unwrap();

        let input = usage_limit("a");
        mark_usage_limited_transaction(&db, &input).await.unwrap();

        let task_row: (String, String) =
            sqlx::query_as("SELECT account_id,status FROM tasks WHERE id='t'")
                .fetch_one(&db)
                .await
                .unwrap();
        assert_eq!(task_row, ("a".into(), "waiting".into()));
        assert_eq!(
            sqlx::query_scalar::<_, String>("SELECT status FROM accounts WHERE id='a'")
                .fetch_one(&db)
                .await
                .unwrap(),
            "waiting"
        );
        let event: (String, Option<String>, String) = sqlx::query_as(
            "SELECT exact_message,reset_at,reset_status FROM usage_limit_events WHERE account_id='a'",
        )
        .fetch_one(&db)
        .await
        .unwrap();
        assert_eq!(event.0, input.exact_message);
        assert_eq!(event.1, None);
        assert_eq!(event.2, "unknown");
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM task_checkpoints WHERE task_id='t'")
                .fetch_one(&db)
                .await
                .unwrap(),
            1
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>(
                "SELECT COUNT(*) FROM reminders WHERE account_id='a' AND status='pending'"
            )
            .fetch_one(&db)
            .await
            .unwrap(),
            1
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM task_history WHERE task_id='t' AND field='status' AND new_value='waiting'")
                .fetch_one(&db).await.unwrap(),
            1
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM activity_logs WHERE activity_type='usage_limit_recorded' AND task_id='t'")
                .fetch_one(&db).await.unwrap(),
            1
        );
    }

    #[tokio::test]
    async fn usage_limit_failure_rolls_back_all_related_records() {
        let dir = tempfile::tempdir().unwrap();
        let db = pool(&dir.path().join("usage-limit-rollback.db")).await;
        save_project_transaction(&db, &project("p", "Sunshine"), false)
            .await
            .unwrap();
        sqlx::query("INSERT INTO project_accounts(project_id,account_id) VALUES('p','a')")
            .execute(&db)
            .await
            .unwrap();
        let mut active = task("t", "Implement controls");
        active.status = "in_progress".into();
        save_task_transaction(&db, &active, false).await.unwrap();
        sqlx::query("CREATE TRIGGER reject_usage_activity BEFORE INSERT ON activity_logs WHEN NEW.activity_type='usage_limit_recorded' BEGIN SELECT RAISE(ABORT,'forced activity failure'); END")
            .execute(&db)
            .await
            .unwrap();

        assert!(mark_usage_limited_transaction(&db, &usage_limit("a"))
            .await
            .is_err());
        let row: (String, String) =
            sqlx::query_as("SELECT account_id,status FROM tasks WHERE id='t'")
                .fetch_one(&db)
                .await
                .unwrap();
        assert_eq!(row, ("a".into(), "in_progress".into()));
        assert_eq!(
            sqlx::query_scalar::<_, String>("SELECT status FROM accounts WHERE id='a'")
                .fetch_one(&db)
                .await
                .unwrap(),
            "idle"
        );
        for table in ["usage_limit_events", "task_checkpoints", "reminders"] {
            let sql = format!("SELECT COUNT(*) FROM {table}");
            assert_eq!(
                sqlx::query_scalar::<_, i64>(&sql)
                    .fetch_one(&db)
                    .await
                    .unwrap(),
                0
            );
        }
        assert_eq!(
            sqlx::query_scalar::<_, i64>(
                "SELECT COUNT(*) FROM task_history WHERE task_id='t' AND field='status' AND new_value='waiting'"
            )
            .fetch_one(&db)
            .await
            .unwrap(),
            0
        );
    }

    #[tokio::test]
    async fn usage_limit_resume_requires_user_confirmation_and_keeps_original_assignment() {
        let dir = tempfile::tempdir().unwrap();
        let db = pool(&dir.path().join("usage-limit-resume.db")).await;
        save_project_transaction(&db, &project("p", "Sunshine"), false)
            .await
            .unwrap();
        sqlx::query("INSERT INTO project_accounts(project_id,account_id) VALUES('p','a')")
            .execute(&db)
            .await
            .unwrap();
        let mut active = task("t", "Implement controls");
        active.status = "in_progress".into();
        save_task_transaction(&db, &active, false).await.unwrap();
        mark_usage_limited_transaction(&db, &usage_limit("a"))
            .await
            .unwrap();

        let not_confirmed = ResumeUsageInput {
            account_id: "a".into(),
            reset_verified: false,
            user_authorized: true,
        };
        assert!(resume_usage_limited_transaction(&db, &not_confirmed)
            .await
            .unwrap_err()
            .contains("Confirm"));
        let input = ResumeUsageInput {
            account_id: "a".into(),
            reset_verified: true,
            user_authorized: true,
        };
        resume_usage_limited_transaction(&db, &input).await.unwrap();

        let task_row: (String, String) =
            sqlx::query_as("SELECT account_id,status FROM tasks WHERE id='t'")
                .fetch_one(&db)
                .await
                .unwrap();
        assert_eq!(task_row, ("a".into(), "ready".into()));
        assert_eq!(
            sqlx::query_scalar::<_, String>("SELECT status FROM accounts WHERE id='a'")
                .fetch_one(&db)
                .await
                .unwrap(),
            "idle"
        );
        assert_eq!(
            sqlx::query_scalar::<_, String>(
                "SELECT status FROM usage_limit_events WHERE account_id='a'"
            )
            .fetch_one(&db)
            .await
            .unwrap(),
            "resolved"
        );
        assert_eq!(
            sqlx::query_scalar::<_, String>("SELECT status FROM reminders WHERE account_id='a'")
                .fetch_one(&db)
                .await
                .unwrap(),
            "completed"
        );
    }

    async fn prepare_queue(db: &SqlitePool) {
        save_project_transaction(db, &project("p", "Sunshine"), false)
            .await
            .unwrap();
        sqlx::query("INSERT INTO project_accounts(project_id,account_id) VALUES('p','a')")
            .execute(db)
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn automatic_queue_assigns_ready_task_and_records_history_atomically() {
        let dir = tempfile::tempdir().unwrap();
        let db = pool(&dir.path().join("queue-success.db")).await;
        prepare_queue(&db).await;
        let mut ready = task("t", "Queue task");
        ready.account_id = None;
        ready.status = "ready".into();
        ready.intended_files = "src\\./App.tsx".into();
        save_task_transaction(&db, &ready, false).await.unwrap();
        let assigned = assign_next_eligible_transaction(&db)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            (assigned.task_id.as_str(), assigned.account_id.as_str()),
            ("t", "a")
        );
        assert_eq!(
            sqlx::query_scalar::<_, String>("SELECT status FROM tasks WHERE id='t'")
                .fetch_one(&db)
                .await
                .unwrap(),
            "ready"
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM task_file_claims WHERE task_id='t' AND status='active' AND path_key='src/app.tsx'")
                .fetch_one(&db).await.unwrap(), 1
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM task_history WHERE task_id='t' AND field='account_id' AND new_value='a'")
                .fetch_one(&db).await.unwrap(), 1
        );
    }

    #[tokio::test]
    async fn queue_returns_none_without_workers_and_skips_ineligible_accounts() {
        let dir = tempfile::tempdir().unwrap();
        let db = pool(&dir.path().join("queue-no-worker.db")).await;
        prepare_queue(&db).await;
        let mut ready = task("t", "Queue task");
        ready.account_id = None;
        ready.status = "ready".into();
        save_task_transaction(&db, &ready, false).await.unwrap();
        sqlx::query("UPDATE accounts SET status='waiting' WHERE id='a'")
            .execute(&db)
            .await
            .unwrap();
        assert!(assign_next_eligible_transaction(&db)
            .await
            .unwrap()
            .is_none());
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM activity_logs WHERE task_id='t' AND activity_type='task_assignment_failed'")
                .fetch_one(&db).await.unwrap(), 1
        );
        let error = assign_task_transaction(&db, "t", "a").await.unwrap_err();
        assert!(error.contains("Account is waiting"));
        assert_eq!(
            sqlx::query_scalar::<_, i64>(
                "SELECT COUNT(*) FROM task_history WHERE task_id='t' AND field='account_id'"
            )
            .fetch_one(&db)
            .await
            .unwrap(),
            0
        );
    }

    #[tokio::test]
    async fn dependencies_reject_self_cycles_and_incomplete_prerequisites() {
        let dir = tempfile::tempdir().unwrap();
        let db = pool(&dir.path().join("queue-dependencies.db")).await;
        prepare_queue(&db).await;
        let mut first = task("first", "Prerequisite");
        first.account_id = None;
        first.status = "backlog".into();
        save_task_transaction(&db, &first, false).await.unwrap();
        let mut second = task("second", "Dependent");
        second.account_id = None;
        second.status = "ready".into();
        second.depends_on = vec!["first".into()];
        save_task_transaction(&db, &second, false).await.unwrap();
        assert!(assign_task_transaction(&db, "second", "a")
            .await
            .unwrap_err()
            .contains("incomplete dependencies"));
        let mut self_dep = second;
        self_dep.depends_on = vec!["second".into()];
        assert!(save_task_transaction(&db, &self_dep, true)
            .await
            .unwrap_err()
            .contains("cannot depend on itself"));
        let mut cycle = first;
        cycle.depends_on = vec!["second".into()];
        assert!(save_task_transaction(&db, &cycle, true)
            .await
            .unwrap_err()
            .contains("circular"));
        sqlx::query("UPDATE tasks SET status='completed' WHERE id='first'")
            .execute(&db)
            .await
            .unwrap();
        let mut ready_second = task("second", "Dependent");
        ready_second.account_id = None;
        ready_second.status = "ready".into();
        ready_second.depends_on = vec!["first".into()];
        save_task_transaction(&db, &ready_second, true)
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn dependency_assignment_checks_the_entire_chain() {
        let dir = tempfile::tempdir().unwrap();
        let db = pool(&dir.path().join("queue-dependency-chain.db")).await;
        prepare_queue(&db).await;
        let mut root = task("root", "Unfinished root");
        root.account_id = None;
        root.status = "backlog".into();
        save_task_transaction(&db, &root, false).await.unwrap();
        let mut middle = task("middle", "Middle");
        middle.account_id = None;
        middle.status = "backlog".into();
        middle.depends_on = vec!["root".into()];
        save_task_transaction(&db, &middle, false).await.unwrap();
        // Represent a legacy/inconsistent completed state: the transitive ancestor must still block.
        sqlx::query("UPDATE tasks SET status='completed' WHERE id='middle'")
            .execute(&db)
            .await
            .unwrap();
        let mut leaf = task("leaf", "Leaf");
        leaf.account_id = None;
        leaf.status = "ready".into();
        leaf.depends_on = vec!["middle".into()];
        save_task_transaction(&db, &leaf, false).await.unwrap();
        assert!(assign_task_transaction(&db, "leaf", "a")
            .await
            .unwrap_err()
            .contains("Unfinished root"));
        assert!(assign_next_eligible_transaction(&db)
            .await
            .unwrap()
            .is_none());
        let mut finish_middle = task("middle", "Middle");
        finish_middle.account_id = None;
        finish_middle.status = "completed".into();
        finish_middle.depends_on = vec!["root".into()];
        assert!(save_task_transaction(&db, &finish_middle, true)
            .await
            .unwrap_err()
            .contains("Unfinished root"));
    }

    #[tokio::test]
    async fn file_claims_reject_same_and_parent_child_overlaps_but_normalize_paths() {
        let dir = tempfile::tempdir().unwrap();
        let db = pool(&dir.path().join("queue-files.db")).await;
        prepare_queue(&db).await;
        sqlx::query(
            "INSERT INTO accounts(id,name,browser_profile_id) VALUES('a2','Worker 2','bp')",
        )
        .execute(&db)
        .await
        .unwrap();
        sqlx::query("INSERT INTO project_accounts(project_id,account_id) VALUES('p','a2')")
            .execute(&db)
            .await
            .unwrap();
        let mut first = task("one", "One");
        first.status = "ready".into();
        first.intended_files = "src\\components".into();
        save_task_transaction(&db, &first, false).await.unwrap();
        let mut second = task("two", "Two");
        second.account_id = None;
        second.status = "ready".into();
        second.intended_files = "SRC/components/App.tsx".into();
        save_task_transaction(&db, &second, false).await.unwrap();
        assert!(assign_task_transaction(&db, "two", "a2")
            .await
            .unwrap_err()
            .contains("overlaps"));
        second.intended_files = "other/file.ts".into();
        save_task_transaction(&db, &second, true).await.unwrap();
        assert!(normalize_file_declarations("../outside", "").is_err());
        let normalized = normalize_file_declarations("src\\x\\..\\App.tsx", "").unwrap();
        assert_eq!(normalized[0].0, "src/app.tsx");
        assert_eq!(
            normalize_file_declarations("SRC/App.tsx.", "").unwrap()[0].0,
            "src/app.tsx"
        );
    }

    #[tokio::test]
    async fn completion_and_explicit_unassignment_release_claims_but_waiting_preserves_them() {
        let dir = tempfile::tempdir().unwrap();
        let db = pool(&dir.path().join("queue-release.db")).await;
        prepare_queue(&db).await;
        let mut active = task("t", "Claim");
        active.status = "in_progress".into();
        active.intended_files = "src/App.tsx".into();
        save_task_transaction(&db, &active, false).await.unwrap();
        mark_usage_limited_transaction(&db, &usage_limit("a"))
            .await
            .unwrap();
        assert_eq!(
            sqlx::query_scalar::<_, i64>(
                "SELECT COUNT(*) FROM task_file_claims WHERE task_id='t' AND status='active'"
            )
            .fetch_one(&db)
            .await
            .unwrap(),
            1
        );
        assert!(release_task_assignment_transaction(&db, "t")
            .await
            .unwrap_err()
            .contains("waiting task"));
        sqlx::query("UPDATE tasks SET status='completed' WHERE id='t'")
            .execute(&db)
            .await
            .unwrap();
        let mut completed = task("t", "Claim");
        completed.status = "completed".into();
        completed.intended_files = "src/App.tsx".into();
        save_task_transaction(&db, &completed, true).await.unwrap();
        assert_eq!(
            sqlx::query_scalar::<_, i64>(
                "SELECT COUNT(*) FROM task_file_claims WHERE task_id='t' AND status='active'"
            )
            .fetch_one(&db)
            .await
            .unwrap(),
            0
        );
    }

    #[tokio::test]
    async fn explicit_release_unassigns_worker_and_releases_claim_transactionally() {
        let dir = tempfile::tempdir().unwrap();
        let db = pool(&dir.path().join("queue-explicit-release.db")).await;
        prepare_queue(&db).await;
        let mut active = task("t", "Release me");
        active.status = "ready".into();
        active.intended_files = "src/App.tsx".into();
        save_task_transaction(&db, &active, false).await.unwrap();
        release_task_assignment_transaction(&db, "t").await.unwrap();
        let row: (Option<String>, String) =
            sqlx::query_as("SELECT account_id,status FROM tasks WHERE id='t'")
                .fetch_one(&db)
                .await
                .unwrap();
        assert_eq!(row, (None, "ready".into()));
        assert_eq!(
            sqlx::query_scalar::<_, i64>(
                "SELECT COUNT(*) FROM task_file_claims WHERE task_id='t' AND status='active'"
            )
            .fetch_one(&db)
            .await
            .unwrap(),
            0
        );
        assert_eq!(sqlx::query_scalar::<_,i64>("SELECT COUNT(*) FROM activity_logs WHERE task_id='t' AND activity_type='task_assignment_released'").fetch_one(&db).await.unwrap(),1);
    }

    #[tokio::test]
    async fn task_can_release_one_declared_file_and_keep_its_assignment() {
        let dir = tempfile::tempdir().unwrap();
        let db = pool(&dir.path().join("queue-file-release.db")).await;
        prepare_queue(&db).await;
        let mut active = task("t", "Partial release");
        active.status = "ready".into();
        active.intended_files = "src/App.tsx\nsrc/Other.tsx".into();
        save_task_transaction(&db, &active, false).await.unwrap();
        active.intended_files = "src/Other.tsx".into();
        active.released_files = "src\\App.tsx".into();
        save_task_transaction(&db, &active, true).await.unwrap();
        assert_eq!(
            sqlx::query_scalar::<_, i64>(
                "SELECT COUNT(*) FROM task_file_claims WHERE task_id='t' AND status='active'"
            )
            .fetch_one(&db)
            .await
            .unwrap(),
            1
        );
        assert_eq!(
            sqlx::query_scalar::<_, String>(
                "SELECT path_display FROM task_file_releases WHERE task_id='t'"
            )
            .fetch_one(&db)
            .await
            .unwrap(),
            "src/App.tsx"
        );
        assert_eq!(
            sqlx::query_scalar::<_, Option<String>>("SELECT account_id FROM tasks WHERE id='t'")
                .fetch_one(&db)
                .await
                .unwrap(),
            Some("a".into())
        );
    }

    #[tokio::test]
    async fn file_claim_conflicts_are_serialized_across_concurrent_assignments() {
        let dir = tempfile::tempdir().unwrap();
        let db = pool(&dir.path().join("queue-concurrent-files.db")).await;
        prepare_queue(&db).await;
        sqlx::query(
            "INSERT INTO accounts(id,name,browser_profile_id) VALUES('a2','Worker 2','bp')",
        )
        .execute(&db)
        .await
        .unwrap();
        sqlx::query("INSERT INTO project_accounts(project_id,account_id) VALUES('p','a2')")
            .execute(&db)
            .await
            .unwrap();
        let mut one = task("one", "One");
        one.account_id = None;
        one.status = "ready".into();
        one.intended_files = "src".into();
        let mut two = task("two", "Two");
        two.account_id = None;
        two.status = "ready".into();
        two.intended_files = "src/App.tsx".into();
        save_task_transaction(&db, &one, false).await.unwrap();
        save_task_transaction(&db, &two, false).await.unwrap();
        let results = tokio::join!(
            assign_task_transaction(&db, "one", "a"),
            assign_task_transaction(&db, "two", "a2")
        );
        assert_ne!(results.0.is_ok(), results.1.is_ok());
    }

    #[tokio::test]
    async fn concurrent_workers_cannot_overwrite_the_same_task_assignment() {
        let dir = tempfile::tempdir().unwrap();
        let db = pool(&dir.path().join("queue-same-task-race.db")).await;
        prepare_queue(&db).await;
        sqlx::query(
            "INSERT INTO accounts(id,name,browser_profile_id) VALUES('a2','Worker 2','bp')",
        )
        .execute(&db)
        .await
        .unwrap();
        sqlx::query("INSERT INTO project_accounts(project_id,account_id) VALUES('p','a2')")
            .execute(&db)
            .await
            .unwrap();
        let mut ready = task("t", "Competing assignment");
        ready.account_id = None;
        ready.status = "ready".into();
        save_task_transaction(&db, &ready, false).await.unwrap();
        let results = tokio::join!(
            assign_task_transaction(&db, "t", "a"),
            assign_task_transaction(&db, "t", "a2")
        );
        assert_ne!(results.0.is_ok(), results.1.is_ok());
        let account: Option<String> =
            sqlx::query_scalar("SELECT account_id FROM tasks WHERE id='t'")
                .fetch_one(&db)
                .await
                .unwrap();
        assert!(account.as_deref() == Some("a") || account.as_deref() == Some("a2"));
        assert_eq!(sqlx::query_scalar::<_,i64>("SELECT COUNT(*) FROM task_history WHERE task_id='t' AND field='account_id' AND new_value IS NOT NULL").fetch_one(&db).await.unwrap(),1);
    }

    #[tokio::test]
    async fn task_history_failure_rolls_back_file_claim_and_account_assignment() {
        let dir = tempfile::tempdir().unwrap();
        let db = pool(&dir.path().join("queue-rollback.db")).await;
        prepare_queue(&db).await;
        let mut ready = task("t", "Rollback");
        ready.account_id = None;
        ready.status = "ready".into();
        ready.intended_files = "src/App.tsx".into();
        save_task_transaction(&db, &ready, false).await.unwrap();
        sqlx::query("CREATE TRIGGER reject_queue_history BEFORE INSERT ON task_history BEGIN SELECT RAISE(ABORT,'forced'); END").execute(&db).await.unwrap();
        assert!(assign_task_transaction(&db, "t", "a").await.is_err());
        assert_eq!(
            sqlx::query_scalar::<_, i64>(
                "SELECT COUNT(*) FROM task_file_claims WHERE task_id='t' AND status='active'"
            )
            .fetch_one(&db)
            .await
            .unwrap(),
            0
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>(
                "SELECT COUNT(*) FROM tasks WHERE id='t' AND account_id IS NOT NULL"
            )
            .fetch_one(&db)
            .await
            .unwrap(),
            0
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM activity_logs WHERE task_id='t'")
                .fetch_one(&db)
                .await
                .unwrap(),
            1
        );
    }
}
