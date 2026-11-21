use crate::db::database;
use keyring::Entry;
use reqwest::{redirect::Policy, Client, StatusCode};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use sqlx::{Row, Sqlite, SqlitePool, Transaction};
use std::collections::HashMap;
use std::future::Future;
use std::sync::{Mutex, OnceLock};
use tauri::AppHandle;
use tokio::sync::oneshot;
use url::{Host, Url};
use uuid::Uuid;

const CREDENTIAL_SERVICE: &str = "com.local.claude-command-center.ai";
const OPENROUTER_BASE_URL: &str = "https://openrouter.ai/api/v1";
const MAX_CHAT_CHARS: usize = 12_000;
const MAX_CONTEXT_CHARS: usize = 18_000;
const MAX_PROVIDER_OUTPUT_TOKENS: u32 = 4096;
static ACTIVE_AI_CANCELLATIONS: OnceLock<Mutex<HashMap<String, oneshot::Sender<()>>>> =
    OnceLock::new();

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiConfigInput {
    #[serde(default = "default_primary_slot")]
    slot: String,
    provider: String,
    base_url: String,
    model: String,
    #[serde(default)]
    api_key: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AiConfig {
    slot: String,
    provider: String,
    base_url: String,
    model: String,
    has_api_key: bool,
    credential_store_available: bool,
    credential_status: String,
    credential_store_error: Option<String>,
    fallback_enabled: bool,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct Conversation {
    id: String,
    title: String,
    created_at: String,
    updated_at: String,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct AiMessage {
    id: String,
    conversation_id: String,
    role: String,
    response_kind: String,
    request_status: Option<String>,
    content: String,
    provider: Option<String>,
    model: Option<String>,
    created_at: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionResult {
    connected: bool,
    message: String,
    provider: String,
    model: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderModel {
    id: String,
    display_name: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SendMessageInput {
    conversation_id: String,
    message: String,
    request_id: String,
    #[serde(default = "default_primary_slot")]
    provider_slot: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HandoffPromptInput {
    conversation_id: String,
    task_id: String,
    kind: String,
}

#[derive(Clone)]
struct ProviderConfig {
    provider: String,
    base_url: String,
    model: String,
    slot: String,
}

impl ProviderConfig {
    fn display_name(&self) -> String {
        format!("{} · {}", self.provider, self.slot)
    }
}

fn default_primary_slot() -> String {
    "primary".into()
}

fn normalize_provider_slot(slot: &str) -> Result<&'static str, String> {
    match slot.trim() {
        "primary" => Ok("primary"),
        "secondary" => Ok("secondary"),
        _ => Err("Select the primary or secondary provider profile.".into()),
    }
}

#[derive(Debug)]
struct ProviderFailure {
    message: String,
    outcome_unknown: bool,
}

struct ProviderCompletion {
    text: String,
    finish_reason: Option<String>,
    prompt_tokens: Option<u64>,
    completion_tokens: Option<u64>,
    total_tokens: Option<u64>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ChatLine {
    role: String,
    content: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum ProposedAction {
    CreateTask {
        project_id: String,
        title: String,
        description: String,
        priority: i64,
    },
    CreateReminder {
        title: String,
        body: String,
        due_at: String,
        project_id: Option<String>,
        task_id: Option<String>,
    },
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ActionEnvelope {
    version: u8,
    summary: String,
    action: ProposedAction,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct AiActionProposal {
    id: String,
    conversation_id: String,
    assistant_message_id: String,
    action_kind: String,
    arguments_json: String,
    content_hash: String,
    status: String,
    last_error: Option<String>,
    result_json: Option<String>,
    created_at: String,
    decided_at: Option<String>,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct AiRequestRecovery {
    request_id: String,
    status: String,
    user_message: String,
    lease_expires_at: Option<String>,
    recovery_attempts: i64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AiRequestRecoverySummary {
    recovered: u64,
    active: Vec<AiRequestRecovery>,
}

#[tauri::command]
pub async fn get_ai_config(app: AppHandle) -> Result<AiConfig, String> {
    get_config(&database(&app).await?, "primary").await
}

#[tauri::command]
pub async fn get_ai_secondary_config(app: AppHandle) -> Result<AiConfig, String> {
    get_config(&database(&app).await?, "secondary").await
}

#[tauri::command]
pub async fn set_ai_fallback_enabled(app: AppHandle, enabled: bool) -> Result<bool, String> {
    let pool = database(&app).await?;
    persist_manual_fallback_setting(&pool, enabled).await?;
    Ok(enabled)
}

#[tauri::command]
pub async fn save_ai_config(app: AppHandle, input: AiConfigInput) -> Result<AiConfig, String> {
    let slot = normalize_provider_slot(&input.slot)?;
    let config = if input.model.trim().is_empty() {
        let (provider, base_url) = validate_provider_identity(&input.provider, &input.base_url)?;
        ProviderConfig {
            provider,
            base_url,
            model: String::new(),
            slot: slot.into(),
        }
    } else {
        let mut config = validate_config(&input.provider, &input.base_url, &input.model)?;
        config.slot = slot.into();
        config
    };
    if let Some(key) = input
        .api_key
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        save_secret(&config, slot, key)?;
    }
    let pool = database(&app).await?;
    persist_provider_settings(&pool, slot, &config).await?;
    get_config(&pool, slot).await
}

async fn persist_provider_settings(
    pool: &SqlitePool,
    slot: &str,
    config: &ProviderConfig,
) -> Result<(), String> {
    let mut tx = pool.begin_with("BEGIN IMMEDIATE").await.map_err(db_error)?;
    let (provider_key, url_key, model_key) = provider_setting_keys(slot);
    for (key, value) in [
        (provider_key, config.provider.as_str()),
        (url_key, config.base_url.as_str()),
        (model_key, config.model.as_str()),
    ] {
        sqlx::query("INSERT INTO settings(key,value,updated_at) VALUES(?,?,datetime('now')) ON CONFLICT(key) DO UPDATE SET value=excluded.value,updated_at=datetime('now')")
            .bind(key).bind(value).execute(&mut *tx).await.map_err(db_error)?;
    }
    tx.commit().await.map_err(db_error)?;
    Ok(())
}

async fn persist_manual_fallback_setting(pool: &SqlitePool, enabled: bool) -> Result<(), String> {
    sqlx::query("INSERT INTO settings(key,value,updated_at) VALUES('ai_manual_fallback_enabled',?,datetime('now')) ON CONFLICT(key) DO UPDATE SET value=excluded.value,updated_at=datetime('now')")
        .bind(if enabled { "true" } else { "false" })
        .execute(pool)
        .await
        .map_err(db_error)?;
    Ok(())
}

#[tauri::command]
pub async fn remove_ai_api_key(app: AppHandle, slot: String) -> Result<(), String> {
    // Key removal must remain available while model discovery is still pending.
    let slot = normalize_provider_slot(&slot)?;
    let config = read_provider_settings(&database(&app).await?, slot).await?;
    let entry = credential_entry(&config, slot)?;
    match entry.delete_credential() {
        Ok(()) => Ok(()),
        Err(keyring::Error::NoEntry) => Ok(()),
        Err(_) => Err("Windows Credential Manager could not remove the LEVI provider key.".into()),
    }
}

#[tauri::command]
pub async fn test_ai_connection(app: AppHandle, slot: String) -> Result<ConnectionResult, String> {
    let slot = normalize_provider_slot(&slot)?;
    let config = read_provider_config(&database(&app).await?, slot).await?;
    let key = load_secret(&config, slot)?;
    let output = send_provider_request(
        &config,
        &key,
        &[ChatLine {
            role: "user".into(),
            content: "Reply with the single word OK.".into(),
        }],
        "You are checking API connectivity. Reply briefly.",
    )
    .await
    .map_err(|failure| failure.message)?;
    if output.trim().is_empty() {
        return Err("The provider accepted the request but returned no text. Check the selected model and endpoint.".into());
    }
    Ok(ConnectionResult {
        connected: true,
        message: "Authenticated test request succeeded. A small provider charge may apply.".into(),
        provider: config.provider,
        model: config.model,
    })
}

#[tauri::command]
pub async fn list_ai_models(app: AppHandle, slot: String) -> Result<Vec<ProviderModel>, String> {
    let slot = normalize_provider_slot(&slot)?;
    let pool = database(&app).await?;
    let config = read_provider_settings(&pool, slot).await?;
    let key = load_secret(&config, slot)?;
    list_provider_models(&config, &key).await
}

#[tauri::command]
pub async fn cancel_ai_request(request_id: String) -> Result<(), String> {
    let id = Uuid::parse_str(request_id.trim())
        .map_err(|_| "The active request ID is invalid.".to_owned())?
        .to_string();
    let registry = ACTIVE_AI_CANCELLATIONS.get_or_init(|| Mutex::new(HashMap::new()));
    let sender = registry
        .lock()
        .map_err(|_| "The request cancellation registry is unavailable.".to_owned())?
        .remove(&id)
        .ok_or_else(|| "This request is no longer active or cannot be cancelled.".to_owned())?;
    sender
        .send(())
        .map_err(|_| "This request has already finished.".to_owned())
}

#[tauri::command]
pub async fn create_ai_conversation(app: AppHandle) -> Result<Conversation, String> {
    let pool = database(&app).await?;
    let id = Uuid::new_v4().to_string();
    sqlx::query("INSERT INTO ai_conversations(id) VALUES(?)")
        .bind(&id)
        .execute(&pool)
        .await
        .map_err(db_error)?;
    sqlx::query_as("SELECT id,title,created_at,updated_at FROM ai_conversations WHERE id=?")
        .bind(id)
        .fetch_one(&pool)
        .await
        .map_err(db_error)
}

#[tauri::command]
pub async fn list_ai_conversations(app: AppHandle) -> Result<Vec<Conversation>, String> {
    sqlx::query_as("SELECT id,title,created_at,updated_at FROM ai_conversations ORDER BY updated_at DESC,created_at DESC LIMIT 100")
        .fetch_all(&database(&app).await?).await.map_err(db_error)
}

#[tauri::command]
pub async fn get_ai_messages(
    app: AppHandle,
    conversation_id: String,
) -> Result<Vec<AiMessage>, String> {
    sqlx::query_as("SELECT m.id,m.conversation_id,m.role,m.response_kind,m.content,m.provider,m.model,m.created_at,r.status AS request_status FROM ai_messages m LEFT JOIN ai_requests r ON r.assistant_message_id=m.id WHERE m.conversation_id=? ORDER BY m.rowid LIMIT 500")
        .bind(conversation_id).fetch_all(&database(&app).await?).await.map_err(db_error)
}

#[tauri::command]
pub async fn send_ai_message(app: AppHandle, input: SendMessageInput) -> Result<AiMessage, String> {
    let message = input.message.trim();
    if message.is_empty() || message.chars().count() > MAX_CHAT_CHARS {
        return Err(format!(
            "Message must contain 1–{MAX_CHAT_CHARS} characters."
        ));
    }
    let pool = database(&app).await?;
    let slot = normalize_provider_slot(&input.provider_slot)?;
    let config = read_provider_config(&pool, slot).await?;
    let key = load_secret(&config, slot)?;
    let (context, history) = load_chat_context(&pool, &input.conversation_id).await?;
    let request_id = Uuid::parse_str(input.request_id.trim())
        .map_err(|_| "Message request ID is invalid. Retry sending the message.".to_owned())?
        .to_string();
    let request_hash = request_payload_hash_for_provider(&input.conversation_id, message, &config)?;
    if let Some(existing) = begin_ai_request(
        &pool,
        &request_id,
        &input.conversation_id,
        message,
        &request_hash,
        &config,
    )
    .await?
    {
        return Ok(existing);
    }

    let mut lines = history;
    lines.push(ChatLine {
        role: "user".into(),
        content: message.to_owned(),
    });
    let system = format!("You are LEVI Command Intelligence for LEVI COMMAND CENTER. Use only the supplied saved workspace snapshot when making claims about projects, tasks, accounts, reminders, activity, and deliverables. State uncertainty where records are missing. Never claim an operation was performed unless the saved data proves it. You have no SQL, shell, PowerShell, browser, account-assignment, deletion, or external-service tools.\n\nIf the user clearly requests creating a task or one-time reminder and all required details are unambiguous, you may propose exactly one typed action using this exact format and no code fences:\nLEVI_ACTION_PROPOSAL: {{\"version\":1,\"summary\":\"plain-language description\",\"action\":{{\"kind\":\"create_task\",\"project_id\":\"an active project ID from the snapshot\",\"title\":\"...\",\"description\":\"...\",\"priority\":2}}}}\nOr use kind create_reminder with title, body, due_at (ISO date-time with timezone), optional project_id and optional task_id. Do not propose any other action kind. Do not claim it is executed; the desktop user must review and approve it. If details or target IDs are unclear, ask a question in ordinary text and make no proposal.\n\nCURRENT SAVED WORKSPACE SNAPSHOT (read-only, curated by the Rust backend):\n{}", context);
    let assistant_id = Uuid::new_v4().to_string();
    let (cancel_sender, cancel_receiver) = oneshot::channel();
    let registry = ACTIVE_AI_CANCELLATIONS.get_or_init(|| Mutex::new(HashMap::new()));
    registry
        .lock()
        .map_err(|_| "The request cancellation registry is unavailable.".to_owned())?
        .insert(request_id.clone(), cancel_sender);
    let provider_result = cancellable_provider_request(
        send_provider_request(&config, &key, &lines, &system),
        cancel_receiver,
    )
    .await;
    if let Some(registry) = ACTIVE_AI_CANCELLATIONS.get() {
        if let Ok(mut guard) = registry.lock() {
            guard.remove(&request_id);
        }
    }
    match provider_result {
        Ok(content) => {
            let (response, proposal) = match parse_action_proposal(&content) {
                Some(envelope) => match validate_action_proposal(&pool, &envelope.action).await {
                    Ok(()) => (envelope.summary.trim().to_owned(), Some(envelope.action)),
                    Err(reason) => (
                        format!("I could not prepare a safe action proposal: {reason} Nothing was changed."),
                        None,
                    ),
                },
                None => (content, None),
            };
            save_ai_response(
                &pool,
                &request_id,
                &assistant_id,
                &input.conversation_id,
                "live_ai",
                "completed",
                &response,
                &config,
                proposal.as_ref(),
            )
            .await?;
        }
        Err(failure) => {
            let request_status = if failure.outcome_unknown {
                "outcome_unknown"
            } else {
                "failed"
            };
            save_ai_response(
                &pool,
                &request_id,
                &assistant_id,
                &input.conversation_id,
                "error",
                request_status,
                &failure.message,
                &config,
                None,
            )
            .await?;
        }
    }
    fetch_ai_message(&pool, &assistant_id).await
}

#[cfg(test)]
fn request_payload_hash(conversation_id: &str, message: &str) -> Result<String, String> {
    let canonical = serde_json::to_vec(&json!({
        "version": 1,
        "conversation_id": conversation_id,
        "message": message,
    }))
    .map_err(|_| "Could not validate the AI request.".to_owned())?;
    Ok(hex_sha256(&canonical))
}

fn request_payload_hash_for_provider(
    conversation_id: &str,
    message: &str,
    config: &ProviderConfig,
) -> Result<String, String> {
    let canonical = serde_json::to_vec(&json!({
        "version": 2,
        "conversation_id": conversation_id,
        "message": message,
        "provider_slot": &config.slot,
        "provider": &config.provider,
        "base_url": &config.base_url,
        "model": &config.model,
    }))
    .map_err(|_| "Could not validate the AI request.".to_owned())?;
    Ok(hex_sha256(&canonical))
}

async fn begin_ai_request(
    pool: &SqlitePool,
    request_id: &str,
    conversation_id: &str,
    message: &str,
    request_hash: &str,
    config: &ProviderConfig,
) -> Result<Option<AiMessage>, String> {
    let mut tx = pool.begin_with("BEGIN IMMEDIATE").await.map_err(db_error)?;
    let exists: i64 = sqlx::query_scalar("SELECT count(*) FROM ai_conversations WHERE id=?")
        .bind(conversation_id)
        .fetch_one(&mut *tx)
        .await
        .map_err(db_error)?;
    if exists == 0 {
        return Err("This conversation no longer exists. Start a new conversation.".into());
    }
    let prior: Option<(String, String, Option<String>, String, Option<String>)> = sqlx::query_as(
        "SELECT conversation_id,status,assistant_message_id,request_hash,lease_expires_at FROM ai_requests WHERE id=?",
    )
    .bind(request_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(db_error)?;
    if let Some((prior_conversation, status, assistant_id, prior_hash, lease_expires_at)) = prior {
        if prior_conversation != conversation_id || prior_hash != request_hash {
            tx.rollback().await.map_err(db_error)?;
            return Err("This request ID was already used for different message content. Start a new request.".into());
        }
        if let Some(id) = assistant_id {
            tx.rollback().await.map_err(db_error)?;
            return fetch_ai_message(pool, &id).await.map(Some);
        }
        if status == "processing" {
            let expired: i64 = sqlx::query_scalar(
                "SELECT CASE WHEN ? IS NULL OR datetime(?)<=datetime('now') THEN 1 ELSE 0 END",
            )
            .bind(&lease_expires_at)
            .bind(&lease_expires_at)
            .fetch_one(&mut *tx)
            .await
            .map_err(db_error)?;
            if expired != 0 {
                mark_ai_request_outcome_unknown(&mut tx, request_id).await?;
                tx.commit().await.map_err(db_error)?;
                let recovered_id: String =
                    sqlx::query_scalar("SELECT assistant_message_id FROM ai_requests WHERE id=?")
                        .bind(request_id)
                        .fetch_one(pool)
                        .await
                        .map_err(db_error)?;
                return fetch_ai_message(pool, &recovered_id).await.map(Some);
            }
            tx.rollback().await.map_err(db_error)?;
            return Err(
                "This request is still processing. LEVI will not send a duplicate request.".into(),
            );
        }
        tx.rollback().await.map_err(db_error)?;
        return Err("The prior AI request has no saved result. Review its recovery state before starting another request.".into());
    }

    let user_id = Uuid::new_v4().to_string();
    let title = message.chars().take(72).collect::<String>();
    sqlx::query("INSERT INTO ai_messages(id,conversation_id,role,response_kind,content,provider,model) VALUES(?,?,'user','live_ai',?,?,?)")
        .bind(&user_id).bind(conversation_id).bind(message).bind(config.display_name()).bind(&config.model)
        .execute(&mut *tx).await.map_err(db_error)?;
    sqlx::query("INSERT INTO ai_requests(id,conversation_id,user_message_id,request_hash,lease_expires_at) VALUES(?,?,?,?,datetime('now','+2 minutes'))")
        .bind(request_id)
        .bind(conversation_id)
        .bind(user_id)
        .bind(request_hash)
        .execute(&mut *tx)
        .await
        .map_err(db_error)?;
    sqlx::query("UPDATE ai_conversations SET title=CASE WHEN title='New conversation' THEN ? ELSE title END,updated_at=datetime('now') WHERE id=?")
        .bind(title).bind(conversation_id).execute(&mut *tx).await.map_err(db_error)?;
    tx.commit().await.map_err(db_error)?;
    Ok(None)
}

async fn mark_ai_request_outcome_unknown(
    tx: &mut Transaction<'_, Sqlite>,
    request_id: &str,
) -> Result<(), String> {
    let row: Option<(String, Option<String>, Option<String>)> = sqlx::query_as(
        "SELECT ai_requests.conversation_id,ai_messages.provider,ai_messages.model FROM ai_requests JOIN ai_messages ON ai_messages.id=ai_requests.user_message_id WHERE ai_requests.id=? AND ai_requests.status='processing'",
    )
    .bind(request_id)
    .fetch_optional(&mut **tx)
    .await
    .map_err(db_error)?;
    let Some((conversation_id, provider, model)) = row else {
        return Ok(());
    };
    let assistant_id = Uuid::new_v4().to_string();
    let content = "The provider request was interrupted before LEVI saved a response. The provider may have received and processed it. LEVI did not resend it. Review provider usage before choosing to send the prompt again as a new request.";
    sqlx::query("INSERT INTO ai_messages(id,conversation_id,role,response_kind,content,provider,model) VALUES(?,?,'assistant','error',?,?,?)")
        .bind(&assistant_id)
        .bind(&conversation_id)
        .bind(content)
        .bind(provider)
        .bind(model)
        .execute(&mut **tx)
        .await
        .map_err(db_error)?;
    sqlx::query("UPDATE ai_requests SET assistant_message_id=?,status='outcome_unknown',recovery_attempts=recovery_attempts+1,last_error=?,lease_expires_at=NULL,updated_at=datetime('now') WHERE id=? AND status='processing'")
        .bind(assistant_id)
        .bind(content)
        .bind(request_id)
        .execute(&mut **tx)
        .await
        .map_err(db_error)?;
    sqlx::query("UPDATE ai_conversations SET updated_at=datetime('now') WHERE id=?")
        .bind(conversation_id)
        .execute(&mut **tx)
        .await
        .map_err(db_error)?;
    Ok(())
}

#[tauri::command]
pub async fn recover_interrupted_ai_requests(
    app: AppHandle,
) -> Result<AiRequestRecoverySummary, String> {
    let pool = database(&app).await?;
    recover_interrupted_ai_requests_in_pool(&pool).await
}

async fn recover_interrupted_ai_requests_in_pool(
    pool: &SqlitePool,
) -> Result<AiRequestRecoverySummary, String> {
    let mut tx = pool.begin_with("BEGIN IMMEDIATE").await.map_err(db_error)?;
    let expired: Vec<String> = sqlx::query_scalar(
        "SELECT id FROM ai_requests WHERE status='processing' AND (lease_expires_at IS NULL OR datetime(lease_expires_at)<=datetime('now')) ORDER BY created_at",
    )
    .fetch_all(&mut *tx)
    .await
    .map_err(db_error)?;
    for request_id in &expired {
        mark_ai_request_outcome_unknown(&mut tx, request_id).await?;
    }
    let active: Vec<AiRequestRecovery> = sqlx::query_as(
        "SELECT r.id AS request_id,r.status,m.content AS user_message,r.lease_expires_at,r.recovery_attempts FROM ai_requests r JOIN ai_messages m ON m.id=r.user_message_id WHERE r.status='processing' ORDER BY r.created_at",
    )
    .fetch_all(&mut *tx)
    .await
    .map_err(db_error)?;
    tx.commit().await.map_err(db_error)?;
    Ok(AiRequestRecoverySummary {
        recovered: expired.len() as u64,
        active,
    })
}

fn parse_action_proposal(content: &str) -> Option<ActionEnvelope> {
    let json = content.trim().strip_prefix("LEVI_ACTION_PROPOSAL:")?;
    let envelope: ActionEnvelope = serde_json::from_str(json.trim()).ok()?;
    (envelope.version == 1
        && !envelope.summary.trim().is_empty()
        && envelope.summary.chars().count() <= 500)
        .then_some(envelope)
}

async fn validate_action_proposal(
    pool: &SqlitePool,
    action: &ProposedAction,
) -> Result<(), String> {
    match action {
        ProposedAction::CreateTask {
            project_id,
            title,
            description,
            priority,
        } => {
            if project_id.trim().is_empty()
                || title.trim().is_empty()
                || title.chars().count() > 180
                || description.chars().count() > 8000
                || !(1..=4).contains(priority)
            {
                return Err("task fields are invalid or exceed their supported limits.".into());
            }
            let active: i64 = sqlx::query_scalar(
                "SELECT count(*) FROM projects WHERE id=? AND archived_at IS NULL AND status='active'",
            )
            .bind(project_id.trim())
            .fetch_one(pool)
            .await
            .map_err(db_error)?;
            if active == 0 {
                return Err("the selected project is not active or no longer exists.".into());
            }
        }
        ProposedAction::CreateReminder {
            title,
            body,
            due_at,
            project_id,
            task_id,
        } => {
            if title.trim().is_empty()
                || title.chars().count() > 120
                || body.chars().count() > 4000
                || due_at.trim().is_empty()
            {
                return Err("reminder fields are invalid or exceed their supported limits.".into());
            }
            let valid_due: Option<String> = sqlx::query_scalar("SELECT datetime(?)")
                .bind(due_at.trim())
                .fetch_one(pool)
                .await
                .map_err(db_error)?;
            if valid_due.is_none() {
                return Err(
                    "the reminder date is invalid. Use an ISO date-time with timezone.".into(),
                );
            }
            validate_action_link(pool, "projects", project_id.as_deref()).await?;
            validate_action_link(pool, "tasks", task_id.as_deref()).await?;
            if let (Some(project), Some(task)) = (project_id.as_deref(), task_id.as_deref()) {
                let matches: i64 = sqlx::query_scalar(
                    "SELECT count(*) FROM tasks WHERE id=? AND project_id=? AND archived_at IS NULL",
                )
                .bind(task.trim())
                .bind(project.trim())
                .fetch_one(pool)
                .await
                .map_err(db_error)?;
                if matches == 0 {
                    return Err("the selected task does not belong to the selected project.".into());
                }
            }
        }
    }
    Ok(())
}

async fn validate_action_link(
    pool: &SqlitePool,
    kind: &str,
    value: Option<&str>,
) -> Result<(), String> {
    let Some(value) = value.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(());
    };
    let sql = match kind {
        "projects" => "SELECT count(*) FROM projects WHERE id=? AND archived_at IS NULL",
        "tasks" => "SELECT count(*) FROM tasks WHERE id=? AND archived_at IS NULL",
        _ => return Err("Invalid action link type.".into()),
    };
    let exists: i64 = sqlx::query_scalar(sql)
        .bind(value)
        .fetch_one(pool)
        .await
        .map_err(db_error)?;
    if exists == 0 {
        return Err(format!("the selected {kind} is unavailable or archived."));
    }
    Ok(())
}

async fn save_ai_response(
    pool: &SqlitePool,
    request_id: &str,
    assistant_id: &str,
    conversation_id: &str,
    response_kind: &str,
    request_status: &str,
    content: &str,
    config: &ProviderConfig,
    action: Option<&ProposedAction>,
) -> Result<(), String> {
    let mut tx = pool.begin_with("BEGIN IMMEDIATE").await.map_err(db_error)?;
    sqlx::query("INSERT INTO ai_messages(id,conversation_id,role,response_kind,content,provider,model) VALUES(?,?,'assistant',?,?,?,?)")
        .bind(assistant_id).bind(conversation_id).bind(response_kind).bind(content).bind(config.display_name()).bind(&config.model)
        .execute(&mut *tx).await.map_err(db_error)?;
    if let Some(action) = action {
        let proposal_id = Uuid::new_v4().to_string();
        let action_kind = match action {
            ProposedAction::CreateTask { .. } => "create_task",
            ProposedAction::CreateReminder { .. } => "create_reminder",
        };
        let arguments = serde_json::to_string(action)
            .map_err(|_| "Could not store the typed action proposal.".to_owned())?;
        let content_hash = action_content_hash(action)?;
        sqlx::query("INSERT INTO ai_action_proposals(id,request_id,assistant_message_id,action_kind,arguments_json,content_hash) VALUES(?,?,?,?,?,?)")
            .bind(proposal_id).bind(request_id).bind(assistant_id).bind(action_kind).bind(arguments).bind(content_hash)
            .execute(&mut *tx).await.map_err(db_error)?;
    }
    let last_error = matches!(request_status, "failed" | "outcome_unknown").then_some(content);
    sqlx::query("UPDATE ai_requests SET assistant_message_id=?,status=?,last_error=?,lease_expires_at=NULL,updated_at=datetime('now') WHERE id=? AND conversation_id=? AND status='processing'")
        .bind(assistant_id).bind(request_status).bind(last_error).bind(request_id).bind(conversation_id)
        .execute(&mut *tx).await.map_err(db_error)?;
    sqlx::query("UPDATE ai_conversations SET updated_at=datetime('now') WHERE id=?")
        .bind(conversation_id)
        .execute(&mut *tx)
        .await
        .map_err(db_error)?;
    tx.commit().await.map_err(db_error)
}

fn action_content_hash(action: &ProposedAction) -> Result<String, String> {
    let canonical = serde_json::to_vec(&json!({
        "version": 1,
        "action": action,
    }))
    .map_err(|_| "Could not calculate the action approval fingerprint.".to_owned())?;
    Ok(hex_sha256(&canonical))
}

fn hex_sha256(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

async fn fetch_ai_message(pool: &SqlitePool, id: &str) -> Result<AiMessage, String> {
    sqlx::query_as("SELECT m.id,m.conversation_id,m.role,m.response_kind,m.content,m.provider,m.model,m.created_at,r.status AS request_status FROM ai_messages m LEFT JOIN ai_requests r ON r.assistant_message_id=m.id WHERE m.id=?")
        .bind(id).fetch_optional(pool).await.map_err(db_error)?
        .ok_or_else(|| "The saved AI response is unavailable. Review the conversation before retrying.".into())
}

#[tauri::command]
pub async fn get_ai_action_proposals(
    app: AppHandle,
    conversation_id: String,
) -> Result<Vec<AiActionProposal>, String> {
    sqlx::query_as("SELECT p.id,r.conversation_id,p.assistant_message_id,p.action_kind,p.arguments_json,p.content_hash,p.status,p.last_error,p.result_json,p.created_at,p.decided_at FROM ai_action_proposals p JOIN ai_requests r ON r.id=p.request_id WHERE r.conversation_id=? ORDER BY p.created_at,p.id")
        .bind(conversation_id).fetch_all(&database(&app).await?).await.map_err(db_error)
}

#[tauri::command]
pub async fn resolve_ai_action_proposal(
    app: AppHandle,
    proposal_id: String,
    expected_hash: String,
    approve: bool,
) -> Result<AiActionProposal, String> {
    let pool = database(&app).await?;
    resolve_ai_action_proposal_in_pool(&pool, &proposal_id, &expected_hash, approve).await
}

async fn resolve_ai_action_proposal_in_pool(
    pool: &SqlitePool,
    proposal_id: &str,
    expected_hash: &str,
    approve: bool,
) -> Result<AiActionProposal, String> {
    let mut tx = pool.begin_with("BEGIN IMMEDIATE").await.map_err(db_error)?;
    let row: Option<(String, String, String, String)> = sqlx::query_as(
        "SELECT status,action_kind,arguments_json,content_hash FROM ai_action_proposals WHERE id=?",
    )
    .bind(proposal_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(db_error)?;
    let Some((status, kind, arguments_json, stored_hash)) = row else {
        return Err("That action proposal no longer exists.".into());
    };
    let action: ProposedAction = serde_json::from_str(&arguments_json)
        .map_err(|_| "Stored action proposal is invalid. No operation was performed.".to_owned())?;
    let actual_hash = action_content_hash(&action)?;
    if stored_hash != actual_hash || expected_hash != actual_hash {
        return Err("This proposal changed after it was displayed. The latest parameters have been refreshed; review them and approve again.".into());
    }
    let action_kind = match &action {
        ProposedAction::CreateTask { .. } => "create_task",
        ProposedAction::CreateReminder { .. } => "create_reminder",
    };
    if kind != action_kind {
        return Err(
            "Stored proposal type does not match its parameters. Nothing was changed.".into(),
        );
    }
    if !approve {
        if status == "rejected" {
            tx.rollback().await.map_err(db_error)?;
            return fetch_ai_action_proposal(pool, proposal_id).await;
        }
        if status != "proposed" {
            return Err("Only a pending action proposal can be rejected.".into());
        }
        sqlx::query("UPDATE ai_action_proposals SET status='rejected',decided_at=datetime('now'),last_error=NULL WHERE id=? AND status='proposed'")
            .bind(proposal_id).execute(&mut *tx).await.map_err(db_error)?;
        tx.commit().await.map_err(db_error)?;
        return fetch_ai_action_proposal(pool, proposal_id).await;
    }
    if status == "executed" {
        tx.rollback().await.map_err(db_error)?;
        return fetch_ai_action_proposal(pool, proposal_id).await;
    }
    if status == "rejected" {
        return Err("A rejected action proposal cannot be executed.".into());
    }
    if status != "proposed" {
        return Err("That action proposal is in an invalid state.".into());
    }

    let entity_id = format!("ai-action-{proposal_id}");
    let result = match action {
        ProposedAction::CreateTask {
            project_id,
            title,
            description,
            priority,
        } => crate::workforce::create_ai_task_in_transaction(
            &mut tx,
            entity_id,
            project_id,
            title,
            description,
            priority,
        )
        .await
        .map(|id| json!({"kind":"task","id":id})),
        ProposedAction::CreateReminder {
            title,
            body,
            due_at,
            project_id,
            task_id,
        } => crate::reminders::create_ai_reminder_in_transaction(
            &mut tx, entity_id, title, body, due_at, project_id, task_id,
        )
        .await
        .map(|id| json!({"kind":"reminder","id":id})),
    };
    // Returning before commit drops the transaction and rolls back both the
    // domain record/activity and the proposal's execution state.
    let result = result?;
    let result_json = serde_json::to_string(&result)
        .map_err(|_| "Could not store the action result.".to_owned())?;
    sqlx::query("UPDATE ai_action_proposals SET status='executed',result_json=?,last_error=NULL,decided_at=COALESCE(decided_at,datetime('now')) WHERE id=? AND status='proposed'")
        .bind(result_json).bind(proposal_id).execute(&mut *tx).await.map_err(db_error)?;
    tx.commit().await.map_err(db_error)?;
    fetch_ai_action_proposal(pool, proposal_id).await
}

async fn fetch_ai_action_proposal(
    pool: &SqlitePool,
    proposal_id: &str,
) -> Result<AiActionProposal, String> {
    sqlx::query_as("SELECT p.id,r.conversation_id,p.assistant_message_id,p.action_kind,p.arguments_json,p.content_hash,p.status,p.last_error,p.result_json,p.created_at,p.decided_at FROM ai_action_proposals p JOIN ai_requests r ON r.id=p.request_id WHERE p.id=?")
        .bind(proposal_id).fetch_optional(pool).await.map_err(db_error)?
        .ok_or_else(|| "That action proposal no longer exists.".into())
}

#[tauri::command]
pub async fn add_local_workforce_summary(
    app: AppHandle,
    conversation_id: String,
) -> Result<AiMessage, String> {
    let pool = database(&app).await?;
    let (context, _) = load_chat_context(&pool, &conversation_id).await?;
    let message = format!("LOCAL WORKFORCE SUMMARY — generated from saved SQLite records; no AI provider was contacted.\n\n{}", context);
    save_local_message(&pool, &conversation_id, "local_summary", &message).await
}

#[tauri::command]
pub async fn add_handoff_prompt(
    app: AppHandle,
    input: HandoffPromptInput,
) -> Result<AiMessage, String> {
    let pool = database(&app).await?;
    let kind = input.kind.as_str();
    if !matches!(
        kind,
        "chatgpt_review" | "claude_revision" | "copilot_implementation" | "verification"
    ) {
        return Err("Choose a supported handoff type.".into());
    }
    let task: Option<(String,String,String,String,String,Option<String>)> = sqlx::query_as("SELECT t.title,t.description,t.instructions,t.completion_notes,t.status,p.name FROM tasks t LEFT JOIN projects p ON p.id=t.project_id WHERE t.id=? AND t.archived_at IS NULL")
        .bind(&input.task_id).fetch_optional(&pool).await.map_err(db_error)?;
    let Some((title, description, instructions, completion, status, project)) = task else {
        return Err("The selected task is unavailable or archived.".into());
    };
    let latest: Option<String> = sqlx::query_scalar(
        "SELECT package_text FROM task_handoffs WHERE task_id=? ORDER BY created_at DESC LIMIT 1",
    )
    .bind(&input.task_id)
    .fetch_optional(&pool)
    .await
    .map_err(db_error)?;
    let audience = match kind {
        "chatgpt_review" => "ChatGPT reviewer",
        "claude_revision" => "original Claude worker",
        "copilot_implementation" => "Copilot implementer",
        _ => "independent verifier",
    };
    let content = format!("MANUAL HANDOFF PROMPT — {audience}\n\nProject: {}\nTask: {title}\nTask status: {status}\n\nOriginal task description:\n{}\n\nRequirements/instructions:\n{}\n\nRecorded completion notes:\n{}\n\nLatest saved handoff package:\n{}\n\nReport actual work, changed files, commands, results, evidence, known issues, and remaining work. Do not claim actions or test results that were not actually performed. This prompt is prepared locally and has not been sent.", project.as_deref().unwrap_or("Unassigned"), empty_as_none(&description), empty_as_none(&instructions), empty_as_none(&completion), latest.as_deref().unwrap_or("No handoff package recorded."));
    save_local_message(&pool, &input.conversation_id, "handoff_prompt", &content).await
}

async fn save_local_message(
    pool: &SqlitePool,
    conversation_id: &str,
    kind: &str,
    content: &str,
) -> Result<AiMessage, String> {
    let id = Uuid::new_v4().to_string();
    let mut tx = pool.begin_with("BEGIN IMMEDIATE").await.map_err(db_error)?;
    let exists: i64 = sqlx::query_scalar("SELECT count(*) FROM ai_conversations WHERE id=?")
        .bind(conversation_id)
        .fetch_one(&mut *tx)
        .await
        .map_err(db_error)?;
    if exists == 0 {
        return Err("This conversation no longer exists. Start a new conversation.".into());
    }
    sqlx::query("INSERT INTO ai_messages(id,conversation_id,role,response_kind,content) VALUES(?,?,'assistant',?,?)")
        .bind(&id).bind(conversation_id).bind(kind).bind(content).execute(&mut *tx).await.map_err(db_error)?;
    sqlx::query("UPDATE ai_conversations SET updated_at=datetime('now') WHERE id=?")
        .bind(conversation_id)
        .execute(&mut *tx)
        .await
        .map_err(db_error)?;
    tx.commit().await.map_err(db_error)?;
    sqlx::query_as("SELECT id,conversation_id,role,response_kind,content,provider,model,created_at,NULL AS request_status FROM ai_messages WHERE id=?")
        .bind(id).fetch_one(pool).await.map_err(db_error)
}

async fn load_chat_context(
    pool: &SqlitePool,
    conversation_id: &str,
) -> Result<(String, Vec<ChatLine>), String> {
    let exists: i64 = sqlx::query_scalar("SELECT count(*) FROM ai_conversations WHERE id=?")
        .bind(conversation_id)
        .fetch_one(pool)
        .await
        .map_err(db_error)?;
    if exists == 0 {
        return Err("This conversation no longer exists. Start a new conversation.".into());
    }
    let projects: Vec<Value> = sqlx::query("SELECT id,name,status,priority,deadline FROM projects WHERE archived_at IS NULL ORDER BY priority DESC,updated_at DESC LIMIT 30")
        .fetch_all(pool).await.map_err(db_error)?.into_iter().map(|r| json!({"id":r.get::<String,_>("id"),"name":r.get::<String,_>("name"),"status":r.get::<String,_>("status"),"priority":r.get::<i64,_>("priority"),"deadline":r.get::<Option<String>,_>("deadline")})).collect();
    let tasks: Vec<Value> = sqlx::query("SELECT t.id,t.title,t.status,t.priority,t.deadline,substr(t.blocker,1,500) AS blocker,substr(t.description,1,600) AS description,p.id AS project_id,p.name AS project,a.id AS worker_id,a.name AS worker,(SELECT group_concat(d.title || ' [' || d.status || ']', ', ') FROM task_dependencies dep JOIN tasks d ON d.id=dep.depends_on_task_id WHERE dep.task_id=t.id AND d.status!='completed') AS dependency_blockers,(SELECT group_concat(DISTINCT cl.path_display || ' claimed by ' || ct.title) FROM task_file_declarations decl JOIN task_file_claims cl ON cl.project_id=t.project_id AND cl.status='active' AND cl.task_id!=t.id AND (lower(decl.path_key)=lower(cl.path_key) OR substr(lower(decl.path_key),1,length(cl.path_key)+1)=lower(cl.path_key)||'/' OR substr(lower(cl.path_key),1,length(decl.path_key)+1)=lower(decl.path_key)||'/') JOIN tasks ct ON ct.id=cl.task_id WHERE decl.task_id=t.id) AS file_claim_conflicts FROM tasks t LEFT JOIN projects p ON p.id=t.project_id LEFT JOIN accounts a ON a.id=t.account_id WHERE t.archived_at IS NULL ORDER BY CASE t.status WHEN 'blocked' THEN 0 WHEN 'waiting' THEN 1 WHEN 'ready' THEN 2 ELSE 3 END,t.priority DESC,t.updated_at DESC LIMIT 40")
        .fetch_all(pool).await.map_err(db_error)?.into_iter().map(|r| json!({"id":r.get::<String,_>("id"),"title":r.get::<String,_>("title"),"status":r.get::<String,_>("status"),"priority":r.get::<i64,_>("priority"),"deadline":r.get::<Option<String>,_>("deadline"),"blocker":r.get::<String,_>("blocker"),"description":r.get::<String,_>("description"),"projectId":r.get::<Option<String>,_>("project_id"),"project":r.get::<Option<String>,_>("project"),"workerId":r.get::<Option<String>,_>("worker_id"),"worker":r.get::<Option<String>,_>("worker"),"incompleteDependencies":r.get::<Option<String>,_>("dependency_blockers"),"activeFileClaimConflicts":r.get::<Option<String>,_>("file_claim_conflicts")})).collect();
    let accounts: Vec<Value> = sqlx::query("SELECT a.id,a.name,a.status,a.archived_at,a.browser_profile_id,b.name AS browser,bp.profile_name AS profile FROM accounts a LEFT JOIN browser_profiles bp ON bp.id=a.browser_profile_id LEFT JOIN browsers b ON b.id=bp.browser_id ORDER BY a.archived_at,a.name COLLATE NOCASE LIMIT 60")
        .fetch_all(pool).await.map_err(db_error)?.into_iter().map(|r| json!({"id":r.get::<String,_>("id"),"name":r.get::<String,_>("name"),"status":r.get::<String,_>("status"),"archived":r.get::<Option<String>,_>("archived_at").is_some(),"browserProfileId":r.get::<Option<String>,_>("browser_profile_id"),"browser":r.get::<Option<String>,_>("browser"),"profile":r.get::<Option<String>,_>("profile")})).collect();
    let reminders: Vec<Value> = sqlx::query("SELECT id,title,due_at,status,kind,project_id,task_id FROM reminders WHERE status IN ('pending','snoozed') ORDER BY COALESCE(snoozed_until,due_at) LIMIT 40")
        .fetch_all(pool).await.map_err(db_error)?.into_iter().map(|r| json!({"id":r.get::<String,_>("id"),"title":r.get::<String,_>("title"),"dueAt":r.get::<String,_>("due_at"),"status":r.get::<String,_>("status"),"kind":r.get::<String,_>("kind"),"projectId":r.get::<Option<String>,_>("project_id"),"taskId":r.get::<Option<String>,_>("task_id")})).collect();
    let deliverables: Vec<Value> = sqlx::query("SELECT d.title,d.notes,d.completed_at,p.name AS project,t.title AS task FROM deliverables d LEFT JOIN projects p ON p.id=d.project_id LEFT JOIN tasks t ON t.id=d.task_id ORDER BY d.completed_at DESC LIMIT 30")
        .fetch_all(pool).await.map_err(db_error)?.into_iter().map(|r| json!({"title":r.get::<String,_>("title"),"notes":r.get::<String,_>("notes"),"completedAt":r.get::<String,_>("completed_at"),"project":r.get::<Option<String>,_>("project"),"task":r.get::<Option<String>,_>("task")})).collect();
    let activity: Vec<Value> = sqlx::query("SELECT activity_type,substr(description,1,500) AS description,occurred_at FROM activity_logs ORDER BY occurred_at DESC LIMIT 20")
        .fetch_all(pool).await.map_err(db_error)?.into_iter().map(|r| json!({"type":r.get::<String,_>("activity_type"),"description":r.get::<String,_>("description"),"occurredAt":r.get::<String,_>("occurred_at")})).collect();
    let (provider, model): (Option<String>, Option<String>) = sqlx::query_as("SELECT (SELECT value FROM settings WHERE key='ai_provider'),(SELECT value FROM settings WHERE key='ai_model')")
        .fetch_one(pool).await.map_err(db_error)?;
    let mut snapshot = json!({"projects":projects,"tasks":tasks,"accounts":accounts,"upcomingReminders":reminders,"deliverables":deliverables,"recentActivity":activity,"aiProvider":{"provider":provider,"model":model,"apiKeyIncluded":false},"capabilities":["read workspace summaries","generate local handoff prompts","propose task creation for user approval","propose one-time reminder creation for user approval"]});
    while snapshot.to_string().chars().count() > MAX_CONTEXT_CHARS {
        let mut removed = false;
        for key in [
            "deliverables",
            "upcomingReminders",
            "recentActivity",
            "tasks",
            "accounts",
            "projects",
        ] {
            if let Some(items) = snapshot.get_mut(key).and_then(Value::as_array_mut) {
                if items.pop().is_some() {
                    removed = true;
                    break;
                }
            }
        }
        if !removed {
            break;
        }
    }
    let snapshot = snapshot.to_string();
    let history: Vec<ChatLine> = sqlx::query_as::<_, (String,String)>("SELECT role,content FROM ai_messages WHERE conversation_id=? AND response_kind='live_ai' ORDER BY rowid DESC LIMIT 20")
        .bind(conversation_id).fetch_all(pool).await.map_err(db_error)?.into_iter().rev().map(|(role,content)| ChatLine { role, content }).collect();
    Ok((snapshot, history))
}

fn provider_setting_keys(slot: &str) -> (&'static str, &'static str, &'static str) {
    if slot == "secondary" {
        (
            "ai_secondary_provider",
            "ai_secondary_base_url",
            "ai_secondary_model",
        )
    } else {
        ("ai_provider", "ai_base_url", "ai_model")
    }
}

async fn get_config(pool: &SqlitePool, slot: &str) -> Result<AiConfig, String> {
    let slot = normalize_provider_slot(slot)?;
    let (provider_key, url_key, model_key) = provider_setting_keys(slot);
    let rows: Vec<(String, String)> =
        sqlx::query_as("SELECT key,value FROM settings WHERE key IN (?,?,?)")
            .bind(provider_key)
            .bind(url_key)
            .bind(model_key)
            .fetch_all(pool)
            .await
            .map_err(db_error)?;
    let mut provider = "openai_compatible".to_owned();
    let mut base_url = if slot == "secondary" {
        OPENROUTER_BASE_URL.to_owned()
    } else {
        String::new()
    };
    let mut model = String::new();
    for (key, value) in rows {
        match key.as_str() {
            key if key == provider_key => provider = value,
            key if key == url_key => base_url = value,
            key if key == model_key => model = value,
            _ => {}
        }
    }
    let identity = ProviderConfig {
        provider: provider.clone(),
        base_url: base_url.clone(),
        model: model.clone(),
        slot: slot.into(),
    };
    let (credential_store_available, credential_status, credential_store_error) =
        credential_diagnostics(&identity, slot);
    let fallback_enabled: bool = sqlx::query_scalar(
        "SELECT COALESCE((SELECT value='true' FROM settings WHERE key='ai_manual_fallback_enabled'),0)",
    )
    .fetch_one(pool)
    .await
    .map_err(db_error)?;
    Ok(AiConfig {
        slot: slot.into(),
        provider,
        base_url,
        model,
        has_api_key: credential_status == "credential_available",
        credential_store_available,
        credential_status: credential_status.to_owned(),
        credential_store_error,
        fallback_enabled,
    })
}

async fn read_provider_config(pool: &SqlitePool, slot: &str) -> Result<ProviderConfig, String> {
    let config = read_provider_settings(pool, slot).await?;
    let mut validated = validate_config(&config.provider, &config.base_url, &config.model)?;
    validated.slot = config.slot;
    Ok(validated)
}

async fn read_provider_settings(pool: &SqlitePool, slot: &str) -> Result<ProviderConfig, String> {
    let slot = normalize_provider_slot(slot)?;
    let (provider_key, url_key, model_key) = provider_setting_keys(slot);
    let rows: Vec<(String, String)> =
        sqlx::query_as("SELECT key,value FROM settings WHERE key IN (?,?,?)")
            .bind(provider_key)
            .bind(url_key)
            .bind(model_key)
            .fetch_all(pool)
            .await
            .map_err(db_error)?;
    let mut provider = "openai_compatible".to_owned();
    let mut base_url = if slot == "secondary" {
        OPENROUTER_BASE_URL.to_owned()
    } else {
        String::new()
    };
    let mut model = String::new();
    for (key, value) in rows {
        match key.as_str() {
            key if key == provider_key => provider = value,
            key if key == url_key => base_url = value,
            key if key == model_key => model = value,
            _ => {}
        }
    }
    let (provider, base_url) = validate_provider_identity(&provider, &base_url)?;
    Ok(ProviderConfig {
        provider,
        base_url,
        model: model.trim().to_owned(),
        slot: slot.into(),
    })
}

fn validate_config(provider: &str, base_url: &str, model: &str) -> Result<ProviderConfig, String> {
    let (provider, base_url) = validate_provider_identity(provider, base_url)?;
    if model.trim().is_empty() || model.trim().len() > 120 {
        return Err("Enter a model identifier between 1 and 120 characters.".into());
    }
    Ok(ProviderConfig {
        provider,
        base_url,
        model: model.trim().into(),
        slot: "primary".into(),
    })
}

fn validate_provider_identity(provider: &str, base_url: &str) -> Result<(String, String), String> {
    if !matches!(provider, "openai_compatible" | "anthropic") {
        return Err("Choose OpenAI-compatible or Anthropic as the provider type.".into());
    }
    let url = Url::parse(base_url.trim())
        .map_err(|_| "Enter a valid provider base URL, including https://".to_owned())?;
    if !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(
            "The provider URL cannot include credentials, query parameters or a fragment.".into(),
        );
    }
    let loopback = matches!(url.host(), Some(Host::Ipv4(ip)) if ip.is_loopback())
        || matches!(url.host(), Some(Host::Ipv6(ip)) if ip.is_loopback())
        || matches!(url.host(),Some(Host::Domain(d)) if d.eq_ignore_ascii_case("localhost"));
    if url.scheme() != "https" && !(url.scheme() == "http" && loopback) {
        return Err(
            "Provider URLs must use HTTPS. HTTP is allowed only for localhost testing.".into(),
        );
    }
    if url.host().is_none() {
        return Err("Provider URL must include a hostname.".into());
    }
    if provider == "anthropic" && url.path() != "/" {
        return Err("Anthropic base URL must be https://api.anthropic.com (without /v1).".into());
    }
    if provider == "openai_compatible" && url.path().ends_with('/') { /* normalized below */ }
    Ok((
        provider.into(),
        base_url.trim().trim_end_matches('/').into(),
    ))
}

fn credential_user(config: &ProviderConfig, slot: &str) -> String {
    let identity = if slot == "secondary" {
        format!("secondary|{}|{}", config.provider, config.base_url)
    } else {
        format!("{}|{}", config.provider, config.base_url)
    };
    let digest = Sha256::digest(identity.as_bytes());
    format!(
        "{}-{}",
        config.provider,
        digest
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>()
    )
}
fn credential_entry(config: &ProviderConfig, slot: &str) -> Result<Entry, String> {
    Entry::new(CREDENTIAL_SERVICE, &credential_user(config, slot)).map_err(|error| {
        format!(
            "Windows Credential Manager entry is unavailable ({}).",
            credential_error_category(&error)
        )
    })
}
fn save_secret(config: &ProviderConfig, slot: &str, secret: &str) -> Result<(), String> {
    if secret.len() > 2048 || secret.chars().any(char::is_control) {
        return Err("API key is invalid or exceeds the supported length.".into());
    }
    credential_entry(config, slot)?
        .set_password(secret)
        .map_err(|error| format!("Windows Credential Manager could not save the key ({}). No plaintext fallback was used.", credential_error_category(&error)))
}
fn load_secret(config: &ProviderConfig, slot: &str) -> Result<String, String> {
    credential_entry(config, slot)?
        .get_password()
        .map_err(|error| match error {
            keyring::Error::NoEntry => {
                "No credential is configured for this provider endpoint. Save it from AI settings."
                    .into()
            }
            error => format!(
                "Windows Credential Manager could not read this provider credential ({}).",
                credential_error_category(&error)
            ),
        })
}

fn credential_diagnostics(
    config: &ProviderConfig,
    slot: &str,
) -> (bool, &'static str, Option<String>) {
    if let Err(error) = Entry::store_status() {
        return (
            false,
            "store_unavailable",
            Some(credential_error_category(error).to_owned()),
        );
    }
    if config.base_url.trim().is_empty() {
        return (true, "configuration_missing", None);
    }
    if validate_provider_identity(&config.provider, &config.base_url).is_err() {
        return (
            true,
            "configuration_invalid",
            Some("invalid_provider_configuration".into()),
        );
    }
    let entry = match credential_entry(config, slot) {
        Ok(entry) => entry,
        Err(_) => {
            return (
                true,
                "credential_lookup_failed",
                Some("entry_initialization_failed".into()),
            )
        }
    };
    match entry.get_password() {
        Ok(_) => (true, "credential_available", None),
        Err(keyring::Error::NoEntry) => (true, "credential_missing", None),
        Err(error) => (
            true,
            "credential_lookup_failed",
            Some(credential_error_category(&error).to_owned()),
        ),
    }
}

fn credential_error_category(error: &keyring::Error) -> &'static str {
    match error {
        keyring::Error::PlatformFailure(_) => "windows_api_failure",
        keyring::Error::NoStorageAccess(_) => "credential_store_access_denied_or_locked",
        keyring::Error::NoEntry => "credential_not_found",
        keyring::Error::BadEncoding(_) => "credential_encoding_invalid",
        keyring::Error::BadDataFormat(_, _) => "credential_format_invalid",
        keyring::Error::BadStoreFormat(_) => "credential_store_format_invalid",
        keyring::Error::TooLong(_, _) => "credential_field_too_long",
        keyring::Error::Invalid(_, _) => "credential_entry_invalid",
        keyring::Error::Ambiguous(_) => "credential_entry_ambiguous",
        keyring::Error::NoDefaultStore => "backend_not_initialized",
        keyring::Error::NotSupportedByStore(_) => "credential_operation_unsupported",
        _ => "credential_store_error",
    }
}

async fn send_provider_request(
    config: &ProviderConfig,
    key: &str,
    messages: &[ChatLine],
    system: &str,
) -> Result<String, ProviderFailure> {
    let client = Client::builder()
        .timeout(std::time::Duration::from_secs(45))
        .redirect(Policy::none())
        .build()
        .map_err(|_| ProviderFailure {
            message: "Could not initialize the secure HTTP client. No provider request was sent."
                .into(),
            outcome_unknown: false,
        })?;
    let url = if config.provider == "anthropic" {
        format!("{}/v1/messages", config.base_url)
    } else {
        format!("{}/chat/completions", config.base_url)
    };
    let (request, response_value) = if config.provider == "anthropic" {
        let body = json!({"model":config.model,"max_tokens":MAX_PROVIDER_OUTPUT_TOKENS,"system":system,"messages":messages});
        let response = client
            .post(url)
            .header("x-api-key", key)
            .header("anthropic-version", "2023-06-01")
            .json(&body)
            .send()
            .await
            .map_err(|_| ProviderFailure {
                message: "The connection ended without a confirmed provider response. The provider may have received the request; LEVI will not resend it automatically.".into(),
                outcome_unknown: true,
            })?;
        let status = response.status();
        let bytes = response
            .bytes()
            .await
            .map_err(|_| ProviderFailure {
                message: "The provider responded, but LEVI could not read the response. The request may have been processed; LEVI will not resend it automatically.".into(),
                outcome_unknown: true,
            })?;
        (
            status,
            serde_json::from_slice::<Value>(&bytes).unwrap_or(Value::Null),
        )
    } else {
        let mut lines = Vec::with_capacity(messages.len() + 1);
        lines.push(ChatLine {
            role: "system".into(),
            content: system.into(),
        });
        lines.extend_from_slice(messages);
        let body =
            json!({"model":config.model,"messages":lines,"max_tokens":MAX_PROVIDER_OUTPUT_TOKENS});
        let response = client
            .post(url)
            .bearer_auth(key)
            .json(&body)
            .send()
            .await
            .map_err(|_| ProviderFailure {
                message: "The connection ended without a confirmed provider response. The provider may have received the request; LEVI will not resend it automatically.".into(),
                outcome_unknown: true,
            })?;
        let status = response.status();
        let bytes = response
            .bytes()
            .await
            .map_err(|_| ProviderFailure {
                message: "The provider responded, but LEVI could not read the response. The request may have been processed; LEVI will not resend it automatically.".into(),
                outcome_unknown: true,
            })?;
        (
            status,
            serde_json::from_slice::<Value>(&bytes).unwrap_or(Value::Null),
        )
    };
    if !request.is_success() {
        return Err(ProviderFailure {
            message: provider_error(request),
            outcome_unknown: request.is_server_error(),
        });
    }
    let completion = parse_response(&config.provider, &response_value).map_err(|error| {
        ProviderFailure {
            message: format!("{error} The provider may have processed the request; LEVI will not resend it automatically."),
            outcome_unknown: true,
        }
    })?;
    completion_text_or_failure(completion)
}

fn completion_text_or_failure(completion: ProviderCompletion) -> Result<String, ProviderFailure> {
    if matches!(
        completion.finish_reason.as_deref(),
        Some("length" | "max_tokens")
    ) {
        return Err(ProviderFailure {
            message: format!("The provider stopped at its output-token limit. LEVI did not present the partial reply as complete. {}{}{} Review the provider usage before retrying; a new request may be billed.",
                completion.prompt_tokens.map(|n| format!("Input tokens: {n}. ")).unwrap_or_default(),
                completion.completion_tokens.map(|n| format!("Output tokens: {n}. ")).unwrap_or_default(),
                completion.total_tokens.map(|n| format!("Total tokens: {n}. ")).unwrap_or_default()),
            outcome_unknown: true,
        });
    }
    Ok(completion.text)
}

async fn cancellable_provider_request<F, T>(
    provider_future: F,
    cancellation: oneshot::Receiver<()>,
) -> Result<T, ProviderFailure>
where
    F: Future<Output = Result<T, ProviderFailure>>,
{
    tokio::select! {
        biased;
        _ = cancellation => Err(ProviderFailure {
            message: "Generation was stopped. The provider may already have received the request; LEVI did not resend it. Review provider usage before retrying.".into(),
            outcome_unknown: true,
        }),
        result = provider_future => result,
    }
}

async fn list_provider_models(
    config: &ProviderConfig,
    key: &str,
) -> Result<Vec<ProviderModel>, String> {
    let client = Client::builder()
        .timeout(std::time::Duration::from_secs(20))
        .redirect(Policy::none())
        .build()
        .map_err(|_| "Could not initialize the secure HTTP client.".to_owned())?;
    let endpoint = if config.provider == "anthropic" {
        format!("{}/v1/models?limit=100", config.base_url)
    } else {
        format!("{}/models", config.base_url)
    };
    let mut request = client.get(endpoint);
    if config.provider == "anthropic" {
        request = request
            .header("x-api-key", key)
            .header("anthropic-version", "2023-06-01");
    } else {
        request = request.bearer_auth(key);
    }
    let response = request
        .send()
        .await
        .map_err(|_| "Could not reach the configured provider model-list endpoint. Check the URL and network, then retry manually.".to_owned())?;
    let status = response.status();
    if !status.is_success() {
        return Err(provider_error(status));
    }
    let bytes = response
        .bytes()
        .await
        .map_err(|_| "The provider model list could not be read.".to_owned())?;
    parse_provider_models(&bytes)
}

fn parse_provider_models(bytes: &[u8]) -> Result<Vec<ProviderModel>, String> {
    let value: Value = serde_json::from_slice(bytes)
        .map_err(|_| "The provider returned an unsupported model-list response.".to_owned())?;
    let data = value
        .get("data")
        .and_then(Value::as_array)
        .ok_or_else(|| "The provider returned an unsupported model-list response.".to_owned())?;
    let mut models = Vec::new();
    for model in data.iter().take(500) {
        let Some(id) = model
            .get("id")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|id| !id.is_empty() && id.len() <= 120 && !id.chars().any(char::is_control))
        else {
            continue;
        };
        let display_name = model
            .get("name")
            .or_else(|| model.get("display_name"))
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|name| {
                !name.is_empty() && name.len() <= 180 && !name.chars().any(char::is_control)
            })
            .map(str::to_owned);
        if !models
            .iter()
            .any(|existing: &ProviderModel| existing.id == id)
        {
            models.push(ProviderModel {
                id: id.to_owned(),
                display_name,
            });
        }
    }
    if models.is_empty() {
        return Err("The provider returned no usable model identifiers. You can enter a compatible model ID manually.".into());
    }
    models.sort_by(|left, right| left.id.cmp(&right.id));
    Ok(models)
}

fn parse_response(provider: &str, value: &Value) -> Result<ProviderCompletion, String> {
    let (text, finish_reason, prompt_tokens, completion_tokens, total_tokens) =
        if provider == "anthropic" {
            let text = value.get("content").and_then(Value::as_array).map(|items| {
                items
                    .iter()
                    .filter_map(|item| item.get("text").and_then(Value::as_str))
                    .collect::<Vec<_>>()
                    .join("\n")
            });
            let usage = value.get("usage");
            (
                text,
                value
                    .get("stop_reason")
                    .and_then(Value::as_str)
                    .map(str::to_owned),
                usage
                    .and_then(|v| v.get("input_tokens"))
                    .and_then(Value::as_u64),
                usage
                    .and_then(|v| v.get("output_tokens"))
                    .and_then(Value::as_u64),
                None,
            )
        } else {
            let usage = value.get("usage");
            (
                value
                    .pointer("/choices/0/message/content")
                    .and_then(Value::as_str)
                    .map(str::to_owned),
                value
                    .pointer("/choices/0/finish_reason")
                    .and_then(Value::as_str)
                    .map(str::to_owned),
                usage
                    .and_then(|v| v.get("prompt_tokens"))
                    .and_then(Value::as_u64),
                usage
                    .and_then(|v| v.get("completion_tokens"))
                    .and_then(Value::as_u64),
                usage
                    .and_then(|v| v.get("total_tokens"))
                    .and_then(Value::as_u64),
            )
        };
    let text = text
        .filter(|s| !s.trim().is_empty())
        .ok_or_else(|| "Provider returned an unsupported or empty response format.".to_owned())?;
    Ok(ProviderCompletion {
        text,
        finish_reason,
        prompt_tokens,
        completion_tokens,
        total_tokens,
    })
}
fn provider_error(status: StatusCode) -> String {
    match status.as_u16() {
        401 | 403 => {
            "Provider rejected the API key or account permissions (HTTP authentication error)."
                .into()
        }
        404 => "Provider endpoint or model was not found. Check the base URL and model identifier."
            .into(),
        429 => {
            "Provider rate limit or quota was reached. Check the provider account and retry later."
                .into()
        }
        400 => {
            "Provider rejected the request. Check model compatibility and provider settings.".into()
        }
        code => format!(
            "Provider request failed with HTTP {code}. No response body or API key was logged."
        ),
    }
}
fn empty_as_none(value: &str) -> &str {
    if value.trim().is_empty() {
        "None recorded."
    } else {
        value
    }
}
fn db_error(error: sqlx::Error) -> String {
    format!("Local database error: {error}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::migrate::{Migration, MigrationType, Migrator};
    use sqlx::sqlite::SqlitePoolOptions;
    use std::borrow::Cow;

    #[cfg(windows)]
    struct TestCredential(Entry);

    #[cfg(windows)]
    impl Drop for TestCredential {
        fn drop(&mut self) {
            let _ = self.0.delete_credential();
        }
    }

    #[cfg(windows)]
    #[test]
    fn windows_credential_manager_round_trip_uses_isolated_temporary_entry() {
        Entry::store_status().as_ref().unwrap_or_else(|error| {
            panic!(
                "Credential Manager backend unavailable: {}",
                credential_error_category(&error)
            )
        });

        let service = "com.local.levi-command-center.tests.credential-manager";
        let account = Uuid::new_v4().to_string();
        let credential = TestCredential(Entry::new(service, &account).unwrap_or_else(|error| {
            panic!(
                "Could not create isolated test credential entry: {}",
                credential_error_category(&error)
            )
        }));
        let first = "LEVI-CREDENTIAL-TEST-VALUE-ONE";
        let second = "LEVI-CREDENTIAL-TEST-VALUE-TWO";

        credential.0.set_password(first).unwrap_or_else(|error| {
            panic!(
                "Could not store isolated test value: {}",
                credential_error_category(&error)
            )
        });
        assert_eq!(credential.0.get_password().unwrap(), first);
        credential.0.set_password(second).unwrap_or_else(|error| {
            panic!(
                "Could not replace isolated test value: {}",
                credential_error_category(&error)
            )
        });
        assert_eq!(credential.0.get_password().unwrap(), second);
        credential.0.delete_credential().unwrap_or_else(|error| {
            panic!(
                "Could not remove isolated test credential: {}",
                credential_error_category(&error)
            )
        });
        assert!(matches!(
            credential.0.get_password(),
            Err(keyring::Error::NoEntry)
        ));
    }

    fn full_test_migrator() -> Migrator {
        let scripts = [
            include_str!("../migrations/001_initial.sql"),
            include_str!("../migrations/002_browser_registry.sql"),
            include_str!("../migrations/003_workforce_checkpoints.sql"),
            include_str!("../migrations/004_task_queue_dependencies_and_claims.sql"),
            include_str!("../migrations/005_task_file_releases.sql"),
            include_str!("../migrations/006_reminder_delivery.sql"),
            include_str!("../migrations/007_checkpoints_and_handoffs.sql"),
            include_str!("../migrations/008_ai_conversations.sql"),
            include_str!("../migrations/009_ai_actions.sql"),
        ];
        let migrations = scripts
            .iter()
            .enumerate()
            .map(|(index, sql)| {
                Migration::new(
                    index as i64 + 1,
                    Cow::Owned(format!("test migration {}", index + 1)),
                    MigrationType::Simple,
                    Cow::Owned((*sql).to_owned()),
                    false,
                )
            })
            .collect();
        Migrator {
            migrations: Cow::Owned(migrations),
            ..Migrator::DEFAULT
        }
    }

    fn one_migration(version: i64, sql: &'static str) -> Migrator {
        Migrator {
            migrations: Cow::Owned(vec![Migration::new(
                version,
                Cow::Borrowed("test recovery migration"),
                MigrationType::Simple,
                Cow::Borrowed(sql),
                false,
            )]),
            ..Migrator::DEFAULT
        }
    }

    async fn db() -> SqlitePool {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::raw_sql("CREATE TABLE settings(key TEXT PRIMARY KEY,value TEXT NOT NULL,updated_at TEXT NOT NULL DEFAULT (datetime('now'))); CREATE TABLE ai_conversations(id TEXT PRIMARY KEY,title TEXT NOT NULL DEFAULT 'New conversation',created_at TEXT NOT NULL DEFAULT (datetime('now')),updated_at TEXT NOT NULL DEFAULT (datetime('now'))); CREATE TABLE ai_messages(id TEXT PRIMARY KEY,conversation_id TEXT NOT NULL REFERENCES ai_conversations(id),role TEXT NOT NULL CHECK(role IN ('user','assistant')),response_kind TEXT NOT NULL CHECK(response_kind IN ('live_ai','local_summary','handoff_prompt','error')),content TEXT NOT NULL,provider TEXT,model TEXT,created_at TEXT NOT NULL DEFAULT (datetime('now'))); CREATE TABLE projects(id TEXT PRIMARY KEY,name TEXT,status TEXT,priority INTEGER,deadline TEXT,archived_at TEXT,updated_at TEXT); CREATE TABLE tasks(id TEXT PRIMARY KEY,title TEXT,status TEXT,priority INTEGER,deadline TEXT,blocker TEXT,description TEXT,project_id TEXT,account_id TEXT,archived_at TEXT,updated_at TEXT); CREATE TABLE accounts(id TEXT PRIMARY KEY,name TEXT,status TEXT,archived_at TEXT,browser_profile_id TEXT); CREATE TABLE browser_profiles(id TEXT PRIMARY KEY,profile_name TEXT,browser_id TEXT); CREATE TABLE browsers(id TEXT PRIMARY KEY,name TEXT); CREATE TABLE reminders(id TEXT PRIMARY KEY,title TEXT,due_at TEXT,status TEXT,kind TEXT,snoozed_until TEXT,project_id TEXT,task_id TEXT); CREATE TABLE deliverables(id TEXT PRIMARY KEY,title TEXT,notes TEXT,completed_at TEXT,project_id TEXT,task_id TEXT); CREATE TABLE task_handoffs(id TEXT PRIMARY KEY,task_id TEXT,package_text TEXT,created_at TEXT); CREATE TABLE task_dependencies(task_id TEXT,depends_on_task_id TEXT); CREATE TABLE task_file_declarations(task_id TEXT,path_key TEXT); CREATE TABLE task_file_claims(project_id TEXT,task_id TEXT,path_key TEXT,path_display TEXT,status TEXT); CREATE TABLE activity_logs(id TEXT PRIMARY KEY,activity_type TEXT,description TEXT,occurred_at TEXT);").execute(&pool).await.unwrap();
        sqlx::raw_sql(include_str!("../migrations/009_ai_actions.sql"))
            .execute(&pool)
            .await
            .unwrap();
        pool
    }

    async fn migrated_db_before_009() -> SqlitePool {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        for migration in [
            include_str!("../migrations/001_initial.sql"),
            include_str!("../migrations/002_browser_registry.sql"),
            include_str!("../migrations/003_workforce_checkpoints.sql"),
            include_str!("../migrations/004_task_queue_dependencies_and_claims.sql"),
            include_str!("../migrations/005_task_file_releases.sql"),
            include_str!("../migrations/006_reminder_delivery.sql"),
            include_str!("../migrations/007_checkpoints_and_handoffs.sql"),
            include_str!("../migrations/008_ai_conversations.sql"),
        ] {
            sqlx::raw_sql(migration).execute(&pool).await.unwrap();
        }
        pool
    }

    async fn migrated_db() -> SqlitePool {
        let pool = migrated_db_before_009().await;
        sqlx::raw_sql(include_str!("../migrations/009_ai_actions.sql"))
            .execute(&pool)
            .await
            .unwrap();
        pool
    }

    #[tokio::test]
    async fn migration_009_applies_after_existing_schema_without_changing_ai_history() {
        let pool = migrated_db_before_009().await;
        sqlx::query(
            "INSERT INTO ai_conversations(id,title) VALUES('existing-conversation','Existing')",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query("INSERT INTO ai_messages(id,conversation_id,role,response_kind,content) VALUES('existing-message','existing-conversation','user','live_ai','existing history')")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::raw_sql(include_str!("../migrations/009_ai_actions.sql"))
            .execute(&pool)
            .await
            .unwrap();
        let history_count: i64 =
            sqlx::query_scalar("SELECT count(*) FROM ai_messages WHERE id='existing-message'")
                .fetch_one(&pool)
                .await
                .unwrap();
        let requests_exists: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM sqlite_master WHERE type='table' AND name='ai_requests'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        let foreign_key_issues: i64 =
            sqlx::query_scalar("SELECT count(*) FROM pragma_foreign_key_check")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(history_count, 1);
        assert_eq!(requests_exists, 1);
        assert_eq!(foreign_key_issues, 0);
    }

    #[tokio::test]
    async fn sqlx_migration_runner_applies_all_migrations_idempotently() {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        let migrator = full_test_migrator();

        migrator.run(&pool).await.unwrap();
        migrator.run(&pool).await.unwrap();

        let applied: i64 = sqlx::query_scalar("SELECT count(*) FROM _sqlx_migrations")
            .fetch_one(&pool)
            .await
            .unwrap();
        let latest: i64 = sqlx::query_scalar("SELECT max(version) FROM _sqlx_migrations")
            .fetch_one(&pool)
            .await
            .unwrap();
        let integrity: String = sqlx::query_scalar("PRAGMA integrity_check")
            .fetch_one(&pool)
            .await
            .unwrap();
        let foreign_key_issues: i64 =
            sqlx::query_scalar("SELECT count(*) FROM pragma_foreign_key_check")
                .fetch_one(&pool)
                .await
                .unwrap();

        assert_eq!(applied, 9);
        assert_eq!(latest, 9);
        assert_eq!(integrity, "ok");
        assert_eq!(foreign_key_issues, 0);
    }

    #[tokio::test]
    async fn failed_sqlx_migration_rolls_back_and_can_be_retried() {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        let broken = one_migration(
            1,
            "CREATE TABLE should_rollback(id TEXT PRIMARY KEY); INSERT INTO missing_table VALUES('x');",
        );

        assert!(broken.run(&pool).await.is_err());
        let partial_table_count: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM sqlite_master WHERE type='table' AND name='should_rollback'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(partial_table_count, 0);

        one_migration(1, "CREATE TABLE should_rollback(id TEXT PRIMARY KEY);")
            .run(&pool)
            .await
            .unwrap();
        let recovered_table_count: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM sqlite_master WHERE type='table' AND name='should_rollback'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(recovered_table_count, 1);
    }

    #[tokio::test]
    async fn migration_009_enforces_foreign_keys_uniqueness_json_and_states() {
        let pool = migrated_db().await;
        sqlx::query("INSERT INTO ai_conversations(id) VALUES('c')")
            .execute(&pool)
            .await
            .unwrap();
        for (id, role) in [
            ("u1", "user"),
            ("u2", "user"),
            ("a1", "assistant"),
            ("a2", "assistant"),
        ] {
            sqlx::query("INSERT INTO ai_messages(id,conversation_id,role,response_kind,content) VALUES(?,'c',?,'live_ai','test')")
                .bind(id).bind(role).execute(&pool).await.unwrap();
        }
        sqlx::query("INSERT INTO ai_requests(id,conversation_id,user_message_id,request_hash) VALUES('r1','c','u1','hash-1'),('r2','c','u2','hash-2')")
            .execute(&pool)
            .await
            .unwrap();
        assert!(sqlx::query("INSERT INTO ai_requests(id,conversation_id,user_message_id,request_hash) VALUES('r3','c','u1','hash-3')")
            .execute(&pool).await.is_err());
        assert!(sqlx::query("INSERT INTO ai_requests(id,conversation_id,user_message_id,request_hash,status) VALUES('r-invalid','c','a1','hash','unknown')")
            .execute(&pool).await.is_err());
        assert!(sqlx::query("INSERT INTO ai_action_proposals(id,request_id,assistant_message_id,action_kind,arguments_json,content_hash) VALUES('p-json','r1','a1','create_task','not-json','hash')")
            .execute(&pool).await.is_err());
        assert!(sqlx::query("INSERT INTO ai_action_proposals(id,request_id,assistant_message_id,action_kind,arguments_json,content_hash) VALUES('p-kind','r1','a1','run_shell','{}','hash')")
            .execute(&pool).await.is_err());
        sqlx::query("INSERT INTO ai_action_proposals(id,request_id,assistant_message_id,action_kind,arguments_json,content_hash) VALUES('p1','r1','a1','create_task','{}','hash')")
            .execute(&pool).await.unwrap();
        assert!(sqlx::query("INSERT INTO ai_action_proposals(id,request_id,assistant_message_id,action_kind,arguments_json,content_hash) VALUES('p2','r2','a1','create_task','{}','hash')")
            .execute(&pool).await.is_err());
        let foreign_key_issues: i64 =
            sqlx::query_scalar("SELECT count(*) FROM pragma_foreign_key_check")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(foreign_key_issues, 0);
    }

    #[test]
    fn provider_urls_reject_plain_http_remote_and_credentials() {
        assert!(validate_config("openai_compatible", "http://example.com/v1", "m").is_err());
        assert!(validate_config(
            "openai_compatible",
            "https://user:secret@example.com/v1",
            "m"
        )
        .is_err());
        assert!(validate_config(
            "openai_compatible",
            "https://example.com/v1?token=secret",
            "m"
        )
        .is_err());
        assert!(validate_config("openai_compatible", "http://127.0.0.1:1234/v1", "m").is_ok());
    }

    #[test]
    fn provider_urls_enforce_supported_provider_shapes() {
        assert!(validate_config("made_up", "https://example.com/v1", "m").is_err());
        assert!(validate_config("anthropic", "https://api.anthropic.com/v1", "model-id").is_err());
        assert!(validate_config("anthropic", "https://api.anthropic.com", "model-id").is_ok());
        assert!(validate_config("openai_compatible", "https://example.com/v1", "model-id").is_ok());
    }

    #[test]
    fn provider_identity_accepts_exact_configured_compatible_endpoint_without_assuming_host() {
        let (provider, base_url) =
            validate_provider_identity("openai_compatible", "https://gateway.example/custom/v1/")
                .unwrap();
        assert_eq!(provider, "openai_compatible");
        assert_eq!(base_url, "https://gateway.example/custom/v1");
        assert!(validate_provider_identity("openai_compatible", "").is_err());
    }

    #[test]
    fn provider_model_catalog_is_bounded_deduplicated_and_rejects_invalid_entries() {
        let models = parse_provider_models(
            br#"{"data":[{"id":"model-z","name":"Model Z"},{"id":"model-a","display_name":"Model A"},{"id":"model-b","name":"Model B","display_name":"Alternate B"},{"id":"model-a"},{"id":""},{"id":"invalid\nmodel"},{"name":"missing id"}]}"#,
        )
        .unwrap();
        assert_eq!(models.len(), 3);
        assert_eq!(models[0].id, "model-a");
        assert_eq!(models[0].display_name.as_deref(), Some("Model A"));
        assert_eq!(models[1].id, "model-b");
        assert_eq!(models[1].display_name.as_deref(), Some("Model B"));
        assert_eq!(models[2].id, "model-z");
        assert!(parse_provider_models(br#"{"models":[]}"#).is_err());
    }

    #[tokio::test]
    async fn cancelling_provider_request_marks_outcome_unknown_without_retrying() {
        let (cancel, receiver) = oneshot::channel();
        let request = tokio::spawn(async move {
            cancellable_provider_request(
                std::future::pending::<Result<(), ProviderFailure>>(),
                receiver,
            )
            .await
        });
        cancel.send(()).unwrap();
        let failure = request.await.unwrap().unwrap_err();
        assert!(failure.outcome_unknown);
        assert!(failure.message.contains("did not resend"));
    }

    #[test]
    fn provider_response_completion_classifies_complete_truncated_empty_and_incomplete() {
        let complete = parse_response(
            "openai_compatible",
            &json!({"choices":[{"message":{"content":"hello"},"finish_reason":"stop"}],"usage":{"prompt_tokens":12,"completion_tokens":8,"total_tokens":20}}),
        ).unwrap();
        assert_eq!(complete.text, "hello");
        assert_eq!(complete.finish_reason.as_deref(), Some("stop"));
        assert_eq!(complete.prompt_tokens, Some(12));
        assert_eq!(complete.completion_tokens, Some(8));
        assert_eq!(complete.total_tokens, Some(20));
        assert_eq!(completion_text_or_failure(complete).unwrap(), "hello");

        let anthropic = parse_response(
            "anthropic",
            &json!({"content":[{"type":"text","text":"hi"}],"stop_reason":"end_turn","usage":{"input_tokens":4,"output_tokens":3}}),
        ).unwrap();
        assert_eq!(anthropic.text, "hi");
        assert_eq!(anthropic.finish_reason.as_deref(), Some("end_turn"));
        assert_eq!(anthropic.prompt_tokens, Some(4));
        assert_eq!(anthropic.completion_tokens, Some(3));
        assert_eq!(anthropic.total_tokens, None);

        assert!(
            parse_response("openai_compatible", &json!({"choices":[]})).is_err(),
            "incomplete response must be rejected"
        );
        assert!(
            parse_response(
                "openai_compatible",
                &json!({"choices":[{"message":{"content":"  "},"finish_reason":"stop"}]})
            )
            .is_err(),
            "empty response must be rejected"
        );
        let truncated = parse_response("openai_compatible", &json!({"choices":[{"message":{"content":"partial"},"finish_reason":"length"}],"usage":{"prompt_tokens":100,"completion_tokens":4096,"total_tokens":4196}})).unwrap();
        assert_eq!(truncated.text, "partial");
        assert_eq!(truncated.finish_reason.as_deref(), Some("length"));
        let failure = completion_text_or_failure(truncated).unwrap_err();
        assert!(failure.outcome_unknown);
        assert!(failure.message.contains("output-token limit"));
        assert!(failure.message.contains("Output tokens: 4096"));
        assert_eq!(MAX_PROVIDER_OUTPUT_TOKENS, 4096);
        assert!(provider_error(StatusCode::UNAUTHORIZED).contains("rejected the API key"));
        assert!(!provider_error(StatusCode::INTERNAL_SERVER_ERROR).contains("secret"));
    }

    #[test]
    fn typed_action_parser_accepts_only_supported_strict_envelopes() {
        let valid = r#"LEVI_ACTION_PROPOSAL: {"version":1,"summary":"Create the task","action":{"kind":"create_task","project_id":"p1","title":"Build room","description":"","priority":2}}"#;
        assert!(parse_action_proposal(valid).is_some());
        assert!(parse_action_proposal(
            r#"LEVI_ACTION_PROPOSAL: {"version":2,"summary":"x","action":{"kind":"create_task","project_id":"p1","title":"x","description":"","priority":2}}"#
        )
        .is_none());
        assert!(parse_action_proposal(
            r#"LEVI_ACTION_PROPOSAL: {"version":1,"summary":"x","extra":true,"action":{"kind":"create_task","project_id":"p1","title":"x","description":"","priority":2}}"#
        )
        .is_none());
        assert!(parse_action_proposal(
            r#"LEVI_ACTION_PROPOSAL: {"version":1,"summary":"x","action":{"kind":"run_sql","query":"DROP TABLE tasks"}}"#
        )
        .is_none());
        assert!(parse_action_proposal(
            r#"LEVI_ACTION_PROPOSAL: {"version":1,"summary":"x","action":{"kind":"create_task","project_id":"p1","title":"x","description":"","priority":2,"command":"whoami"}}"#
        )
        .is_none());
    }

    #[tokio::test]
    async fn action_validation_requires_active_project_and_valid_reminder_links() {
        let pool = db().await;
        sqlx::query("INSERT INTO projects(id,name,status) VALUES('p1','Sunshine','active')")
            .execute(&pool)
            .await
            .unwrap();
        let task = ProposedAction::CreateTask {
            project_id: "p1".into(),
            title: "A task".into(),
            description: String::new(),
            priority: 2,
        };
        assert!(validate_action_proposal(&pool, &task).await.is_ok());
        let invalid_project = ProposedAction::CreateTask {
            project_id: "missing".into(),
            title: "A task".into(),
            description: String::new(),
            priority: 2,
        };
        assert!(validate_action_proposal(&pool, &invalid_project)
            .await
            .unwrap_err()
            .contains("not active"));
        let invalid_link = ProposedAction::CreateReminder {
            title: "Review".into(),
            body: String::new(),
            due_at: "2030-01-02T12:00:00Z".into(),
            project_id: Some("missing".into()),
            task_id: None,
        };
        assert!(validate_action_proposal(&pool, &invalid_link)
            .await
            .unwrap_err()
            .contains("project"));
    }

    #[tokio::test]
    async fn duplicate_ai_request_replays_saved_response_without_duplicate_messages() {
        let pool = db().await;
        sqlx::query("INSERT INTO ai_conversations(id) VALUES('c')")
            .execute(&pool)
            .await
            .unwrap();
        let config = ProviderConfig {
            provider: "openai_compatible".into(),
            base_url: "http://127.0.0.1:1234/v1".into(),
            model: "test".into(),
            slot: "primary".into(),
        };
        let request_hash = request_payload_hash("c", "hello").unwrap();
        assert!(
            begin_ai_request(&pool, "request-1", "c", "hello", &request_hash, &config)
                .await
                .unwrap()
                .is_none()
        );
        save_ai_response(
            &pool,
            "request-1",
            "assistant-1",
            "c",
            "live_ai",
            "completed",
            "hello back",
            &config,
            None,
        )
        .await
        .unwrap();
        let replay = begin_ai_request(&pool, "request-1", "c", "hello", &request_hash, &config)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(replay.id, "assistant-1");
        let count: i64 = sqlx::query_scalar("SELECT count(*) FROM ai_messages")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(count, 2);
    }

    #[tokio::test]
    async fn request_id_cannot_be_reused_for_changed_payload_or_while_active() {
        let pool = db().await;
        sqlx::query("INSERT INTO ai_conversations(id) VALUES('c')")
            .execute(&pool)
            .await
            .unwrap();
        let config = ProviderConfig {
            provider: "openai_compatible".into(),
            base_url: "http://127.0.0.1:1234/v1".into(),
            model: "test".into(),
            slot: "primary".into(),
        };
        let request_hash = request_payload_hash("c", "hello").unwrap();
        begin_ai_request(
            &pool,
            "request-active",
            "c",
            "hello",
            &request_hash,
            &config,
        )
        .await
        .unwrap();

        let active_replay = begin_ai_request(
            &pool,
            "request-active",
            "c",
            "hello",
            &request_hash,
            &config,
        )
        .await
        .unwrap_err();
        assert!(active_replay.contains("still processing"));

        let changed_hash = request_payload_hash("c", "different content").unwrap();
        let changed_replay = begin_ai_request(
            &pool,
            "request-active",
            "c",
            "different content",
            &changed_hash,
            &config,
        )
        .await
        .unwrap_err();
        assert!(changed_replay.contains("different message content"));

        let messages: i64 = sqlx::query_scalar("SELECT count(*) FROM ai_messages")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(messages, 1);
    }

    #[tokio::test]
    async fn concurrent_duplicate_request_ids_only_create_one_processing_request() {
        let pool = db().await;
        sqlx::query("INSERT INTO ai_conversations(id) VALUES('c')")
            .execute(&pool)
            .await
            .unwrap();
        let config = ProviderConfig {
            provider: "openai_compatible".into(),
            base_url: "http://127.0.0.1:1234/v1".into(),
            model: "test".into(),
            slot: "primary".into(),
        };
        let request_hash = request_payload_hash("c", "same prompt").unwrap();
        let (first, second) = tokio::join!(
            begin_ai_request(
                &pool,
                "request-concurrent",
                "c",
                "same prompt",
                &request_hash,
                &config,
            ),
            begin_ai_request(
                &pool,
                "request-concurrent",
                "c",
                "same prompt",
                &request_hash,
                &config,
            ),
        );
        let (started, rejected) = match (first, second) {
            (Ok(first), Err(second)) => (first, second),
            (Err(first), Ok(second)) => (second, first),
            _ => panic!("expected one reservation and one rejection"),
        };
        assert!(started.is_none());
        assert!(rejected.contains("still processing"));
        let requests: i64 = sqlx::query_scalar("SELECT count(*) FROM ai_requests")
            .fetch_one(&pool)
            .await
            .unwrap();
        let messages: i64 = sqlx::query_scalar("SELECT count(*) FROM ai_messages")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(requests, 1);
        assert_eq!(messages, 1);
    }

    #[tokio::test]
    async fn expired_request_is_marked_unknown_once_and_never_resent() {
        let pool = db().await;
        sqlx::query("INSERT INTO ai_conversations(id) VALUES('c')")
            .execute(&pool)
            .await
            .unwrap();
        let config = ProviderConfig {
            provider: "openai_compatible".into(),
            base_url: "http://127.0.0.1:1234/v1".into(),
            model: "test".into(),
            slot: "primary".into(),
        };
        let request_hash = request_payload_hash("c", "a prompt").unwrap();
        begin_ai_request(
            &pool,
            "request-expired",
            "c",
            "a prompt",
            &request_hash,
            &config,
        )
        .await
        .unwrap();
        sqlx::query("UPDATE ai_requests SET lease_expires_at=datetime('now','-1 minute') WHERE id='request-expired'")
            .execute(&pool)
            .await
            .unwrap();

        let recovered = recover_interrupted_ai_requests_in_pool(&pool)
            .await
            .unwrap();
        assert_eq!(recovered.recovered, 1);
        assert!(recovered.active.is_empty());
        let assistant_id: String = sqlx::query_scalar(
            "SELECT assistant_message_id FROM ai_requests WHERE id='request-expired'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        let assistant = fetch_ai_message(&pool, &assistant_id).await.unwrap();
        assert_eq!(assistant.response_kind, "error");
        assert_eq!(assistant.request_status.as_deref(), Some("outcome_unknown"));
        assert!(assistant.content.contains("did not resend it"));

        let replay = begin_ai_request(
            &pool,
            "request-expired",
            "c",
            "a prompt",
            &request_hash,
            &config,
        )
        .await
        .unwrap()
        .unwrap();
        assert_eq!(replay.id, assistant_id);
        let second_recovery = recover_interrupted_ai_requests_in_pool(&pool)
            .await
            .unwrap();
        assert_eq!(second_recovery.recovered, 0);
        let messages: i64 = sqlx::query_scalar("SELECT count(*) FROM ai_messages")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(messages, 2);
    }

    #[tokio::test]
    async fn assistant_message_proposal_and_request_completion_roll_back_together() {
        let pool = db().await;
        sqlx::query("INSERT INTO ai_conversations(id) VALUES('c')")
            .execute(&pool)
            .await
            .unwrap();
        let config = ProviderConfig {
            provider: "openai_compatible".into(),
            base_url: "http://127.0.0.1:1234/v1".into(),
            model: "test".into(),
            slot: "primary".into(),
        };
        let request_hash = request_payload_hash("c", "create a task").unwrap();
        begin_ai_request(
            &pool,
            "request-rollback",
            "c",
            "create a task",
            &request_hash,
            &config,
        )
        .await
        .unwrap();
        sqlx::query("CREATE TRIGGER reject_proposal BEFORE INSERT ON ai_action_proposals BEGIN SELECT RAISE(ABORT, 'forced test failure'); END")
            .execute(&pool)
            .await
            .unwrap();
        let action = ProposedAction::CreateTask {
            project_id: "p".into(),
            title: "task".into(),
            description: String::new(),
            priority: 2,
        };
        assert!(save_ai_response(
            &pool,
            "request-rollback",
            "assistant-rollback",
            "c",
            "live_ai",
            "completed",
            "summary",
            &config,
            Some(&action),
        )
        .await
        .is_err());
        let assistants: i64 =
            sqlx::query_scalar("SELECT count(*) FROM ai_messages WHERE id='assistant-rollback'")
                .fetch_one(&pool)
                .await
                .unwrap();
        let request_status: String =
            sqlx::query_scalar("SELECT status FROM ai_requests WHERE id='request-rollback'")
                .fetch_one(&pool)
                .await
                .unwrap();
        let proposals: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM ai_action_proposals WHERE request_id='request-rollback'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(assistants, 0);
        assert_eq!(proposals, 0);
        assert_eq!(request_status, "processing");
    }

    #[tokio::test]
    async fn action_rejection_is_persistent_and_prevents_execution() {
        let pool = migrated_db().await;
        sqlx::query("INSERT INTO projects(id,name) VALUES('p1','Sunshine')")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO ai_conversations(id) VALUES('c')")
            .execute(&pool)
            .await
            .unwrap();
        let config = ProviderConfig {
            provider: "openai_compatible".into(),
            base_url: "http://127.0.0.1:1234/v1".into(),
            model: "test".into(),
            slot: "primary".into(),
        };
        let request_hash = request_payload_hash("c", "create a task").unwrap();
        begin_ai_request(
            &pool,
            "request-reject",
            "c",
            "create a task",
            &request_hash,
            &config,
        )
        .await
        .unwrap();
        let action = ProposedAction::CreateTask {
            project_id: "p1".into(),
            title: "Do not create".into(),
            description: String::new(),
            priority: 2,
        };
        save_ai_response(
            &pool,
            "request-reject",
            "assistant-reject",
            "c",
            "live_ai",
            "completed",
            "Proposed",
            &config,
            Some(&action),
        )
        .await
        .unwrap();
        let proposal_id: String = sqlx::query_scalar(
            "SELECT id FROM ai_action_proposals WHERE request_id='request-reject'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        let content_hash: String =
            sqlx::query_scalar("SELECT content_hash FROM ai_action_proposals WHERE id=?")
                .bind(&proposal_id)
                .fetch_one(&pool)
                .await
                .unwrap();
        let rejected =
            resolve_ai_action_proposal_in_pool(&pool, &proposal_id, &content_hash, false)
                .await
                .unwrap();
        assert_eq!(rejected.status, "rejected");
        assert!(
            resolve_ai_action_proposal_in_pool(&pool, &proposal_id, &content_hash, true)
                .await
                .is_err()
        );
        let tasks: i64 = sqlx::query_scalar("SELECT count(*) FROM tasks")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(tasks, 0);
    }

    #[tokio::test]
    async fn changed_proposal_content_requires_fresh_approval() {
        let pool = migrated_db().await;
        sqlx::query("INSERT INTO projects(id,name) VALUES('p1','Sunshine')")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO ai_conversations(id) VALUES('c')")
            .execute(&pool)
            .await
            .unwrap();
        let config = ProviderConfig {
            provider: "openai_compatible".into(),
            base_url: "http://127.0.0.1:1234/v1".into(),
            model: "test".into(),
            slot: "primary".into(),
        };
        let request_hash = request_payload_hash("c", "create a task").unwrap();
        begin_ai_request(
            &pool,
            "request-stale",
            "c",
            "create a task",
            &request_hash,
            &config,
        )
        .await
        .unwrap();
        let action = ProposedAction::CreateTask {
            project_id: "p1".into(),
            title: "Original title".into(),
            description: "Original details".into(),
            priority: 2,
        };
        save_ai_response(
            &pool,
            "request-stale",
            "assistant-stale",
            "c",
            "live_ai",
            "completed",
            "Proposed",
            &config,
            Some(&action),
        )
        .await
        .unwrap();
        let proposal_id: String = sqlx::query_scalar(
            "SELECT id FROM ai_action_proposals WHERE request_id='request-stale'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        let displayed_hash: String =
            sqlx::query_scalar("SELECT content_hash FROM ai_action_proposals WHERE id=?")
                .bind(&proposal_id)
                .fetch_one(&pool)
                .await
                .unwrap();
        let stale_version = resolve_ai_action_proposal_in_pool(
            &pool,
            &proposal_id,
            "stale-displayed-version",
            true,
        )
        .await
        .unwrap_err();
        assert!(stale_version.contains("changed after it was displayed"));
        let changed = ProposedAction::CreateTask {
            project_id: "p1".into(),
            title: "Changed title".into(),
            description: "Changed details".into(),
            priority: 2,
        };
        sqlx::query("UPDATE ai_action_proposals SET arguments_json=? WHERE id=?")
            .bind(serde_json::to_string(&changed).unwrap())
            .bind(&proposal_id)
            .execute(&pool)
            .await
            .unwrap();

        let error = resolve_ai_action_proposal_in_pool(&pool, &proposal_id, &displayed_hash, true)
            .await
            .unwrap_err();
        assert!(error.contains("changed after it was displayed"));
        let tasks: i64 = sqlx::query_scalar("SELECT count(*) FROM tasks")
            .fetch_one(&pool)
            .await
            .unwrap();
        let status: String =
            sqlx::query_scalar("SELECT status FROM ai_action_proposals WHERE id=?")
                .bind(&proposal_id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(tasks, 0);
        assert_eq!(status, "proposed");
    }

    #[tokio::test]
    async fn approved_action_executes_once_and_returns_persisted_result() {
        let pool = migrated_db().await;
        sqlx::query("INSERT INTO projects(id,name) VALUES('p1','Sunshine')")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO ai_conversations(id) VALUES('c')")
            .execute(&pool)
            .await
            .unwrap();
        let config = ProviderConfig {
            provider: "openai_compatible".into(),
            base_url: "http://127.0.0.1:1234/v1".into(),
            model: "test".into(),
            slot: "primary".into(),
        };
        let request_hash = request_payload_hash("c", "create a task").unwrap();
        begin_ai_request(
            &pool,
            "request-approve",
            "c",
            "create a task",
            &request_hash,
            &config,
        )
        .await
        .unwrap();
        let action = ProposedAction::CreateTask {
            project_id: "p1".into(),
            title: "Create from approved action".into(),
            description: "A test task".into(),
            priority: 2,
        };
        save_ai_response(
            &pool,
            "request-approve",
            "assistant-approve",
            "c",
            "live_ai",
            "completed",
            "Proposed",
            &config,
            Some(&action),
        )
        .await
        .unwrap();
        let proposal_id: String = sqlx::query_scalar(
            "SELECT id FROM ai_action_proposals WHERE request_id='request-approve'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        let content_hash: String =
            sqlx::query_scalar("SELECT content_hash FROM ai_action_proposals WHERE id=?")
                .bind(&proposal_id)
                .fetch_one(&pool)
                .await
                .unwrap();
        let executed = resolve_ai_action_proposal_in_pool(&pool, &proposal_id, &content_hash, true)
            .await
            .unwrap();
        assert_eq!(executed.status, "executed");
        assert!(executed
            .result_json
            .as_deref()
            .unwrap()
            .contains("ai-action-"));
        let (replay_one, replay_two) = tokio::join!(
            resolve_ai_action_proposal_in_pool(&pool, &proposal_id, &content_hash, true),
            resolve_ai_action_proposal_in_pool(&pool, &proposal_id, &content_hash, true),
        );
        assert_eq!(replay_one.unwrap().status, "executed");
        assert_eq!(replay_two.unwrap().status, "executed");
        let tasks: i64 = sqlx::query_scalar("SELECT count(*) FROM tasks")
            .fetch_one(&pool)
            .await
            .unwrap();
        let activity: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM activity_logs WHERE activity_type='task_created'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(tasks, 1);
        assert_eq!(activity, 1);
    }

    #[tokio::test]
    async fn action_domain_failure_rolls_back_record_history_and_approval() {
        let pool = migrated_db().await;
        sqlx::query("INSERT INTO projects(id,name) VALUES('p1','Sunshine')")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO ai_conversations(id) VALUES('c')")
            .execute(&pool)
            .await
            .unwrap();
        let config = ProviderConfig {
            provider: "openai_compatible".into(),
            base_url: "http://127.0.0.1:1234/v1".into(),
            model: "test".into(),
            slot: "primary".into(),
        };
        let request_hash = request_payload_hash("c", "create a task").unwrap();
        begin_ai_request(
            &pool,
            "request-domain-rollback",
            "c",
            "create a task",
            &request_hash,
            &config,
        )
        .await
        .unwrap();
        let action = ProposedAction::CreateTask {
            project_id: "p1".into(),
            title: "Rollback task".into(),
            description: String::new(),
            priority: 2,
        };
        save_ai_response(
            &pool,
            "request-domain-rollback",
            "assistant-domain-rollback",
            "c",
            "live_ai",
            "completed",
            "Proposed",
            &config,
            Some(&action),
        )
        .await
        .unwrap();
        let proposal_id: String = sqlx::query_scalar(
            "SELECT id FROM ai_action_proposals WHERE request_id='request-domain-rollback'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        let content_hash: String =
            sqlx::query_scalar("SELECT content_hash FROM ai_action_proposals WHERE id=?")
                .bind(&proposal_id)
                .fetch_one(&pool)
                .await
                .unwrap();
        sqlx::query("CREATE TRIGGER fail_task_history BEFORE INSERT ON task_history BEGIN SELECT RAISE(ABORT, 'forced history failure'); END")
            .execute(&pool)
            .await
            .unwrap();

        assert!(
            resolve_ai_action_proposal_in_pool(&pool, &proposal_id, &content_hash, true,)
                .await
                .is_err()
        );
        let tasks: i64 = sqlx::query_scalar("SELECT count(*) FROM tasks")
            .fetch_one(&pool)
            .await
            .unwrap();
        let history: i64 = sqlx::query_scalar("SELECT count(*) FROM task_history")
            .fetch_one(&pool)
            .await
            .unwrap();
        let activity: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM activity_logs WHERE activity_type='task_created'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        let status: String =
            sqlx::query_scalar("SELECT status FROM ai_action_proposals WHERE id=?")
                .bind(&proposal_id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(tasks, 0);
        assert_eq!(history, 0);
        assert_eq!(activity, 0);
        assert_eq!(status, "proposed");
    }

    #[test]
    fn credential_target_is_provider_and_endpoint_scoped_without_containing_url_text() {
        let first =
            validate_config("openai_compatible", "https://provider-one.example/v1", "m").unwrap();
        let second =
            validate_config("openai_compatible", "https://provider-two.example/v1", "m").unwrap();
        let anthro = validate_config("anthropic", "https://api.anthropic.com", "m").unwrap();
        let target = credential_user(&first, "primary");
        assert_ne!(target, credential_user(&second, "primary"));
        assert_ne!(target, credential_user(&anthro, "primary"));
        assert_ne!(target, credential_user(&first, "secondary"));
        assert!(!target.contains("provider-one"));
    }

    #[test]
    fn credential_status_distinguishes_store_unavailable_from_missing_configuration() {
        let empty = ProviderConfig {
            provider: "openai_compatible".into(),
            base_url: String::new(),
            model: String::new(),
            slot: "primary".into(),
        };
        let status = credential_diagnostics(&empty, "primary");
        if Entry::store_status().is_ok() {
            assert_eq!(status, (true, "configuration_missing", None));
        } else {
            assert_eq!(status.0, false);
            assert_eq!(status.1, "store_unavailable");
            assert!(status.2.is_some());
        }

        let invalid = ProviderConfig {
            base_url: "file:///unsupported".into(),
            ..empty
        };
        let status = credential_diagnostics(&invalid, "primary");
        if Entry::store_status().is_ok() {
            assert_eq!(status.0, true);
            assert_eq!(status.1, "configuration_invalid");
            assert_eq!(status.2.as_deref(), Some("invalid_provider_configuration"));
        } else {
            assert_eq!(status.1, "store_unavailable");
        }
    }

    #[test]
    fn credential_backend_errors_are_reduced_to_safe_categories() {
        assert_eq!(
            credential_error_category(&keyring::Error::NoDefaultStore),
            "backend_not_initialized"
        );
        assert_eq!(
            credential_error_category(&keyring::Error::NoEntry),
            "credential_not_found"
        );
    }

    #[tokio::test]
    async fn provider_config_save_persists_all_settings_and_fallback_preference() {
        let pool = db().await;
        let config = ProviderConfig {
            provider: "openai_compatible".into(),
            base_url: "http://127.0.0.1:31415/v1".into(),
            model: "gemini-3.5-flash".into(),
            slot: "primary".into(),
        };

        persist_provider_settings(&pool, "primary", &config)
            .await
            .unwrap();
        persist_manual_fallback_setting(&pool, true).await.unwrap();

        let rows: Vec<(String, String, String)> = sqlx::query_as(
            "SELECT key,value,updated_at FROM settings WHERE key IN ('ai_provider','ai_base_url','ai_model') ORDER BY key",
        )
        .fetch_all(&pool)
        .await
        .unwrap();
        assert_eq!(
            rows.iter()
                .map(|(key, value, _)| (key.as_str(), value.as_str()))
                .collect::<Vec<_>>(),
            vec![
                ("ai_base_url", "http://127.0.0.1:31415/v1"),
                ("ai_model", "gemini-3.5-flash"),
                ("ai_provider", "openai_compatible"),
            ]
        );
        assert!(rows.iter().all(|(_, _, updated_at)| !updated_at.is_empty()));
        let fallback: String =
            sqlx::query_scalar("SELECT value FROM settings WHERE key='ai_manual_fallback_enabled'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(fallback, "true");
        let secondary_rows: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM settings WHERE key IN ('ai_secondary_provider','ai_secondary_base_url','ai_secondary_model')",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(secondary_rows, 0);
    }

    #[tokio::test]
    async fn provider_config_save_rolls_back_partial_setting_writes() {
        let pool = db().await;
        let config = ProviderConfig {
            provider: "openai_compatible".into(),
            base_url: "http://127.0.0.1:31415/v1".into(),
            model: "gemini-3.5-flash".into(),
            slot: "primary".into(),
        };
        sqlx::query("CREATE TRIGGER fail_provider_url BEFORE INSERT ON settings WHEN NEW.key='ai_base_url' BEGIN SELECT RAISE(ABORT,'forced provider settings failure'); END")
            .execute(&pool)
            .await
            .unwrap();

        assert!(persist_provider_settings(&pool, "primary", &config)
            .await
            .is_err());
        let provider_rows: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM settings WHERE key IN ('ai_provider','ai_base_url','ai_model')",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(provider_rows, 0);
    }

    #[test]
    fn provider_profiles_have_separate_setting_slots_and_request_fingerprints() {
        assert_eq!(
            provider_setting_keys("primary"),
            ("ai_provider", "ai_base_url", "ai_model")
        );
        assert_eq!(
            provider_setting_keys("secondary"),
            (
                "ai_secondary_provider",
                "ai_secondary_base_url",
                "ai_secondary_model"
            )
        );
        assert!(normalize_provider_slot("tertiary").is_err());
        let mut primary =
            validate_config("openai_compatible", "https://gateway.example/v1", "model-x").unwrap();
        let primary_hash = request_payload_hash_for_provider("c", "same prompt", &primary).unwrap();
        primary.slot = "secondary".into();
        let secondary_hash =
            request_payload_hash_for_provider("c", "same prompt", &primary).unwrap();
        assert_ne!(primary_hash, secondary_hash);
    }

    #[tokio::test]
    async fn local_summary_and_handoff_prompts_are_persisted_and_scoped_to_records() {
        let pool = db().await;
        sqlx::query("INSERT INTO ai_conversations(id) VALUES('c')")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO projects(id,name,status,priority,updated_at) VALUES('p','Sunshine','active',1,datetime('now'))").execute(&pool).await.unwrap();
        sqlx::query("INSERT INTO tasks(id,title,status,priority,description,project_id,updated_at) VALUES('t','Finish home','ready',1,'Build deliverables','p',datetime('now'))").execute(&pool).await.unwrap();
        let (snapshot, _) = load_chat_context(&pool, "c").await.unwrap();
        assert!(snapshot.contains("Sunshine"));
        assert!(snapshot.contains("Finish home"));
        let message = save_local_message(&pool, "c", "local_summary", &snapshot)
            .await
            .unwrap();
        assert_eq!(message.response_kind, "local_summary");
        assert!(
            save_local_message(&pool, "missing", "handoff_prompt", "no row")
                .await
                .is_err()
        );
    }
}
