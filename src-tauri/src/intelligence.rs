use crate::browser::database;
use keyring::Entry;
use reqwest::{redirect::Policy, Client, StatusCode};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use sqlx::{Row, SqlitePool};
use tauri::AppHandle;
use url::{Host, Url};
use uuid::Uuid;

const CREDENTIAL_SERVICE: &str = "com.local.claude-command-center.ai";
const MAX_CHAT_CHARS: usize = 12_000;
const MAX_CONTEXT_CHARS: usize = 18_000;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiConfigInput {
    provider: String,
    base_url: String,
    model: String,
    #[serde(default)]
    api_key: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AiConfig {
    provider: String,
    base_url: String,
    model: String,
    has_api_key: bool,
    credential_store_available: bool,
    credential_store_error: Option<String>,
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

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SendMessageInput {
    conversation_id: String,
    message: String,
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
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ChatLine {
    role: String,
    content: String,
}

#[tauri::command]
pub async fn get_ai_config(app: AppHandle) -> Result<AiConfig, String> {
    get_config(&database(&app).await?).await
}

#[tauri::command]
pub async fn save_ai_config(app: AppHandle, input: AiConfigInput) -> Result<AiConfig, String> {
    let config = validate_config(&input.provider, &input.base_url, &input.model)?;
    if let Some(key) = input
        .api_key
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        save_secret(&config, key)?;
    }
    let pool = database(&app).await?;
    let mut tx = pool.begin_with("BEGIN IMMEDIATE").await.map_err(db_error)?;
    for (key, value) in [
        ("ai_provider", config.provider.as_str()),
        ("ai_base_url", config.base_url.as_str()),
        ("ai_model", config.model.as_str()),
    ] {
        sqlx::query("INSERT INTO settings(key,value) VALUES(?,?,datetime('now')) ON CONFLICT(key) DO UPDATE SET value=excluded.value,updated_at=datetime('now')")
            .bind(key).bind(value).execute(&mut *tx).await.map_err(db_error)?;
    }
    tx.commit().await.map_err(db_error)?;
    get_config(&pool).await
}

#[tauri::command]
pub async fn remove_ai_api_key(app: AppHandle) -> Result<(), String> {
    let config = read_provider_config(&database(&app).await?).await?;
    let entry = credential_entry(&config)?;
    match entry.delete_credential() {
        Ok(()) => Ok(()),
        Err(keyring::Error::NoEntry) => Ok(()),
        Err(_) => Err("Windows Credential Manager could not remove the LEVI provider key.".into()),
    }
}

#[tauri::command]
pub async fn test_ai_connection(app: AppHandle) -> Result<ConnectionResult, String> {
    let config = read_provider_config(&database(&app).await?).await?;
    let key = load_secret(&config)?;
    let output = send_provider_request(
        &config,
        &key,
        &[ChatLine {
            role: "user".into(),
            content: "Reply with the single word OK.".into(),
        }],
        "You are checking API connectivity. Reply briefly.",
    )
    .await?;
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
    sqlx::query_as("SELECT id,conversation_id,role,response_kind,content,provider,model,created_at FROM ai_messages WHERE conversation_id=? ORDER BY rowid LIMIT 500")
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
    let config = read_provider_config(&pool).await?;
    let key = load_secret(&config)?;
    let (context, history) = load_chat_context(&pool, &input.conversation_id).await?;
    let user_id = Uuid::new_v4().to_string();
    let assistant_id = Uuid::new_v4().to_string();
    let title = message.chars().take(72).collect::<String>();
    let mut tx = pool.begin_with("BEGIN IMMEDIATE").await.map_err(db_error)?;
    let exists: i64 = sqlx::query_scalar("SELECT count(*) FROM ai_conversations WHERE id=?")
        .bind(&input.conversation_id)
        .fetch_one(&mut *tx)
        .await
        .map_err(db_error)?;
    if exists == 0 {
        return Err("This conversation no longer exists. Start a new conversation.".into());
    }
    sqlx::query("INSERT INTO ai_messages(id,conversation_id,role,response_kind,content,provider,model) VALUES(?,?,'user','live_ai',?,?,?)")
        .bind(&user_id).bind(&input.conversation_id).bind(message).bind(&config.provider).bind(&config.model)
        .execute(&mut *tx).await.map_err(db_error)?;
    sqlx::query("UPDATE ai_conversations SET title=CASE WHEN title='New conversation' THEN ? ELSE title END,updated_at=datetime('now') WHERE id=?")
        .bind(title).bind(&input.conversation_id).execute(&mut *tx).await.map_err(db_error)?;
    tx.commit().await.map_err(db_error)?;

    let mut lines = history;
    lines.push(ChatLine {
        role: "user".into(),
        content: message.to_owned(),
    });
    let system = format!("You are LEVI Command Intelligence for LEVI COMMAND CENTER. Use only the supplied saved workforce snapshot when making claims about this workspace. State uncertainty where records are missing. Recommend assignments and handoffs, but never say an action was performed unless the saved data proves it. You have no tools and cannot execute SQL, shell commands, browser actions, assignments, reminders, or external handoffs. For consequential actions, describe the exact proposed action and direct the user to the relevant Command Center UI for confirmation. Distinguish application-record facts from your analysis.\n\nCURRENT SAVED WORKFORCE SNAPSHOT (read-only, curated by the Rust backend):\n{}", context);
    match send_provider_request(&config, &key, &lines, &system).await {
        Ok(content) => {
            save_assistant_message(
                &pool,
                &assistant_id,
                &input.conversation_id,
                "live_ai",
                &content,
                &config,
            )
            .await?;
            sqlx::query_as("SELECT id,conversation_id,role,response_kind,content,provider,model,created_at FROM ai_messages WHERE id=?")
                .bind(assistant_id).fetch_one(&pool).await.map_err(db_error)
        }
        Err(error) => {
            save_assistant_message(
                &pool,
                &assistant_id,
                &input.conversation_id,
                "error",
                &format!("Provider request failed: {error}"),
                &config,
            )
            .await?;
            Err(error)
        }
    }
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
    sqlx::query_as("SELECT id,conversation_id,role,response_kind,content,provider,model,created_at FROM ai_messages WHERE id=?")
        .bind(id).fetch_one(pool).await.map_err(db_error)
}

async fn save_assistant_message(
    pool: &SqlitePool,
    id: &str,
    conversation_id: &str,
    kind: &str,
    content: &str,
    config: &ProviderConfig,
) -> Result<(), String> {
    let mut tx = pool.begin_with("BEGIN IMMEDIATE").await.map_err(db_error)?;
    sqlx::query("INSERT INTO ai_messages(id,conversation_id,role,response_kind,content,provider,model) VALUES(?,?,'assistant',?,?,?,?)")
        .bind(id).bind(conversation_id).bind(kind).bind(content).bind(&config.provider).bind(&config.model)
        .execute(&mut *tx).await.map_err(db_error)?;
    sqlx::query("UPDATE ai_conversations SET updated_at=datetime('now') WHERE id=?")
        .bind(conversation_id)
        .execute(&mut *tx)
        .await
        .map_err(db_error)?;
    tx.commit().await.map_err(db_error)
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
    let projects: Vec<Value> = sqlx::query("SELECT name,status,priority,deadline FROM projects WHERE archived_at IS NULL ORDER BY priority DESC,updated_at DESC LIMIT 30")
        .fetch_all(pool).await.map_err(db_error)?.into_iter().map(|r| json!({"name":r.get::<String,_>("name"),"status":r.get::<String,_>("status"),"priority":r.get::<i64,_>("priority"),"deadline":r.get::<Option<String>,_>("deadline")})).collect();
    let tasks: Vec<Value> = sqlx::query("SELECT t.title,t.status,t.priority,t.deadline,substr(t.blocker,1,500) AS blocker,substr(t.description,1,600) AS description,p.name AS project,a.name AS worker,(SELECT group_concat(d.title || ' [' || d.status || ']', ', ') FROM task_dependencies dep JOIN tasks d ON d.id=dep.depends_on_task_id WHERE dep.task_id=t.id AND d.status!='completed') AS dependency_blockers,(SELECT group_concat(DISTINCT cl.path_display || ' claimed by ' || ct.title) FROM task_file_declarations decl JOIN task_file_claims cl ON cl.project_id=t.project_id AND cl.status='active' AND cl.task_id!=t.id AND (lower(decl.path_key)=lower(cl.path_key) OR substr(lower(decl.path_key),1,length(cl.path_key)+1)=lower(cl.path_key)||'/' OR substr(lower(cl.path_key),1,length(decl.path_key)+1)=lower(decl.path_key)||'/') JOIN tasks ct ON ct.id=cl.task_id WHERE decl.task_id=t.id) AS file_claim_conflicts FROM tasks t LEFT JOIN projects p ON p.id=t.project_id LEFT JOIN accounts a ON a.id=t.account_id WHERE t.archived_at IS NULL ORDER BY CASE t.status WHEN 'blocked' THEN 0 WHEN 'waiting' THEN 1 WHEN 'ready' THEN 2 ELSE 3 END,t.priority DESC,t.updated_at DESC LIMIT 40")
        .fetch_all(pool).await.map_err(db_error)?.into_iter().map(|r| json!({"title":r.get::<String,_>("title"),"status":r.get::<String,_>("status"),"priority":r.get::<i64,_>("priority"),"deadline":r.get::<Option<String>,_>("deadline"),"blocker":r.get::<String,_>("blocker"),"description":r.get::<String,_>("description"),"project":r.get::<Option<String>,_>("project"),"worker":r.get::<Option<String>,_>("worker"),"incompleteDependencies":r.get::<Option<String>,_>("dependency_blockers"),"activeFileClaimConflicts":r.get::<Option<String>,_>("file_claim_conflicts")})).collect();
    let accounts: Vec<Value> = sqlx::query("SELECT a.name,a.status,a.archived_at,b.name AS browser,bp.profile_name AS profile FROM accounts a LEFT JOIN browser_profiles bp ON bp.id=a.browser_profile_id LEFT JOIN browsers b ON b.id=bp.browser_id ORDER BY a.archived_at,a.name COLLATE NOCASE LIMIT 60")
        .fetch_all(pool).await.map_err(db_error)?.into_iter().map(|r| json!({"name":r.get::<String,_>("name"),"status":r.get::<String,_>("status"),"archived":r.get::<Option<String>,_>("archived_at").is_some(),"browser":r.get::<Option<String>,_>("browser"),"profile":r.get::<Option<String>,_>("profile")})).collect();
    let reminders: Vec<Value> = sqlx::query("SELECT title,due_at,status,kind FROM reminders WHERE status IN ('pending','snoozed') ORDER BY COALESCE(snoozed_until,due_at) LIMIT 40")
        .fetch_all(pool).await.map_err(db_error)?.into_iter().map(|r| json!({"title":r.get::<String,_>("title"),"dueAt":r.get::<String,_>("due_at"),"status":r.get::<String,_>("status"),"kind":r.get::<String,_>("kind")})).collect();
    let deliverables: Vec<Value> = sqlx::query("SELECT d.title,d.notes,d.completed_at,p.name AS project,t.title AS task FROM deliverables d LEFT JOIN projects p ON p.id=d.project_id LEFT JOIN tasks t ON t.id=d.task_id ORDER BY d.completed_at DESC LIMIT 30")
        .fetch_all(pool).await.map_err(db_error)?.into_iter().map(|r| json!({"title":r.get::<String,_>("title"),"notes":r.get::<String,_>("notes"),"completedAt":r.get::<String,_>("completed_at"),"project":r.get::<Option<String>,_>("project"),"task":r.get::<Option<String>,_>("task")})).collect();
    let mut snapshot = json!({"projects":projects,"tasks":tasks,"accounts":accounts,"upcomingReminders":reminders,"deliverables":deliverables});
    while snapshot.to_string().chars().count() > MAX_CONTEXT_CHARS {
        let mut removed = false;
        for key in [
            "deliverables",
            "upcomingReminders",
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

async fn get_config(pool: &SqlitePool) -> Result<AiConfig, String> {
    let rows: Vec<(String, String)> = sqlx::query_as(
        "SELECT key,value FROM settings WHERE key IN ('ai_provider','ai_base_url','ai_model')",
    )
    .fetch_all(pool)
    .await
    .map_err(db_error)?;
    let mut provider = "openai_compatible".to_owned();
    let mut base_url = String::new();
    let mut model = String::new();
    for (key, value) in rows {
        match key.as_str() {
            "ai_provider" => provider = value,
            "ai_base_url" => base_url = value,
            "ai_model" => model = value,
            _ => {}
        }
    }
    let identity = ProviderConfig {
        provider: provider.clone(),
        base_url: base_url.clone(),
        model: model.clone(),
    };
    let credential_status = Entry::store_status();
    Ok(AiConfig {
        provider,
        base_url,
        model,
        has_api_key: has_secret(&identity),
        credential_store_available: credential_status.is_ok(),
        credential_store_error: credential_status.as_ref().err().map(ToString::to_string),
    })
}

async fn read_provider_config(pool: &SqlitePool) -> Result<ProviderConfig, String> {
    let rows: Vec<(String, String)> = sqlx::query_as(
        "SELECT key,value FROM settings WHERE key IN ('ai_provider','ai_base_url','ai_model')",
    )
    .fetch_all(pool)
    .await
    .map_err(db_error)?;
    let mut provider = "openai_compatible".to_owned();
    let mut base_url = String::new();
    let mut model = String::new();
    for (key, value) in rows {
        match key.as_str() {
            "ai_provider" => provider = value,
            "ai_base_url" => base_url = value,
            "ai_model" => model = value,
            _ => {}
        }
    }
    validate_config(&provider, &base_url, &model)
}

fn validate_config(provider: &str, base_url: &str, model: &str) -> Result<ProviderConfig, String> {
    if !matches!(provider, "openai_compatible" | "anthropic") {
        return Err("Choose OpenAI-compatible or Anthropic as the provider type.".into());
    }
    if model.trim().is_empty() || model.trim().len() > 120 {
        return Err("Enter a model identifier between 1 and 120 characters.".into());
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
    Ok(ProviderConfig {
        provider: provider.into(),
        base_url: base_url.trim().trim_end_matches('/').into(),
        model: model.trim().into(),
    })
}

fn credential_user(config: &ProviderConfig) -> String {
    let digest = Sha256::digest(format!("{}|{}", config.provider, config.base_url).as_bytes());
    format!(
        "{}-{}",
        config.provider,
        digest
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>()
    )
}
fn credential_entry(config: &ProviderConfig) -> Result<Entry, String> {
    Entry::new(CREDENTIAL_SERVICE, &credential_user(config)).map_err(|_| "Windows Credential Manager is unavailable. LEVI cannot store an API key securely on this device.".into())
}
fn save_secret(config: &ProviderConfig, secret: &str) -> Result<(), String> {
    if secret.len() > 2048 || secret.chars().any(char::is_control) {
        return Err("API key is invalid or exceeds the supported length.".into());
    }
    credential_entry(config)?.set_password(secret).map_err(|_| "Windows Credential Manager could not save the provider API key. No plaintext fallback was used.".into())
}
fn load_secret(config: &ProviderConfig) -> Result<String, String> {
    credential_entry(config)?
        .get_password()
        .map_err(|e| match e {
            keyring::Error::NoEntry => {
                "Save an API key for this provider URL in AI settings before connecting.".into()
            }
            _ => "Windows Credential Manager could not read the provider API key.".into(),
        })
}
fn has_secret(config: &ProviderConfig) -> bool {
    credential_entry(config)
        .ok()
        .and_then(|entry| entry.get_password().ok())
        .is_some()
}

async fn send_provider_request(
    config: &ProviderConfig,
    key: &str,
    messages: &[ChatLine],
    system: &str,
) -> Result<String, String> {
    let client = Client::builder()
        .timeout(std::time::Duration::from_secs(45))
        .redirect(Policy::none())
        .build()
        .map_err(|_| "Could not initialize the secure HTTP client.".to_owned())?;
    let url = if config.provider == "anthropic" {
        format!("{}/v1/messages", config.base_url)
    } else {
        format!("{}/chat/completions", config.base_url)
    };
    let (request, response_value) = if config.provider == "anthropic" {
        let body =
            json!({"model":config.model,"max_tokens":1024,"system":system,"messages":messages});
        let response = client
            .post(url)
            .header("x-api-key", key)
            .header("anthropic-version", "2023-06-01")
            .json(&body)
            .send()
            .await
            .map_err(|_| {
                "Could not reach the configured provider. Check the network and base URL."
                    .to_owned()
            })?;
        let status = response.status();
        let bytes = response
            .bytes()
            .await
            .map_err(|_| "The provider response could not be read.".to_owned())?;
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
        let body = json!({"model":config.model,"messages":lines,"max_tokens":1024});
        let response = client
            .post(url)
            .bearer_auth(key)
            .json(&body)
            .send()
            .await
            .map_err(|_| {
                "Could not reach the configured provider. Check the network and base URL."
                    .to_owned()
            })?;
        let status = response.status();
        let bytes = response
            .bytes()
            .await
            .map_err(|_| "The provider response could not be read.".to_owned())?;
        (
            status,
            serde_json::from_slice::<Value>(&bytes).unwrap_or(Value::Null),
        )
    };
    if !request.is_success() {
        return Err(provider_error(request));
    }
    parse_response(&config.provider, &response_value)
}

fn parse_response(provider: &str, value: &Value) -> Result<String, String> {
    let text = if provider == "anthropic" {
        value.get("content").and_then(Value::as_array).map(|items| {
            items
                .iter()
                .filter_map(|item| item.get("text").and_then(Value::as_str))
                .collect::<Vec<_>>()
                .join("\n")
        })
    } else {
        value
            .pointer("/choices/0/message/content")
            .and_then(Value::as_str)
            .map(str::to_owned)
    };
    text.filter(|s| !s.trim().is_empty())
        .ok_or_else(|| "Provider returned an unsupported or empty response format.".into())
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
    use sqlx::sqlite::SqlitePoolOptions;

    async fn db() -> SqlitePool {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::raw_sql("CREATE TABLE settings(key TEXT PRIMARY KEY,value TEXT NOT NULL,updated_at TEXT NOT NULL DEFAULT (datetime('now'))); CREATE TABLE ai_conversations(id TEXT PRIMARY KEY,title TEXT NOT NULL DEFAULT 'New conversation',created_at TEXT NOT NULL DEFAULT (datetime('now')),updated_at TEXT NOT NULL DEFAULT (datetime('now'))); CREATE TABLE ai_messages(id TEXT PRIMARY KEY,conversation_id TEXT NOT NULL REFERENCES ai_conversations(id),role TEXT NOT NULL,response_kind TEXT NOT NULL,content TEXT NOT NULL,provider TEXT,model TEXT,created_at TEXT NOT NULL DEFAULT (datetime('now'))); CREATE TABLE projects(id TEXT PRIMARY KEY,name TEXT,status TEXT,priority INTEGER,deadline TEXT,archived_at TEXT,updated_at TEXT); CREATE TABLE tasks(id TEXT PRIMARY KEY,title TEXT,status TEXT,priority INTEGER,deadline TEXT,blocker TEXT,description TEXT,project_id TEXT,account_id TEXT,archived_at TEXT,updated_at TEXT); CREATE TABLE accounts(id TEXT PRIMARY KEY,name TEXT,status TEXT,archived_at TEXT,browser_profile_id TEXT); CREATE TABLE browser_profiles(id TEXT PRIMARY KEY,profile_name TEXT,browser_id TEXT); CREATE TABLE browsers(id TEXT PRIMARY KEY,name TEXT); CREATE TABLE reminders(id TEXT PRIMARY KEY,title TEXT,due_at TEXT,status TEXT,kind TEXT,snoozed_until TEXT); CREATE TABLE deliverables(id TEXT PRIMARY KEY,title TEXT,notes TEXT,completed_at TEXT,project_id TEXT,task_id TEXT); CREATE TABLE task_handoffs(id TEXT PRIMARY KEY,task_id TEXT,package_text TEXT,created_at TEXT); CREATE TABLE task_dependencies(task_id TEXT,depends_on_task_id TEXT); CREATE TABLE task_file_declarations(task_id TEXT,path_key TEXT); CREATE TABLE task_file_claims(project_id TEXT,task_id TEXT,path_key TEXT,path_display TEXT,status TEXT);").execute(&pool).await.unwrap();
        pool
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
    fn response_parsing_and_errors_never_include_provider_body() {
        assert_eq!(
            parse_response(
                "openai_compatible",
                &json!({"choices":[{"message":{"content":"hello"}}]})
            )
            .unwrap(),
            "hello"
        );
        assert_eq!(
            parse_response(
                "anthropic",
                &json!({"content":[{"type":"text","text":"hi"}]})
            )
            .unwrap(),
            "hi"
        );
        assert!(provider_error(StatusCode::UNAUTHORIZED).contains("rejected the API key"));
        assert!(!provider_error(StatusCode::INTERNAL_SERVER_ERROR).contains("secret"));
    }

    #[test]
    fn credential_target_is_provider_and_endpoint_scoped_without_containing_url_text() {
        let first =
            validate_config("openai_compatible", "https://provider-one.example/v1", "m").unwrap();
        let second =
            validate_config("openai_compatible", "https://provider-two.example/v1", "m").unwrap();
        let anthro = validate_config("anthropic", "https://api.anthropic.com", "m").unwrap();
        let target = credential_user(&first);
        assert_ne!(target, credential_user(&second));
        assert_ne!(target, credential_user(&anthro));
        assert!(!target.contains("provider-one"));
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
