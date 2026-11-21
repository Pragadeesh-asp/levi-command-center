use crate::db::database;
use serde::Deserialize;
#[cfg(test)]
use serde::Serialize;
use sqlx::{SqlitePool, Transaction};
use tauri::AppHandle;
use uuid::Uuid;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CheckpointInput {
    pub task_id: String,
    pub completed_work: String,
    #[serde(default)]
    pub remaining_work: String,
    #[serde(default)]
    pub modified_files: String,
    #[serde(default)]
    pub blockers: String,
    #[serde(default)]
    pub next_action: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompletionReportInput {
    pub task_id: String,
    pub completed_work: String,
    #[serde(default)]
    pub remaining_work: String,
    #[serde(default)]
    pub modified_files: String,
    #[serde(default)]
    pub blockers: String,
    #[serde(default)]
    pub next_action: String,
    #[serde(default)]
    pub tests_executed: String,
    #[serde(default)]
    pub test_results: String,
    #[serde(default)]
    pub known_issues: String,
    #[serde(default)]
    pub deliverable_refs: String,
    #[serde(default)]
    pub review_questions: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewFeedbackInput {
    pub task_id: String,
    pub decision: String,
    pub feedback: String,
    #[serde(default)]
    pub required_changes: String,
    #[serde(default)]
    pub acceptance_criteria: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImplementationReportInput {
    pub task_id: String,
    pub completed_work: String,
    #[serde(default)]
    pub modified_files: String,
    #[serde(default)]
    pub tests_executed: String,
    #[serde(default)]
    pub test_results: String,
    #[serde(default)]
    pub known_issues: String,
    #[serde(default)]
    pub evidence: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VerificationInput {
    pub task_id: String,
    pub passed: bool,
    pub verification_steps: String,
    pub test_results: String,
    #[serde(default)]
    pub evidence: String,
    #[serde(default)]
    pub remaining_failures: String,
}

#[cfg(test)]
#[derive(Debug, Serialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct CheckpointRecord {
    pub id: String,
    pub task_id: Option<String>,
    pub account_id: Option<String>,
    pub project_id: Option<String>,
    pub browser_profile_id: Option<String>,
    pub task_title_snapshot: String,
    pub project_name_snapshot: String,
    pub account_name_snapshot: String,
    pub browser_profile_name_snapshot: String,
    pub summary: String,
    pub remaining_work: String,
    pub files_changed: String,
    pub last_successful_step: String,
    pub next_action: String,
    pub blockers: String,
    pub created_at: String,
}

#[tauri::command]
pub async fn save_task_checkpoint(
    app: AppHandle,
    input: CheckpointInput,
) -> Result<String, String> {
    save_checkpoint_transaction(&database(&app).await?, &input).await
}

#[tauri::command]
pub async fn report_task_completion(
    app: AppHandle,
    input: CompletionReportInput,
) -> Result<String, String> {
    report_completion_transaction(&database(&app).await?, &input).await
}

#[tauri::command]
pub async fn record_review_feedback(
    app: AppHandle,
    input: ReviewFeedbackInput,
) -> Result<String, String> {
    record_review_feedback_transaction(&database(&app).await?, &input).await
}

#[tauri::command]
pub async fn record_implementation_report(
    app: AppHandle,
    input: ImplementationReportInput,
) -> Result<String, String> {
    record_implementation_report_transaction(&database(&app).await?, &input).await
}

#[tauri::command]
pub async fn record_task_verification(
    app: AppHandle,
    input: VerificationInput,
) -> Result<(), String> {
    record_verification_transaction(&database(&app).await?, &input).await
}

#[tauri::command]
pub async fn mark_handoff_started(
    app: AppHandle,
    task_id: String,
    handoff_id: String,
) -> Result<(), String> {
    mark_handoff_started_transaction(&database(&app).await?, &task_id, &handoff_id).await
}

#[tauri::command]
pub async fn resume_from_checkpoint(
    app: AppHandle,
    checkpoint_id: String,
    reset_verified: bool,
    user_authorized: bool,
) -> Result<(), String> {
    resume_checkpoint_transaction(
        &database(&app).await?,
        &checkpoint_id,
        reset_verified,
        user_authorized,
    )
    .await
}

async fn save_checkpoint_transaction(
    pool: &SqlitePool,
    input: &CheckpointInput,
) -> Result<String, String> {
    validate_checkpoint(
        &input.completed_work,
        &input.remaining_work,
        &input.modified_files,
        &input.blockers,
        &input.next_action,
    )?;
    let mut tx = pool.begin_with("BEGIN IMMEDIATE").await.map_err(db_error)?;
    let checkpoint_id = insert_checkpoint(
        &mut tx,
        &input.task_id,
        &input.completed_work,
        &input.remaining_work,
        &input.modified_files,
        &input.blockers,
        &input.next_action,
    )
    .await?;
    audit(
        &mut tx,
        &input.task_id,
        "checkpoint_created",
        &format!("Saved checkpoint {checkpoint_id}"),
        None,
    )
    .await?;
    tx.commit().await.map_err(db_error)?;
    Ok(checkpoint_id)
}

async fn report_completion_transaction(
    pool: &SqlitePool,
    input: &CompletionReportInput,
) -> Result<String, String> {
    validate_report(input)?;
    let mut tx = pool.begin_with("BEGIN IMMEDIATE").await.map_err(db_error)?;
    let task = task_context(&mut tx, &input.task_id).await?;
    if task.status != "in_progress" {
        return Err("Only an assigned In Progress task can be submitted for review.".into());
    }
    let checkpoint = insert_checkpoint(
        &mut tx,
        &task.id,
        &input.completed_work,
        &input.remaining_work,
        &input.modified_files,
        &input.blockers,
        &input.next_action,
    )
    .await?;
    let details = serde_json::json!({"completedWork":input.completed_work,"remainingWork":input.remaining_work,"modifiedFiles":input.modified_files,"blockers":input.blockers,"nextAction":input.next_action,"testsExecuted":input.tests_executed,"testResults":input.test_results,"knownIssues":input.known_issues,"deliverableRefs":input.deliverable_refs,"reviewQuestions":input.review_questions}).to_string();
    insert_deliverable(&mut tx, &task, &checkpoint, &details).await?;
    let package = review_package(&task, input);
    let handoff = insert_handoff(
        &mut tx,
        &task,
        Some(&checkpoint),
        "chatgpt_review",
        "prepared",
        &package,
        &details,
    )
    .await?;
    set_task_status(&mut tx, &task, "review").await?;
    audit(
        &mut tx,
        &task.id,
        "review_package_prepared",
        "Prepared a manual ChatGPT review package; no external review has occurred.",
        Some(&handoff),
    )
    .await?;
    tx.commit().await.map_err(db_error)?;
    Ok(handoff)
}

async fn record_review_feedback_transaction(
    pool: &SqlitePool,
    input: &ReviewFeedbackInput,
) -> Result<String, String> {
    if !matches!(input.decision.as_str(), "revision_required" | "approved") {
        return Err("Choose Revision required or Approved for the review decision.".into());
    }
    required("Review feedback", &input.feedback, 12_000)?;
    optional("Required changes", &input.required_changes, 8_000)?;
    optional("Acceptance criteria", &input.acceptance_criteria, 8_000)?;
    let mut tx = pool.begin_with("BEGIN IMMEDIATE").await.map_err(db_error)?;
    let task = task_context(&mut tx, &input.task_id).await?;
    if task.status != "review" {
        return Err(
            "Review feedback can only be recorded while the task is Awaiting Review.".into(),
        );
    }
    let review: Option<(String, Option<String>, String)> = sqlx::query_as("SELECT id,checkpoint_id,package_text FROM task_handoffs WHERE task_id=? AND kind='chatgpt_review' AND status='prepared' ORDER BY created_at DESC LIMIT 1").bind(&task.id).fetch_optional(&mut *tx).await.map_err(db_error)?;
    let Some((review_id, checkpoint, source_package)) = review else {
        return Err("No prepared review package exists. Submit a completion report first.".into());
    };
    sqlx::query("UPDATE task_handoffs SET status='feedback_recorded',details_json=?,updated_at=datetime('now') WHERE id=? AND status='prepared'").bind(serde_json::json!({"decision":input.decision,"feedback":input.feedback,"requiredChanges":input.required_changes,"acceptanceCriteria":input.acceptance_criteria}).to_string()).bind(&review_id).execute(&mut *tx).await.map_err(db_error)?;
    audit(
        &mut tx,
        &task.id,
        "review_feedback_recorded",
        &input.feedback,
        Some(&review_id),
    )
    .await?;
    let (kind, status, package) = if input.decision == "revision_required" {
        ("claude_revision", "revision_required", format!("# Claude revision handoff — manual\n\n## Original task\n{}\n\n## Original requirements\n{}\n\n## Completion package\n{}\n\n## Review feedback\n{}\n\n## Required changes\n{}\n\n## Acceptance criteria\n{}\n\nResume with the original assigned account. The account/task mapping is preserved. This is a prepared instruction only; LEVI has not sent it to Claude or started work.", task.title, task.instructions, source_package, input.feedback, input.required_changes, input.acceptance_criteria))
    } else {
        ("copilot_implementation", "approved", format!("# Copilot implementation handoff — manual\n\n## Approved changes\n{}\n\n## Task\n{}\n\n## Requirements\n{}\n\n## Acceptance criteria\n{}\n\n## Required modifications\n{}\n\n## Tests to run\nRun the acceptance checks and report exact commands, results, changed files, failures and evidence. A handoff is not implementation evidence.\n\nThe user recorded this review as Approved. LEVI has not contacted ChatGPT or Copilot and has not executed this handoff.", input.feedback, task.title, task.instructions, input.acceptance_criteria, input.required_changes))
    };
    let details = serde_json::json!({"sourceReviewId":review_id,"decision":input.decision,"feedback":input.feedback,"requiredChanges":input.required_changes,"acceptanceCriteria":input.acceptance_criteria}).to_string();
    let id = insert_handoff(
        &mut tx,
        &task,
        checkpoint.as_deref(),
        kind,
        status,
        &package,
        &details,
    )
    .await?;
    audit(
        &mut tx,
        &task.id,
        if kind == "claude_revision" {
            "claude_revision_prepared"
        } else {
            "copilot_handoff_prepared"
        },
        "Generated a manual next-step handoff from recorded review feedback.",
        Some(&id),
    )
    .await?;
    tx.commit().await.map_err(db_error)?;
    Ok(id)
}

async fn record_implementation_report_transaction(
    pool: &SqlitePool,
    input: &ImplementationReportInput,
) -> Result<String, String> {
    required("Implementation report", &input.completed_work, 12_000)?;
    optional("Modified files", &input.modified_files, 8_000)?;
    optional("Tests executed", &input.tests_executed, 8_000)?;
    optional("Test results", &input.test_results, 8_000)?;
    optional("Known issues", &input.known_issues, 8_000)?;
    optional("Evidence", &input.evidence, 8_000)?;
    let mut tx = pool.begin_with("BEGIN IMMEDIATE").await.map_err(db_error)?;
    let task = task_context(&mut tx, &input.task_id).await?;
    if task.status != "review" {
        return Err(
            "Implementation results can only be recorded for a task in the review workflow.".into(),
        );
    }
    let implementation: Option<(String, String)> = sqlx::query_as("SELECT id,package_text FROM task_handoffs WHERE task_id=? AND kind='copilot_implementation' AND status IN ('approved','started') ORDER BY created_at DESC LIMIT 1").bind(&task.id).fetch_optional(&mut *tx).await.map_err(db_error)?;
    let Some((parent_id, parent_package)) = implementation else {
        return Err("No approved Copilot handoff exists. Record an approved review first.".into());
    };
    let report = format!("Implementation summary:\n{}\n\nModified files:\n{}\n\nTests executed:\n{}\n\nTest results:\n{}\n\nKnown issues:\n{}\n\nEvidence:\n{}", input.completed_work,input.modified_files,input.tests_executed,input.test_results,input.known_issues,input.evidence);
    let checkpoint = insert_checkpoint(
        &mut tx,
        &task.id,
        &input.completed_work,
        "",
        &input.modified_files,
        &input.known_issues,
        "Verify the implementation report",
    )
    .await?;
    insert_deliverable(&mut tx, &task, &checkpoint, &report).await?;
    sqlx::query(
        "UPDATE task_handoffs SET status='submitted',updated_at=datetime('now') WHERE id=?",
    )
    .bind(&parent_id)
    .execute(&mut *tx)
    .await
    .map_err(db_error)?;
    audit(
        &mut tx,
        &task.id,
        "implementation_report_recorded",
        &report,
        Some(&parent_id),
    )
    .await?;
    let package = format!("# Verification handoff — manual\n\n## Task\n{}\n\n## Approved implementation instructions\n{}\n\n## Reported implementation\n{}\n\nRun the acceptance checks independently and record exact commands, results, evidence and remaining failures. This report has not been independently verified.",task.title,parent_package,report);
    let details = serde_json::json!({"implementationHandoffId":parent_id,"checkpointId":checkpoint,"reportedTests":input.tests_executed,"reportedResults":input.test_results}).to_string();
    let id = insert_handoff(
        &mut tx,
        &task,
        Some(&checkpoint),
        "verification",
        "prepared",
        &package,
        &details,
    )
    .await?;
    audit(&mut tx,&task.id,"verification_prepared","Prepared a verification record from the reported implementation; verification has not yet passed.",Some(&id)).await?;
    tx.commit().await.map_err(db_error)?;
    Ok(id)
}

async fn record_verification_transaction(
    pool: &SqlitePool,
    input: &VerificationInput,
) -> Result<(), String> {
    required("Verification steps", &input.verification_steps, 8_000)?;
    required("Test results", &input.test_results, 8_000)?;
    optional("Evidence", &input.evidence, 8_000)?;
    optional("Remaining failures", &input.remaining_failures, 8_000)?;
    if input.passed && !input.remaining_failures.trim().is_empty() {
        return Err("A verification with remaining failures cannot be recorded as passed.".into());
    }
    let mut tx = pool.begin_with("BEGIN IMMEDIATE").await.map_err(db_error)?;
    let task = task_context(&mut tx, &input.task_id).await?;
    if task.status != "review" {
        return Err(
            "Verification can only be recorded while the task is in the review workflow.".into(),
        );
    }
    let handoff: Option<String> = sqlx::query_scalar("SELECT id FROM task_handoffs WHERE task_id=? AND kind='verification' AND status IN ('prepared','failed') ORDER BY created_at DESC LIMIT 1").bind(&task.id).fetch_optional(&mut *tx).await.map_err(db_error)?;
    let Some(handoff) = handoff else {
        return Err(
            "No prepared verification exists. Record an implementation report first.".into(),
        );
    };
    let status = if input.passed { "passed" } else { "failed" };
    let details = serde_json::json!({"verificationSteps":input.verification_steps,"testResults":input.test_results,"evidence":input.evidence,"remainingFailures":input.remaining_failures,"userRecordedAsPassed":input.passed}).to_string();
    sqlx::query("UPDATE task_handoffs SET status=?,details_json=?,updated_at=datetime('now') WHERE id=? AND status IN ('prepared','failed')").bind(status).bind(&details).bind(&handoff).execute(&mut *tx).await.map_err(db_error)?;
    audit(
        &mut tx,
        &task.id,
        if input.passed {
            "verification_passed"
        } else {
            "verification_failed"
        },
        &details,
        Some(&handoff),
    )
    .await?;
    if input.passed {
        sqlx::query("UPDATE tasks SET status='completed',completed_at=datetime('now'),updated_at=datetime('now') WHERE id=? AND archived_at IS NULL AND status='review'").bind(&task.id).execute(&mut *tx).await.map_err(db_error)?;
        crate::workforce::release_claims(&mut tx, &task.id).await?;
        history(
            &mut tx,
            &task.id,
            "status",
            Some("review"),
            Some("completed"),
        )
        .await?;
        crate::workforce::insert_activity(
            &mut tx,
            "task_verified_and_completed",
            "Task marked completed after user-recorded passing verification",
            task.account_id.as_deref(),
            task.project_id.as_deref(),
            Some(&task.id),
        )
        .await?;
    } else {
        crate::workforce::insert_activity(
            &mut tx,
            "task_verification_failed",
            "Task verification was recorded with remaining failures",
            task.account_id.as_deref(),
            task.project_id.as_deref(),
            Some(&task.id),
        )
        .await?;
    }
    tx.commit().await.map_err(db_error)
}

async fn mark_handoff_started_transaction(
    pool: &SqlitePool,
    task_id: &str,
    handoff_id: &str,
) -> Result<(), String> {
    let mut tx = pool.begin_with("BEGIN IMMEDIATE").await.map_err(db_error)?;
    let task = task_context(&mut tx, task_id).await?;
    if task.status != "review" {
        return Err("Only a task in the review workflow can start a prepared handoff.".into());
    }
    let row: Option<(String, String, Option<String>)> =
        sqlx::query_as("SELECT kind,status,account_id FROM task_handoffs WHERE id=? AND task_id=?")
            .bind(handoff_id)
            .bind(task_id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(db_error)?;
    let Some((kind, status, original_account)) = row else {
        return Err("That handoff does not belong to this task.".into());
    };
    let expected =
        match kind.as_str() {
            "claude_revision" => "revision_required",
            "copilot_implementation" => "approved",
            _ => return Err(
                "Only Claude revision and Copilot implementation handoffs can be started manually."
                    .into(),
            ),
        };
    if status != expected || original_account != task.account_id {
        return Err(
            "This handoff is no longer startable or its original worker assignment changed.".into(),
        );
    }
    if kind == "claude_revision" {
        let worker: Option<(String,String,String,Option<String>,Option<String>)> = sqlx::query_as("SELECT a.status,b.availability_status,bp.availability_status,a.archived_at,bp.archived_at FROM accounts a JOIN browser_profiles bp ON bp.id=a.browser_profile_id JOIN browsers b ON b.id=bp.browser_id WHERE a.id=?").bind(task.account_id.as_deref().unwrap_or_default()).fetch_optional(&mut *tx).await.map_err(db_error)?;
        let Some((account, browser, profile, account_archived, profile_archived)) = worker else {
            return Err("The original worker or browser profile is unavailable.".into());
        };
        if account != "idle"
            || browser != "available"
            || profile != "available"
            || account_archived.is_some()
            || profile_archived.is_some()
        {
            return Err("The original worker must be Idle with an active, available browser profile before resuming the revision.".into());
        }
        sqlx::query(
            "UPDATE task_handoffs SET status='started',updated_at=datetime('now') WHERE id=?",
        )
        .bind(handoff_id)
        .execute(&mut *tx)
        .await
        .map_err(db_error)?;
        sqlx::query("UPDATE tasks SET status='in_progress',updated_at=datetime('now') WHERE id=? AND status='review'").bind(task_id).execute(&mut *tx).await.map_err(db_error)?;
        history(
            &mut tx,
            task_id,
            "status",
            Some("review"),
            Some("in_progress"),
        )
        .await?;
        audit(&mut tx,task_id,"claude_revision_started","User marked the prepared Claude revision handoff as started with the original worker. LEVI did not launch Claude.",Some(handoff_id)).await?;
    } else {
        sqlx::query(
            "UPDATE task_handoffs SET status='started',updated_at=datetime('now') WHERE id=?",
        )
        .bind(handoff_id)
        .execute(&mut *tx)
        .await
        .map_err(db_error)?;
        audit(&mut tx,task_id,"copilot_handoff_started","User marked the prepared Copilot handoff as started manually. LEVI did not launch Copilot.",Some(handoff_id)).await?;
    }
    tx.commit().await.map_err(db_error)
}

async fn resume_checkpoint_transaction(
    pool: &SqlitePool,
    checkpoint_id: &str,
    reset_verified: bool,
    user_authorized: bool,
) -> Result<(), String> {
    if !reset_verified || !user_authorized {
        return Err(
            "Verify that the original account is available and explicitly authorize resuming it."
                .into(),
        );
    }
    let mut tx = pool.begin_with("BEGIN IMMEDIATE").await.map_err(db_error)?;
    let row: Option<(String,String,String,String)> = sqlx::query_as("SELECT c.id,c.task_id,c.account_id,t.status FROM task_checkpoints c JOIN tasks t ON t.id=c.task_id WHERE c.id=? AND c.task_id IS NOT NULL AND c.account_id IS NOT NULL").bind(checkpoint_id).fetch_optional(&mut *tx).await.map_err(db_error)?;
    let Some((_, task_id, account_id, task_status)) = row else {
        return Err(
            "This checkpoint is not linked to a resumable task and original worker.".into(),
        );
    };
    if task_status != "waiting" {
        return Err("Only a task waiting for its original worker's usage reset can be resumed from this checkpoint.".into());
    }
    let worker: Option<(String,String,String,Option<String>,Option<String>)> = sqlx::query_as("SELECT a.status,b.availability_status,bp.availability_status,a.archived_at,bp.archived_at FROM accounts a JOIN browser_profiles bp ON bp.id=a.browser_profile_id JOIN browsers b ON b.id=bp.browser_id WHERE a.id=?").bind(&account_id).fetch_optional(&mut *tx).await.map_err(db_error)?;
    let Some((account_status, browser_status, profile_status, account_archived, profile_archived)) =
        worker
    else {
        return Err("The original worker or browser profile is unavailable.".into());
    };
    if account_status != "waiting"
        || browser_status != "available"
        || profile_status != "available"
        || account_archived.is_some()
        || profile_archived.is_some()
    {
        return Err("The original account must be waiting with an active, available browser profile before resuming.".into());
    }
    let event: Option<(String,Option<String>)> = sqlx::query_as("SELECT id,task_id FROM usage_limit_events WHERE checkpoint_id=? AND account_id=? AND status='waiting'").bind(checkpoint_id).bind(&account_id).fetch_optional(&mut *tx).await.map_err(db_error)?;
    let Some((event_id, Some(event_task))) = event else {
        return Err("No unresolved usage-limit event is linked to this checkpoint.".into());
    };
    if event_task != task_id {
        return Err("The usage-limit event no longer points to this checkpoint's task.".into());
    }
    sqlx::query("UPDATE tasks SET status='ready',updated_at=datetime('now') WHERE id=? AND account_id=? AND status='waiting'").bind(&task_id).bind(&account_id).execute(&mut *tx).await.map_err(db_error)?;
    sqlx::query("UPDATE usage_limit_events SET status='resolved' WHERE id=? AND status='waiting'")
        .bind(&event_id)
        .execute(&mut *tx)
        .await
        .map_err(db_error)?;
    let account_name: String = sqlx::query_scalar("SELECT name FROM accounts WHERE id=?")
        .bind(&account_id)
        .fetch_one(&mut *tx)
        .await
        .map_err(db_error)?;
    sqlx::query("UPDATE reminders SET status='completed',completed_at=datetime('now') WHERE account_id=? AND status IN ('pending','snoozed') AND title IN (?,?)").bind(&account_id).bind(format!("Check {account_name} usage reset")).bind(format!("Check {account_name} usage availability")).execute(&mut *tx).await.map_err(db_error)?;
    sqlx::query("UPDATE accounts SET status='idle',updated_at=datetime('now') WHERE id=?")
        .bind(&account_id)
        .execute(&mut *tx)
        .await
        .map_err(db_error)?;
    history(&mut tx, &task_id, "status", Some("waiting"), Some("ready")).await?;
    crate::workforce::insert_activity(
        &mut tx,
        "checkpoint_resumed",
        "User verified availability and authorized same-account task resume from checkpoint",
        Some(&account_id),
        None,
        Some(&task_id),
    )
    .await?;
    tx.commit().await.map_err(db_error)
}

async fn insert_checkpoint(
    tx: &mut Transaction<'_, sqlx::Sqlite>,
    task_id: &str,
    summary: &str,
    remaining: &str,
    files: &str,
    blockers: &str,
    next: &str,
) -> Result<String, String> {
    let ctx: Option<(Option<String>,Option<String>,String,String,String,Option<String>,Option<String>)> = sqlx::query_as("SELECT t.project_id,t.account_id,t.title,COALESCE(p.name,''),COALESCE(a.name,''),bp.id,bp.profile_name FROM tasks t LEFT JOIN projects p ON p.id=t.project_id LEFT JOIN accounts a ON a.id=t.account_id LEFT JOIN browser_profiles bp ON bp.id=a.browser_profile_id WHERE t.id=? AND t.archived_at IS NULL").bind(task_id).fetch_optional(&mut **tx).await.map_err(db_error)?;
    let Some((project_id, account_id, title, project, account, profile_id, profile)) = ctx else {
        return Err("That task no longer exists or is archived.".into());
    };
    let id = Uuid::new_v4().to_string();
    sqlx::query("INSERT INTO task_checkpoints(id,task_id,account_id,summary,remaining_work,files_changed,last_successful_step,next_action,blockers,project_id,browser_profile_id,project_name_snapshot,task_title_snapshot,account_name_snapshot,browser_profile_name_snapshot) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)")
        .bind(&id).bind(task_id).bind(account_id).bind(summary).bind(remaining).bind(files).bind(summary).bind(next).bind(blockers).bind(project_id).bind(profile_id).bind(project).bind(title).bind(account).bind(profile.unwrap_or_default()).execute(&mut **tx).await.map_err(db_error)?;
    Ok(id)
}

#[derive(sqlx::FromRow)]
struct TaskContext {
    id: String,
    project_id: Option<String>,
    account_id: Option<String>,
    title: String,
    description: String,
    instructions: String,
    status: String,
    project_name: String,
    account_name: String,
    profile_name: String,
}
async fn task_context(
    tx: &mut Transaction<'_, sqlx::Sqlite>,
    id: &str,
) -> Result<TaskContext, String> {
    let task:Option<TaskContext>=sqlx::query_as("SELECT t.id,t.project_id,t.account_id,t.title,t.description,t.instructions,t.status,COALESCE(p.name,'') AS project_name,COALESCE(a.name,'') AS account_name,COALESCE(bp.profile_name,'') AS profile_name FROM tasks t LEFT JOIN projects p ON p.id=t.project_id LEFT JOIN accounts a ON a.id=t.account_id LEFT JOIN browser_profiles bp ON bp.id=a.browser_profile_id WHERE t.id=? AND t.archived_at IS NULL").bind(id).fetch_optional(&mut **tx).await.map_err(db_error)?;
    let Some(task) = task else {
        return Err("That task no longer exists or is archived.".into());
    };
    if task.account_id.is_none() {
        return Err(
            "Assign this task to a worker before starting a completion or handoff workflow.".into(),
        );
    }
    Ok(task)
}
async fn insert_handoff(
    tx: &mut Transaction<'_, sqlx::Sqlite>,
    task: &TaskContext,
    checkpoint: Option<&str>,
    kind: &str,
    status: &str,
    package: &str,
    details: &str,
) -> Result<String, String> {
    let id = Uuid::new_v4().to_string();
    sqlx::query("INSERT INTO task_handoffs(id,task_id,account_id,checkpoint_id,kind,status,package_text,details_json,account_name_snapshot,browser_profile_name_snapshot) VALUES(?,?,?,?,?,?,?,?,?,?)").bind(&id).bind(&task.id).bind(&task.account_id).bind(checkpoint).bind(kind).bind(status).bind(package).bind(details).bind(&task.account_name).bind(&task.profile_name).execute(&mut **tx).await.map_err(db_error)?;
    sqlx::query("INSERT INTO task_handoff_events(id,handoff_id,task_id,account_id,event_type,details_json) VALUES(?,?,?,?,?,?)").bind(Uuid::new_v4().to_string()).bind(&id).bind(&task.id).bind(&task.account_id).bind(format!("{kind}_prepared")).bind(details).execute(&mut **tx).await.map_err(db_error)?;
    Ok(id)
}
async fn audit(
    tx: &mut Transaction<'_, sqlx::Sqlite>,
    task_id: &str,
    kind: &str,
    details: &str,
    handoff: Option<&str>,
) -> Result<(), String> {
    if let Some(id) = handoff {
        let task = task_context(tx, task_id).await?;
        sqlx::query("INSERT INTO task_handoff_events(id,handoff_id,task_id,account_id,event_type,details_json) VALUES(?,?,?,?,?,?)").bind(Uuid::new_v4().to_string()).bind(id).bind(task_id).bind(&task.account_id).bind(kind).bind(details).execute(&mut **tx).await.map_err(db_error)?;
        crate::workforce::insert_activity(
            tx,
            kind,
            details,
            task.account_id.as_deref(),
            task.project_id.as_deref(),
            Some(task_id),
        )
        .await?;
    } else {
        let (account, project): (Option<String>, Option<String>) =
            sqlx::query_as("SELECT account_id,project_id FROM tasks WHERE id=?")
                .bind(task_id)
                .fetch_one(&mut **tx)
                .await
                .map_err(db_error)?;
        crate::workforce::insert_activity(
            tx,
            kind,
            details,
            account.as_deref(),
            project.as_deref(),
            Some(task_id),
        )
        .await?;
    }
    Ok(())
}
async fn insert_deliverable(
    tx: &mut Transaction<'_, sqlx::Sqlite>,
    task: &TaskContext,
    checkpoint: &str,
    details: &str,
) -> Result<(), String> {
    sqlx::query("INSERT INTO deliverables(id,title,project_id,account_id,task_id,notes) VALUES(?,?,?,?,?,?)").bind(Uuid::new_v4().to_string()).bind(format!("Work report: {}",task.title)).bind(&task.project_id).bind(&task.account_id).bind(&task.id).bind(format!("Checkpoint {checkpoint}\n\n{details}")).execute(&mut **tx).await.map_err(db_error)?;
    Ok(())
}
async fn set_task_status(
    tx: &mut Transaction<'_, sqlx::Sqlite>,
    task: &TaskContext,
    status: &str,
) -> Result<(), String> {
    sqlx::query(
        "UPDATE tasks SET status=?,updated_at=datetime('now'),completed_at=NULL WHERE id=?",
    )
    .bind(status)
    .bind(&task.id)
    .execute(&mut **tx)
    .await
    .map_err(db_error)?;
    history(tx, &task.id, "status", Some(&task.status), Some(status)).await?;
    crate::workforce::insert_activity(
        tx,
        "task_submitted_for_review",
        "Task completion report saved and task moved to review",
        task.account_id.as_deref(),
        task.project_id.as_deref(),
        Some(&task.id),
    )
    .await?;
    Ok(())
}
async fn history(
    tx: &mut Transaction<'_, sqlx::Sqlite>,
    task: &str,
    field: &str,
    old: Option<&str>,
    new: Option<&str>,
) -> Result<(), String> {
    sqlx::query("INSERT INTO task_history(id,task_id,field,old_value,new_value,source) VALUES(?,?,?,?,?,'manual')").bind(Uuid::new_v4().to_string()).bind(task).bind(field).bind(old).bind(new).execute(&mut **tx).await.map_err(db_error)?;
    Ok(())
}
fn review_package(task: &TaskContext, i: &CompletionReportInput) -> String {
    format!("# ChatGPT review package — manual\n\n## Project\n{}\n\n## Task\n{}\n\n## Task description\n{}\n\n## Original requirements\n{}\n\n## Work completed (reported)\n{}\n\n## Remaining work\n{}\n\n## Files changed (reported)\n{}\n\n## Tests executed (reported)\n{}\n\n## Test results (reported)\n{}\n\n## Known issues\n{}\n\n## Blockers\n{}\n\n## Next action\n{}\n\n## Deliverable references\n{}\n\n## Questions requiring review\n{}\n\nGenerated from local records and user-entered report fields. No external AI review has occurred.",task.project_name,task.title,task.description,task.instructions,i.completed_work,i.remaining_work,i.modified_files,i.tests_executed,i.test_results,i.known_issues,i.blockers,i.next_action,i.deliverable_refs,i.review_questions)
}
fn validate_report(i: &CompletionReportInput) -> Result<(), String> {
    required("Completed work", &i.completed_work, 12_000)?;
    for (label, value) in [
        ("Remaining work", &i.remaining_work),
        ("Modified files", &i.modified_files),
        ("Blockers", &i.blockers),
        ("Next action", &i.next_action),
        ("Tests executed", &i.tests_executed),
        ("Test results", &i.test_results),
        ("Known issues", &i.known_issues),
        ("Deliverable references", &i.deliverable_refs),
        ("Review questions", &i.review_questions),
    ] {
        optional(label, value, 8_000)?
    }
    Ok(())
}
fn validate_checkpoint(
    summary: &str,
    remaining: &str,
    files: &str,
    blockers: &str,
    next: &str,
) -> Result<(), String> {
    required("Completed work", summary, 12_000)?;
    for (label, value) in [
        ("Remaining work", remaining),
        ("Modified files", files),
        ("Blockers", blockers),
        ("Next action", next),
    ] {
        optional(label, value, 8_000)?
    }
    Ok(())
}
fn required(label: &str, value: &str, max: usize) -> Result<(), String> {
    if value.trim().is_empty() || value.chars().count() > max {
        Err(format!("{label} must contain 1–{max} characters."))
    } else {
        Ok(())
    }
}
fn optional(label: &str, value: &str, max: usize) -> Result<(), String> {
    if value.chars().count() > max {
        Err(format!("{label} must be no longer than {max} characters."))
    } else {
        Ok(())
    }
}
fn db_error(e: sqlx::Error) -> String {
    format!("Database operation failed: {e}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
    use std::str::FromStr;

    async fn db() -> SqlitePool {
        let options = SqliteConnectOptions::from_str("sqlite::memory:")
            .unwrap()
            .foreign_keys(true);
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(options)
            .await
            .unwrap();
        for sql in [
            include_str!("../migrations/001_initial.sql"),
            include_str!("../migrations/002_browser_registry.sql"),
            include_str!("../migrations/003_workforce_checkpoints.sql"),
            include_str!("../migrations/004_task_queue_dependencies_and_claims.sql"),
            include_str!("../migrations/005_task_file_releases.sql"),
            include_str!("../migrations/006_reminder_delivery.sql"),
            include_str!("../migrations/007_checkpoints_and_handoffs.sql"),
        ] {
            sqlx::raw_sql(sql).execute(&pool).await.unwrap();
        }
        sqlx::query("INSERT INTO browsers(id,name,executable_path,family,availability_status) VALUES('b','Brave','C:/Brave.exe','brave','available')").execute(&pool).await.unwrap();
        sqlx::query("INSERT INTO browser_profiles(id,browser_id,profile_name,profile_path,detection_method,availability_status) VALUES('bp','b','Test Profile','C:/Temp/Profile','manual','available')").execute(&pool).await.unwrap();
        sqlx::query("INSERT INTO projects(id,name,status) VALUES('p','Test Project','active')")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO accounts(id,name,browser_profile_id,status) VALUES('a','Test Worker','bp','idle')").execute(&pool).await.unwrap();
        sqlx::query("INSERT INTO project_accounts(project_id,account_id) VALUES('p','a')")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO tasks(id,project_id,account_id,title,instructions,status) VALUES('t','p','a','Test task','Build and verify the feature','in_progress')").execute(&pool).await.unwrap();
        pool
    }
    fn completion() -> CompletionReportInput {
        CompletionReportInput {
            task_id: "t".into(),
            completed_work: "Implemented the feature".into(),
            remaining_work: "Review UX".into(),
            modified_files: "src/App.tsx".into(),
            blockers: String::new(),
            next_action: "Review".into(),
            tests_executed: "cargo test".into(),
            test_results: "42 passed".into(),
            known_issues: "Toast unverified".into(),
            deliverable_refs: "src/App.tsx".into(),
            review_questions: "Does this meet the criteria?".into(),
        }
    }
    async fn report(pool: &SqlitePool) -> String {
        report_completion_transaction(pool, &completion())
            .await
            .unwrap()
    }
    async fn record(pool: &SqlitePool, decision: &str) -> String {
        record_review_feedback_transaction(
            pool,
            &ReviewFeedbackInput {
                task_id: "t".into(),
                decision: decision.into(),
                feedback: "Header overflows at 375px".into(),
                required_changes: "Fix the narrow header".into(),
                acceptance_criteria: "No overflow at 375px".into(),
            },
        )
        .await
        .unwrap()
    }

    #[tokio::test]
    async fn checkpoint_persists_searchable_assignment_snapshots() {
        let pool = db().await;
        let id = save_checkpoint_transaction(
            &pool,
            &CheckpointInput {
                task_id: "t".into(),
                completed_work: "Step passed".into(),
                remaining_work: "Next step".into(),
                modified_files: "src/main.rs".into(),
                blockers: String::new(),
                next_action: "Run tests".into(),
            },
        )
        .await
        .unwrap();
        let row:CheckpointRecord=sqlx::query_as("SELECT id,task_id,account_id,project_id,browser_profile_id,task_title_snapshot,project_name_snapshot,account_name_snapshot,browser_profile_name_snapshot,summary,remaining_work,files_changed,last_successful_step,next_action,blockers,created_at FROM task_checkpoints WHERE id=?").bind(id).fetch_one(&pool).await.unwrap();
        assert_eq!(row.project_name_snapshot, "Test Project");
        assert_eq!(row.task_title_snapshot, "Test task");
        assert_eq!(row.account_name_snapshot, "Test Worker");
        assert_eq!(row.browser_profile_name_snapshot, "Test Profile");
    }

    #[tokio::test]
    async fn completion_persists_checkpoint_deliverable_review_package_and_task_transition() {
        let pool = db().await;
        let id = report(&pool).await;
        assert_eq!(
            sqlx::query_scalar::<_, String>("SELECT status FROM tasks WHERE id='t'")
                .fetch_one(&pool)
                .await
                .unwrap(),
            "review"
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM task_checkpoints WHERE task_id='t'")
                .fetch_one(&pool)
                .await
                .unwrap(),
            1
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM deliverables WHERE task_id='t'")
                .fetch_one(&pool)
                .await
                .unwrap(),
            1
        );
        let package: String =
            sqlx::query_scalar("SELECT package_text FROM task_handoffs WHERE id=?")
                .bind(id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert!(package.contains("No external AI review has occurred"));
    }

    #[tokio::test]
    async fn handoff_event_failure_rolls_back_completion_and_activity() {
        let pool = db().await;
        sqlx::query("CREATE TRIGGER reject_handoff BEFORE INSERT ON task_handoff_events BEGIN SELECT RAISE(ABORT,'forced'); END").execute(&pool).await.unwrap();
        assert!(report_completion_transaction(&pool, &completion())
            .await
            .is_err());
        for table in [
            "task_checkpoints",
            "task_handoffs",
            "deliverables",
            "activity_logs",
        ] {
            let count: i64 = sqlx::query_scalar(&format!("SELECT COUNT(*) FROM {table}"))
                .fetch_one(&pool)
                .await
                .unwrap();
            assert_eq!(count, 0, "{table} left a partial row");
        }
        assert_eq!(
            sqlx::query_scalar::<_, String>("SELECT status FROM tasks WHERE id='t'")
                .fetch_one(&pool)
                .await
                .unwrap(),
            "in_progress"
        );
    }

    #[tokio::test]
    async fn revision_handoff_keeps_original_account_and_feedback() {
        let pool = db().await;
        report(&pool).await;
        let id = record(&pool, "revision_required").await;
        let (row, account, package): (String, String, String) =
            sqlx::query_as("SELECT kind,account_id,package_text FROM task_handoffs WHERE id=?")
                .bind(id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(row, "claude_revision");
        assert_eq!(account, "a");
        assert!(package.contains("original assigned account"));
        assert!(package.contains("Fix the narrow header"));
    }

    #[tokio::test]
    async fn approved_review_generates_copilot_then_verification_and_only_passing_verification_completes(
    ) {
        let pool = db().await;
        report(&pool).await;
        let id = record(&pool, "approved").await;
        assert_eq!(
            sqlx::query_scalar::<_, String>("SELECT kind FROM task_handoffs WHERE id=?")
                .bind(id)
                .fetch_one(&pool)
                .await
                .unwrap(),
            "copilot_implementation"
        );
        let verification = record_implementation_report_transaction(
            &pool,
            &ImplementationReportInput {
                task_id: "t".into(),
                completed_work: "Changes implemented".into(),
                modified_files: "src/App.tsx".into(),
                tests_executed: "npm test".into(),
                test_results: "Passed".into(),
                known_issues: String::new(),
                evidence: "Run log".into(),
            },
        )
        .await
        .unwrap();
        assert_eq!(
            sqlx::query_scalar::<_, String>("SELECT kind FROM task_handoffs WHERE id=?")
                .bind(&verification)
                .fetch_one(&pool)
                .await
                .unwrap(),
            "verification"
        );
        assert!(record_verification_transaction(
            &pool,
            &VerificationInput {
                task_id: "t".into(),
                passed: true,
                verification_steps: "Run checks".into(),
                test_results: "Passed".into(),
                evidence: String::new(),
                remaining_failures: "one failure".into()
            }
        )
        .await
        .is_err());
        assert_eq!(
            sqlx::query_scalar::<_, String>("SELECT status FROM tasks WHERE id='t'")
                .fetch_one(&pool)
                .await
                .unwrap(),
            "review"
        );
        record_verification_transaction(
            &pool,
            &VerificationInput {
                task_id: "t".into(),
                passed: false,
                verification_steps: "Run checks".into(),
                test_results: "One check failed".into(),
                evidence: "Failed CI result".into(),
                remaining_failures: "One failure".into(),
            },
        )
        .await
        .unwrap();
        assert_eq!(
            sqlx::query_scalar::<_, String>("SELECT status FROM task_handoffs WHERE id=?")
                .bind(&verification)
                .fetch_one(&pool)
                .await
                .unwrap(),
            "failed"
        );
        record_verification_transaction(
            &pool,
            &VerificationInput {
                task_id: "t".into(),
                passed: true,
                verification_steps: "Run checks".into(),
                test_results: "Passed".into(),
                evidence: "CI result".into(),
                remaining_failures: String::new(),
            },
        )
        .await
        .unwrap();
        assert_eq!(
            sqlx::query_scalar::<_, String>("SELECT status FROM tasks WHERE id='t'")
                .fetch_one(&pool)
                .await
                .unwrap(),
            "completed"
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT count(*) FROM task_handoff_events WHERE task_id='t' AND event_type IN ('verification_failed','verification_passed')")
                .fetch_one(&pool)
                .await
                .unwrap(),
            2
        );
    }

    #[tokio::test]
    async fn invalid_transitions_are_rejected() {
        let pool = db().await;
        assert!(record_review_feedback_transaction(
            &pool,
            &ReviewFeedbackInput {
                task_id: "t".into(),
                decision: "approved".into(),
                feedback: "Feedback".into(),
                required_changes: String::new(),
                acceptance_criteria: String::new()
            }
        )
        .await
        .is_err());
        assert!(record_verification_transaction(
            &pool,
            &VerificationInput {
                task_id: "t".into(),
                passed: true,
                verification_steps: "Check".into(),
                test_results: "Pass".into(),
                evidence: String::new(),
                remaining_failures: String::new()
            }
        )
        .await
        .is_err());
    }

    #[tokio::test]
    async fn revision_start_requires_original_available_worker_and_preserves_assignment() {
        let pool = db().await;
        report(&pool).await;
        let id = record(&pool, "revision_required").await;
        mark_handoff_started_transaction(&pool, "t", &id)
            .await
            .unwrap();
        let row:(String,String,String)=sqlx::query_as("SELECT status,account_id,(SELECT status FROM task_handoffs WHERE id=?) FROM tasks WHERE id='t'").bind(&id).fetch_one(&pool).await.unwrap();
        assert_eq!(row.0, "in_progress");
        assert_eq!(row.1, "a");
        assert_eq!(row.2, "started");
    }

    #[tokio::test]
    async fn checkpoint_resume_is_scoped_to_its_usage_event_and_requires_authorization() {
        let pool = db().await;
        let checkpoint = save_checkpoint_transaction(
            &pool,
            &CheckpointInput {
                task_id: "t".into(),
                completed_work: "Saved before reset".into(),
                remaining_work: "Continue later".into(),
                modified_files: "src/App.tsx".into(),
                blockers: "Limit reached".into(),
                next_action: "Resume with same account".into(),
            },
        )
        .await
        .unwrap();
        sqlx::query("UPDATE tasks SET status='waiting' WHERE id='t'")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("UPDATE accounts SET status='waiting' WHERE id='a'")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO usage_limit_events(id,account_id,task_id,checkpoint_id,exact_message,reset_status) VALUES('event','a','t',?,'Limit reached','unknown')").bind(&checkpoint).execute(&pool).await.unwrap();
        assert!(
            resume_checkpoint_transaction(&pool, &checkpoint, false, true)
                .await
                .is_err()
        );
        resume_checkpoint_transaction(&pool, &checkpoint, true, true)
            .await
            .unwrap();
        let task: (String, String) =
            sqlx::query_as("SELECT status,account_id FROM tasks WHERE id='t'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(task.0, "ready");
        assert_eq!(task.1, "a");
        assert_eq!(
            sqlx::query_scalar::<_, String>(
                "SELECT status FROM usage_limit_events WHERE id='event'"
            )
            .fetch_one(&pool)
            .await
            .unwrap(),
            "resolved"
        );
    }
}
