use serde::{Deserialize, Serialize};
#[cfg(test)]
use sqlx::sqlite::SqlitePoolOptions;
use sqlx::{sqlite::SqliteConnectOptions, FromRow, SqlitePool};
use std::{
    collections::HashSet,
    env,
    path::{Path, PathBuf},
    process::Command,
    time::Duration,
};
#[cfg(windows)]
use sysinfo::{ProcessRefreshKind, ProcessesToUpdate, System};
use tauri::AppHandle;
use tauri::Manager;
use url::Url;
use uuid::Uuid;

const BROWSER_FAMILIES: [&str; 5] = ["chrome", "edge", "brave", "opera", "firefox"];

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountInput {
    id: String,
    name: String,
    identifier: Option<String>,
    browser_profile_id: String,
    conversation_url: Option<String>,
    status: String,
    priority: i64,
    notes: String,
    project_id: Option<String>,
}

#[tauri::command]
pub async fn save_account(
    app: AppHandle,
    account: AccountInput,
    is_update: bool,
) -> Result<(), String> {
    let name = account.name.trim();
    if name.is_empty() || name.chars().count() > 80 {
        return Err("Account name must contain 1–80 characters.".into());
    }
    if !(1..=4).contains(&account.priority) {
        return Err("Priority must be between 1 and 4.".into());
    }
    if !matches!(
        account.status.as_str(),
        "working" | "waiting" | "idle" | "blocked" | "completed" | "needs_attention"
    ) {
        return Err("Choose a valid account status.".into());
    }
    let identifier = account
        .identifier
        .as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty());
    if identifier.is_some_and(|value| value.chars().count() > 254)
        || account.notes.chars().count() > 5000
    {
        return Err("Identifier or notes exceed the supported length.".into());
    }
    let conversation_url = account
        .conversation_url
        .as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty());
    if let Some(value) = conversation_url {
        validate_claude_url(value)?;
    }

    let pool = database(&app).await?;
    save_account_transaction(&pool, &account, is_update).await
}

async fn save_account_transaction(
    pool: &SqlitePool,
    account: &AccountInput,
    is_update: bool,
) -> Result<(), String> {
    let name = account.name.trim();
    let identifier = account
        .identifier
        .as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty());
    let conversation_url = account
        .conversation_url
        .as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty());
    let mut tx = pool.begin().await.map_err(db_error)?;
    let profile_available: Option<i64> = sqlx::query_scalar(
        "SELECT 1 FROM browser_profiles bp JOIN browsers b ON b.id=bp.browser_id WHERE bp.id=? AND bp.archived_at IS NULL AND bp.availability_status='available' AND b.availability_status='available'"
    ).bind(&account.browser_profile_id).fetch_optional(&mut *tx).await.map_err(db_error)?;
    if profile_available.is_none() {
        return Err("Choose an available registered browser profile.".into());
    }
    if let Some(project_id) = account.project_id.as_deref().filter(|v| !v.is_empty()) {
        let project_available: Option<i64> = sqlx::query_scalar(
            "SELECT 1 FROM projects WHERE id=? AND archived_at IS NULL AND status='active'",
        )
        .bind(project_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(db_error)?;
        if project_available.is_none() {
            return Err("Choose an active project.".into());
        }
    }
    let existing: Option<i64> =
        sqlx::query_scalar("SELECT 1 FROM accounts WHERE id=? AND archived_at IS NULL")
            .bind(&account.id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(db_error)?;
    if is_update && existing.is_none() {
        return Err("That account no longer exists or is archived.".into());
    }
    if !is_update && existing.is_some() {
        return Err("An account with that ID already exists.".into());
    }

    if is_update {
        sqlx::query("UPDATE accounts SET name=?,identifier=?,browser_profile_id=?,conversation_url=?,status=?,priority=?,notes=?,updated_at=datetime('now') WHERE id=?")
            .bind(name).bind(identifier).bind(&account.browser_profile_id).bind(conversation_url).bind(&account.status).bind(account.priority).bind(&account.notes).bind(&account.id)
            .execute(&mut *tx).await.map_err(db_error)?;
    } else {
        sqlx::query("INSERT INTO accounts(id,name,identifier,browser_profile_id,conversation_url,status,priority,notes) VALUES(?,?,?,?,?,?,?,?)")
            .bind(&account.id).bind(name).bind(identifier).bind(&account.browser_profile_id).bind(conversation_url).bind(&account.status).bind(account.priority).bind(&account.notes)
            .execute(&mut *tx).await.map_err(db_error)?;
    }
    sqlx::query("DELETE FROM project_accounts WHERE account_id=?")
        .bind(&account.id)
        .execute(&mut *tx)
        .await
        .map_err(db_error)?;
    if let Some(project_id) = account.project_id.as_deref().filter(|v| !v.is_empty()) {
        sqlx::query("INSERT INTO project_accounts(project_id,account_id) VALUES(?,?)")
            .bind(project_id)
            .bind(&account.id)
            .execute(&mut *tx)
            .await
            .map_err(db_error)?;
    }
    sqlx::query("INSERT INTO activity_logs(id,activity_type,description,source,account_id) VALUES(?,?,?,'manual',?)")
        .bind(Uuid::new_v4().to_string()).bind(if is_update { "account_updated" } else { "account_created" })
        .bind(format!("{} account {}", if is_update { "Updated" } else { "Created" }, name)).bind(&account.id)
        .execute(&mut *tx).await.map_err(db_error)?;
    tx.commit().await.map_err(db_error)
}

#[tauri::command]
pub async fn archive_account(app: AppHandle, account_id: String) -> Result<(), String> {
    let pool = database(&app).await?;
    archive_account_transaction(&pool, &account_id).await
}

async fn archive_account_transaction(pool: &SqlitePool, account_id: &str) -> Result<(), String> {
    let mut tx = pool.begin().await.map_err(db_error)?;
    let result = sqlx::query("UPDATE accounts SET archived_at=datetime('now'),updated_at=datetime('now') WHERE id=? AND archived_at IS NULL")
        .bind(&account_id).execute(&mut *tx).await.map_err(db_error)?;
    if result.rows_affected() == 0 {
        return Err("That account no longer exists or is already archived.".into());
    }
    sqlx::query("INSERT INTO activity_logs(id,activity_type,description,source,account_id) VALUES(?,'account_archived',?,'manual',?)")
        .bind(Uuid::new_v4().to_string()).bind("Archived account").bind(&account_id).execute(&mut *tx).await.map_err(db_error)?;
    tx.commit().await.map_err(db_error)
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DetectedProfile {
    pub profile_name: String,
    pub profile_path: String,
    pub user_data_directory: Option<String>,
    pub availability_status: String,
    pub detection_method: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DetectedBrowser {
    pub family: String,
    pub name: String,
    pub executable_path: String,
    pub browser_version: Option<String>,
    pub detection_method: String,
    pub profiles: Vec<DetectedProfile>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ValidatedPath {
    pub canonical_path: String,
    pub browser_version: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfileAvailability {
    pub id: String,
    pub profile_name: String,
    pub profile_path: String,
    pub availability_status: String,
}

#[derive(Debug, FromRow)]
struct BrowserRow {
    family: String,
}

#[derive(Debug, FromRow)]
struct BrowserRegistrationRow {
    name: String,
    family: String,
}

#[derive(Debug, FromRow)]
struct RegisteredProfilePath {
    id: String,
    browser_name: String,
    browser_family: String,
    profile_name: String,
    profile_path: String,
    archived_at: Option<String>,
}

#[derive(Debug, FromRow)]
struct ProfileRow {
    id: String,
    profile_name: String,
    profile_path: String,
}

#[derive(Debug, FromRow)]
struct LaunchRow {
    account_id: String,
    account_name: String,
    conversation_url: Option<String>,
    browser_family: String,
    executable_path: String,
    browser_availability: String,
    profile_id: String,
    profile_name: String,
    profile_path: String,
    user_data_directory: Option<String>,
    profile_availability: String,
}

#[derive(Debug, Clone)]
struct LaunchSpec {
    executable: PathBuf,
    args: Vec<String>,
    profile_name: String,
    profile_path: String,
    requested_url: String,
    reused_existing_session: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LaunchFeedback {
    executable_path: String,
    profile_name: String,
    profile_path: String,
    process_id: u32,
    process_created: bool,
    executable_validated: bool,
    profile_validated: bool,
    profile_conflict_check: String,
    url_navigation_requested: bool,
    launch_outcome: String,
    message: String,
}

#[tauri::command]
pub fn detect_browsers() -> Vec<DetectedBrowser> {
    BROWSER_FAMILIES
        .iter()
        .filter_map(|family| {
            find_executable(family).map(|(path, method)| DetectedBrowser {
                family: (*family).to_owned(),
                name: browser_name(family).to_owned(),
                browser_version: read_browser_version(&path),
                detection_method: method,
                profiles: discover_profiles_for(family),
                executable_path: path.to_string_lossy().into_owned(),
            })
        })
        .collect()
}

#[tauri::command]
pub fn validate_browser_path(
    family: String,
    executable_path: String,
) -> Result<ValidatedPath, String> {
    if !BROWSER_FAMILIES.contains(&family.as_str()) {
        return Err("Choose Chrome, Edge, Brave, Opera, or Firefox.".into());
    }
    let path = PathBuf::from(executable_path.trim());
    if !path.is_file()
        || !path
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("exe"))
    {
        return Err("The browser executable path must point to an existing .exe file.".into());
    }
    let path = path
        .canonicalize()
        .map_err(|_| "The browser executable path could not be resolved.".to_string())?;
    let browser_version = read_browser_version(&path);
    Ok(ValidatedPath {
        canonical_path: path.to_string_lossy().into_owned(),
        browser_version,
    })
}

#[tauri::command]
pub async fn discover_profiles(
    app: AppHandle,
    browser_id: String,
) -> Result<Vec<DetectedProfile>, String> {
    let pool = database(&app).await?;
    let browser = sqlx::query_as::<_, BrowserRow>(
        "SELECT family FROM browsers WHERE id=? AND availability_status != 'archived'",
    )
    .bind(browser_id)
    .fetch_optional(&pool)
    .await
    .map_err(db_error)?
    .ok_or_else(|| "That browser is no longer registered.".to_string())?;
    Ok(discover_profiles_for(&browser.family))
}

#[tauri::command]
pub async fn register_browser_profile(
    app: AppHandle,
    browser_id: String,
    profile_name: String,
    profile_path: String,
    user_data_directory: Option<String>,
    detection_method: String,
) -> Result<DetectedProfile, String> {
    let pool = database(&app).await?;
    register_browser_profile_transaction(
        &pool,
        &browser_id,
        &profile_name,
        &profile_path,
        user_data_directory.as_deref(),
        &detection_method,
    )
    .await
}

async fn register_browser_profile_transaction(
    pool: &SqlitePool,
    browser_id: &str,
    profile_name: &str,
    profile_path: &str,
    user_data_directory: Option<&str>,
    detection_method: &str,
) -> Result<DetectedProfile, String> {
    let profile_name = profile_name.trim();
    if profile_name.is_empty() || profile_name.chars().count() > 100 {
        return Err("Profile name must contain 1–100 characters.".into());
    }
    if !matches!(detection_method, "manual" | "automatic") {
        return Err(
            "Profile registration method is invalid. Please retry from Browser Profiles.".into(),
        );
    }

    // BEGIN IMMEDIATE serializes profile registrations before the duplicate-path
    // check, so a concurrent registration cannot pass the same check in parallel.
    let mut tx = pool.begin_with("BEGIN IMMEDIATE").await.map_err(db_error)?;
    let browser = sqlx::query_as::<_, BrowserRegistrationRow>(
        "SELECT name,family FROM browsers WHERE id=? AND availability_status != 'archived'",
    )
    .bind(browser_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(db_error)?
    .ok_or_else(|| "That browser is no longer registered or is archived.".to_string())?;

    let checked = validate_profile_paths(
        &browser.family,
        profile_path,
        user_data_directory,
        detection_method,
    )?;
    let requested_key = normalized_profile_path_key(&checked.profile_path);

    let registered = sqlx::query_as::<_, RegisteredProfilePath>(
        "SELECT bp.id,b.name AS browser_name,b.family AS browser_family,bp.profile_name,bp.profile_path,bp.archived_at FROM browser_profiles bp JOIN browsers b ON b.id=bp.browser_id WHERE bp.browser_id != ? ORDER BY (bp.archived_at IS NULL) DESC,bp.created_at",
    )
    .bind(browser_id)
    .fetch_all(&mut *tx)
    .await
    .map_err(db_error)?;

    if let Some(existing) = registered.into_iter().find(|row| {
        row.archived_at.is_none() && normalized_profile_path_key(&row.profile_path) == requested_key
    }) {
        return Err(format!(
            "This directory is already registered as the active profile '{}' for {} ({}). It cannot also be registered for {} ({}). Select a different directory or use the existing browser registration. No profile record was changed.",
            existing.profile_name,
            existing.browser_name,
            existing.browser_family,
            browser.name,
            browser.family,
        ));
    }

    let same_browser_profiles = sqlx::query_as::<_, RegisteredProfilePath>(
        "SELECT bp.id,b.name AS browser_name,b.family AS browser_family,bp.profile_name,bp.profile_path,bp.archived_at FROM browser_profiles bp JOIN browsers b ON b.id=bp.browser_id WHERE bp.browser_id=? ORDER BY (bp.archived_at IS NULL) DESC,bp.created_at",
    )
    .bind(browser_id)
    .fetch_all(&mut *tx)
    .await
    .map_err(db_error)?;
    if let Some(existing) = same_browser_profiles
        .into_iter()
        .find(|row| normalized_profile_path_key(&row.profile_path) == requested_key)
    {
        sqlx::query("UPDATE browser_profiles SET profile_name=?,profile_path=?,user_data_directory=?,detection_method=?,availability_status='available',archived_at=NULL,updated_at=datetime('now') WHERE id=?")
            .bind(profile_name)
            .bind(&checked.profile_path)
            .bind(&checked.user_data_directory)
            .bind(detection_method)
            .bind(existing.id)
            .execute(&mut *tx)
            .await
            .map_err(db_error)?;
    } else {
        sqlx::query("INSERT INTO browser_profiles(id,browser_id,profile_name,profile_path,user_data_directory,detection_method,availability_status) VALUES(?,?,?,?,?,?,?)")
            .bind(Uuid::new_v4().to_string())
            .bind(browser_id)
            .bind(profile_name)
            .bind(&checked.profile_path)
            .bind(&checked.user_data_directory)
            .bind(detection_method)
            .bind("available")
            .execute(&mut *tx)
            .await
            .map_err(db_error)?;
    }
    tx.commit().await.map_err(db_error)?;
    Ok(checked)
}

#[cfg(windows)]
fn normalized_profile_path_key(path: &str) -> String {
    let canonical = Path::new(path)
        .canonicalize()
        .unwrap_or_else(|_| PathBuf::from(path));
    normalize_windows_path_text(&canonical.to_string_lossy())
}

#[cfg(not(windows))]
fn normalized_profile_path_key(path: &str) -> String {
    let canonical = Path::new(path)
        .canonicalize()
        .unwrap_or_else(|_| PathBuf::from(path));
    normalize_windows_path_text(&canonical.to_string_lossy())
}

fn normalize_windows_path_text(path: &str) -> String {
    let replaced = path.replace('/', "\\");
    let unprefixed = replaced
        .strip_prefix("\\\\?\\UNC\\")
        .map(|rest| format!("\\\\{rest}"))
        .or_else(|| replaced.strip_prefix("\\\\?\\").map(str::to_owned))
        .unwrap_or(replaced);

    let (prefix, rest) = if let Some(rest) = unprefixed.strip_prefix("\\\\") {
        ("\\\\".to_owned(), rest)
    } else if unprefixed.len() >= 3
        && unprefixed.as_bytes()[1] == b':'
        && unprefixed.as_bytes()[2] == b'\\'
    {
        (unprefixed[..3].to_owned(), &unprefixed[3..])
    } else if unprefixed.starts_with('\\') {
        ("\\".to_owned(), unprefixed.trim_start_matches('\\'))
    } else {
        (String::new(), unprefixed.as_str())
    };

    let mut parts: Vec<&str> = Vec::new();
    for component in rest.split('\\') {
        match component {
            "" | "." => {}
            ".." if parts.last().is_some_and(|part| *part != "..") => {
                parts.pop();
            }
            ".." if prefix.is_empty() => parts.push(component),
            ".." => {}
            value => parts.push(value),
        }
    }
    let mut normalized = format!("{prefix}{}", parts.join("\\"));
    if cfg!(windows) {
        // Windows filesystems are case-insensitive; Unicode path case-folding is
        // not needed for browser profile paths used by this application.
        normalized = normalized.to_lowercase();
    }
    normalized.trim_end_matches('\\').to_owned()
}

#[tauri::command]
pub async fn refresh_profile_availability(
    app: AppHandle,
    browser_id: String,
) -> Result<Vec<ProfileAvailability>, String> {
    let pool = database(&app).await?;
    let browser_path: String =
        sqlx::query_scalar("SELECT executable_path FROM browsers WHERE id=?")
            .bind(&browser_id)
            .fetch_optional(&pool)
            .await
            .map_err(db_error)?
            .ok_or_else(|| "That browser is no longer registered.".to_string())?;
    let rows = sqlx::query_as::<_, ProfileRow>("SELECT id,profile_name,profile_path FROM browser_profiles WHERE browser_id=? AND archived_at IS NULL")
        .bind(&browser_id).fetch_all(&pool).await.map_err(db_error)?;
    let mut refreshed = Vec::with_capacity(rows.len());
    for row in rows {
        let status = if Path::new(&row.profile_path).is_dir() {
            "available"
        } else {
            "unavailable"
        };
        sqlx::query("UPDATE browser_profiles SET availability_status=?,updated_at=datetime('now') WHERE id=?")
            .bind(status).bind(&row.id).execute(&pool).await.map_err(db_error)?;
        refreshed.push(ProfileAvailability {
            id: row.id,
            profile_name: row.profile_name,
            profile_path: row.profile_path,
            availability_status: status.to_owned(),
        });
    }
    let browser_status = if Path::new(&browser_path).is_file() {
        "available"
    } else {
        "unavailable"
    };
    sqlx::query("UPDATE browsers SET availability_status=?,updated_at=datetime('now') WHERE id=?")
        .bind(browser_status)
        .bind(browser_id)
        .execute(&pool)
        .await
        .map_err(db_error)?;
    Ok(refreshed)
}

#[tauri::command]
pub async fn launch_account(app: AppHandle, account_id: String) -> Result<LaunchFeedback, String> {
    let pool = database(&app).await?;
    let record = sqlx::query_as::<_, LaunchRow>(
        "SELECT a.id AS account_id,a.name AS account_name,a.conversation_url,b.family AS browser_family,b.executable_path,b.availability_status AS browser_availability,bp.id AS profile_id,bp.profile_name,bp.profile_path,bp.user_data_directory,bp.availability_status AS profile_availability FROM accounts a JOIN browser_profiles bp ON bp.id=a.browser_profile_id JOIN browsers b ON b.id=bp.browser_id WHERE a.id=? AND a.archived_at IS NULL AND bp.archived_at IS NULL")
        .bind(&account_id).fetch_optional(&pool).await.map_err(db_error)?
        .ok_or_else(|| "This account or its browser profile is no longer available.".to_string())?;
    if record.browser_availability != "available" {
        return Err(
            "The registered browser is unavailable. Check its executable path in Browser Profiles."
                .into(),
        );
    }
    if record.profile_availability != "available" {
        return Err("The registered browser profile is unavailable. Rescan or update its profile path before opening this account.".into());
    }
    let url = record
        .conversation_url
        .as_deref()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or("https://claude.ai/");
    validate_claude_url(url)?;
    let spec = make_launch_spec(
        &record.browser_family,
        &record.executable_path,
        &record.profile_path,
        record.user_data_directory.as_deref(),
        &record.profile_name,
        url,
    )?;
    let process_id = spec.spawn().map_err(|error| spec.spawn_error(error))?;
    let mut feedback = spec.feedback(process_id);
    if let Err(error) = record_launch(&pool, &record, process_id).await {
        feedback
            .message
            .push_str(&format!(" Activity logging failed: {error}"));
    }
    Ok(feedback)
}

#[tauri::command]
pub async fn test_profile_launch(
    app: AppHandle,
    profile_id: String,
    test_url: String,
) -> Result<LaunchFeedback, String> {
    let pool = database(&app).await?;
    let row = sqlx::query_as::<_, (String, String, String, Option<String>, String, String)>(
        "SELECT b.family,b.executable_path,b.availability_status,bp.user_data_directory,bp.profile_name,bp.profile_path FROM browser_profiles bp JOIN browsers b ON b.id=bp.browser_id WHERE bp.id=? AND bp.archived_at IS NULL")
        .bind(&profile_id).fetch_optional(&pool).await.map_err(db_error)?
        .ok_or_else(|| "That profile is no longer registered.".to_string())?;
    if row.2 != "available" {
        return Err(
            "The browser is unavailable. Check its executable path before testing this profile."
                .into(),
        );
    }
    let spec = make_launch_spec(&row.0, &row.1, &row.5, row.3.as_deref(), &row.4, &test_url)?;
    let process_id = spec.spawn().map_err(|error| spec.spawn_error(error))?;
    let mut feedback = spec.feedback(process_id);
    if let Err(error) = sqlx::query("UPDATE browser_profiles SET last_accessed_at=datetime('now'),availability_status='available',updated_at=datetime('now') WHERE id=?")
        .bind(profile_id).execute(&pool).await.map_err(db_error)
    {
        feedback.message.push_str(&format!(" Profile access time could not be saved: {error}"));
    }
    Ok(feedback)
}

impl LaunchSpec {
    fn spawn(&self) -> Result<u32, String> {
        let mut command = Command::new(&self.executable);
        command.args(&self.args);
        command
            .spawn()
            .map(|child| child.id())
            .map_err(|e| e.to_string())
    }

    fn spawn_error(&self, error: String) -> String {
        format!(
            "Launch requested. Executable validated: {}. Profile validated: {}. URL navigation requested for {}. Process creation failed: {error}. Launch outcome is unknown. Check the executable, profile permissions, and any browser conflict; close the test browser manually if it opened.",
            self.executable.display(), self.profile_name, self.requested_url
        )
    }

    fn feedback(&self, process_id: u32) -> LaunchFeedback {
        let profile_conflict_check = if self.reused_existing_session {
            "An exact Chromium profile match was found; the request uses that same browser session."
                .to_owned()
        } else {
            "No matching active profile process was detected at check time.".to_owned()
        };
        let forwarding_note = if self.reused_existing_session {
            " The reported PID belongs to this start request; Chromium may forward it to the existing session."
        } else {
            ""
        };
        LaunchFeedback {
            executable_path: self.executable.to_string_lossy().into_owned(),
            profile_name: self.profile_name.clone(),
            profile_path: self.profile_path.clone(),
            process_id,
            process_created: true,
            executable_validated: true,
            profile_validated: true,
            profile_conflict_check: profile_conflict_check.clone(),
            url_navigation_requested: true,
            launch_outcome: "unknown".into(),
            message: format!(
                "Launch requested. Executable validated: {}. Profile validated: {} at {}. {profile_conflict_check} Process creation succeeded (PID {process_id}) for {}. URL navigation requested: {}. Launch outcome is unknown until you verify the browser window and page.{forwarding_note} No authentication was checked. Close the test browser manually.",
                self.executable.display(), self.profile_name, self.profile_path, self.executable.display(), self.requested_url
            ),
        }
    }
}

fn make_launch_spec(
    family: &str,
    executable: &str,
    profile_path: &str,
    user_data_directory: Option<&str>,
    profile_name: &str,
    url: &str,
) -> Result<LaunchSpec, String> {
    let executable = validate_browser_path(family.to_owned(), executable.to_owned())?;
    let profile = validate_profile_paths(family, profile_path, user_data_directory, "registered")?;
    let url = validate_profile_launch_url(url).map_err(|error| {
        format!(
            "Launch request rejected. Executable validated: {}. Profile validated: {}. URL validation failed: {error}. No browser process was started.",
            executable.canonical_path, profile.profile_name
        )
    })?;
    let reused_existing_session = ensure_profile_not_locked(
        family,
        &executable.canonical_path,
        &profile.profile_path,
        profile.user_data_directory.as_deref(),
    )
    .map_err(|error| {
        format!(
            "Launch requested. Executable validated: {}. Profile validated: {}. Potential profile conflict: {error}. URL navigation was not requested because the launch was blocked.",
            executable.canonical_path, profile.profile_name
        )
    })?;
    let _display_name = profile_name;
    let mut args = Vec::new();
    match family {
        "chrome" | "edge" | "brave" | "opera" => {
            let root = profile.user_data_directory.ok_or_else(|| {
                "This Chromium profile is missing its user-data directory.".to_string()
            })?;
            args.push(format!("--user-data-dir={root}"));
            let profile_directory = Path::new(&profile.profile_path)
                .file_name()
                .and_then(|v| v.to_str())
                .ok_or_else(|| "The Chromium profile directory name is invalid.".to_string())?;
            args.push(format!("--profile-directory={profile_directory}"));
            args.push("--new-window".into());
        }
        "firefox" => {
            args.push("-no-remote".into());
            args.push("-profile".into());
            args.push(profile.profile_path.clone());
            args.push("-new-window".into());
        }
        _ => return Err("This browser family is not supported for profile launching.".into()),
    }
    args.push(url.clone());
    Ok(LaunchSpec {
        executable: PathBuf::from(executable.canonical_path),
        args,
        profile_name: profile_name.to_owned(),
        profile_path: profile.profile_path,
        requested_url: url,
        reused_existing_session,
    })
}

fn validate_claude_url(value: &str) -> Result<String, String> {
    let url = Url::parse(value.trim())
        .map_err(|_| "Enter a valid HTTPS Claude conversation URL.".to_string())?;
    if url.scheme() != "https"
        || url.host_str() != Some("claude.ai")
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return Err("For safety, account URLs must use https://claude.ai/ and cannot contain embedded credentials.".into());
    }
    Ok(url.to_string())
}

fn validate_profile_launch_url(value: &str) -> Result<String, String> {
    match value {
        "about:blank" | "https://example.com/" => Ok(value.to_owned()),
        _ => validate_claude_url(value),
    }
}

#[cfg(windows)]
fn read_browser_version(path: &Path) -> Option<String> {
    use std::ffi::c_void;

    #[repr(C)]
    struct FixedFileInfo {
        signature: u32,
        structure_version: u32,
        file_version_ms: u32,
        file_version_ls: u32,
        remaining: [u32; 9],
    }

    #[link(name = "version")]
    unsafe extern "system" {
        fn GetFileVersionInfoSizeW(file_name: *const u16, handle: *mut u32) -> u32;
        fn GetFileVersionInfoW(
            file_name: *const u16,
            handle: u32,
            length: u32,
            data: *mut c_void,
        ) -> i32;
        fn VerQueryValueW(
            block: *const c_void,
            sub_block: *const u16,
            value: *mut *mut c_void,
            length: *mut u32,
        ) -> i32;
    }

    // `canonicalize()` may return Win32's extended `\\?\` prefix. The
    // version resource APIs do not consistently accept that namespace, so
    // pass the ordinary DOS path when it is representable.
    let path_text = path.to_string_lossy();
    let version_path = path_text.strip_prefix("\\\\?\\").unwrap_or(&path_text);
    let file_name: Vec<u16> = version_path.encode_utf16().chain(Some(0)).collect();
    let mut handle = 0;
    let length = unsafe { GetFileVersionInfoSizeW(file_name.as_ptr(), &mut handle) };
    if length == 0 {
        return None;
    }
    let mut block = vec![0u8; length as usize];
    if unsafe {
        GetFileVersionInfoW(
            file_name.as_ptr(),
            handle,
            length,
            block.as_mut_ptr().cast(),
        )
    } == 0
    {
        return None;
    }
    let query: Vec<u16> = "\\\0".encode_utf16().collect();
    let mut value = std::ptr::null_mut();
    let mut value_length = 0;
    if unsafe {
        VerQueryValueW(
            block.as_ptr().cast(),
            query.as_ptr(),
            &mut value,
            &mut value_length,
        )
    } == 0
        || value_length < std::mem::size_of::<FixedFileInfo>() as u32
    {
        return None;
    }
    let info = unsafe { &*value.cast::<FixedFileInfo>() };
    if info.signature != 0xFEEF04BD {
        return None;
    }
    Some(format!(
        "{}.{}.{}.{}",
        info.file_version_ms >> 16,
        info.file_version_ms & 0xFFFF,
        info.file_version_ls >> 16,
        info.file_version_ls & 0xFFFF
    ))
}

#[cfg(not(windows))]
fn read_browser_version(_path: &Path) -> Option<String> {
    None
}

fn ensure_profile_not_locked(
    family: &str,
    executable: &str,
    profile_path: &str,
    user_data_directory: Option<&str>,
) -> Result<bool, String> {
    let reuse_existing =
        matching_browser_process(family, executable, profile_path, user_data_directory)?;
    let lock_paths: Vec<PathBuf> = match family {
        "chrome" | "edge" | "brave" | "opera" => {
            let root = user_data_directory
                .ok_or_else(|| "The Chromium user-data directory is missing.".to_string())?;
            ["SingletonLock", "SingletonCookie", "SingletonSocket"]
                .iter()
                .map(|name| Path::new(root).join(name))
                .collect()
        }
        "firefox" => ["parent.lock", ".parentlock"]
            .iter()
            .map(|name| Path::new(profile_path).join(name))
            .collect(),
        _ => return Err("This browser family cannot be safely checked for profile locks.".into()),
    };
    if let Some(marker) = lock_paths
        .iter()
        .find(|path| std::fs::symlink_metadata(path).is_ok())
    {
        if reuse_existing {
            return Ok(true);
        }
        return Err(format!(
            "This browser profile appears to be in use (lock marker: {}). LEVI did not launch another process or alter the marker. Close the browser normally, then retry. A lock marker by itself cannot confirm which process owns the profile.",
            marker.display()
        ));
    }
    Ok(reuse_existing)
}

#[cfg(windows)]
fn matching_browser_process(
    family: &str,
    executable: &str,
    profile_path: &str,
    user_data_directory: Option<&str>,
) -> Result<bool, String> {
    let expected_exe = Path::new(executable);
    let mut system = System::new().map_err(|_| "Windows could not enumerate running processes. Close the browser normally before launching this profile. LEVI did not stop any process.".to_string())?;
    system.refresh_processes_specifics(
        ProcessesToUpdate::All,
        true,
        ProcessRefreshKind::nothing()
            .with_exe(sysinfo::UpdateKind::OnlyIfNotSet)
            .with_cmd(sysinfo::UpdateKind::OnlyIfNotSet),
    );
    let expected_profile = normalize_process_path(profile_path);
    let expected_root = user_data_directory.map(normalize_process_path);
    for process in system.processes().values() {
        let Some(process_exe) = process.exe() else {
            continue;
        };
        if !normalize_process_path(&process_exe.to_string_lossy())
            .eq(&normalize_process_path(&expected_exe.to_string_lossy()))
        {
            continue;
        }
        let args = process.cmd();
        if args.is_empty() {
            return Err("A browser process is already running, but Windows did not expose its command line. Close that browser normally before launching this profile. LEVI did not stop it.".into());
        }
        match family {
            "chrome" | "edge" | "brave" | "opera" => {
                let root = expected_root
                    .as_deref()
                    .ok_or_else(|| "The Chromium user-data directory is missing.".to_string())?;
                let running_root =
                    argument_value(args, "--user-data-dir").map(|v| normalize_process_path(&v));
                let running_profile = argument_value(args, "--profile-directory");
                if running_root.as_deref() == Some(root) {
                    if running_profile.as_deref()
                        == Path::new(profile_path).file_name().and_then(|v| v.to_str())
                    {
                        return Ok(true);
                    }
                    return Err("This browser is already using the same user-data directory with a different or unconfirmed profile. Close that browser normally before launching this profile. LEVI did not open another profile.".into());
                }
            }
            "firefox" => {
                let running_profile =
                    argument_value(args, "-profile").map(|v| normalize_process_path(&v));
                if running_profile.as_deref() == Some(expected_profile.as_str()) {
                    return Err("This Firefox profile is already running. Close that Firefox window normally before using it here. LEVI did not launch another process.".into());
                }
                if running_profile.is_none() && !args.iter().any(|arg| arg == "-no-remote") {
                    return Err("Firefox is running and its profile could not be identified reliably. Close Firefox normally before launching a registered profile. LEVI did not stop it.".into());
                }
            }
            _ => {
                return Err("This browser family cannot be safely checked for profile use.".into())
            }
        }
    }
    Ok(false)
}

#[cfg(not(windows))]
fn matching_browser_process(
    _family: &str,
    _executable: &str,
    _profile_path: &str,
    _user_data_directory: Option<&str>,
) -> Result<bool, String> {
    Err("Reliable browser process-to-profile detection is available only on Windows. Close the browser manually before launching a registered profile.".into())
}

#[cfg(windows)]
fn argument_value(args: &[std::ffi::OsString], name: &str) -> Option<String> {
    let prefix = format!("{name}=");
    args.iter().enumerate().find_map(|(index, arg)| {
        let value = arg.to_string_lossy();
        if let Some(value) = value.strip_prefix(&prefix) {
            Some(value.trim_matches('"').to_owned())
        } else if value == name {
            args.get(index + 1)
                .map(|next| next.to_string_lossy().trim_matches('"').to_owned())
        } else {
            None
        }
    })
}

#[cfg(windows)]
fn normalize_process_path(value: &str) -> String {
    value
        .trim_matches('"')
        .replace('/', "\\")
        .trim_end_matches('\\')
        .to_lowercase()
}

fn validate_profile_paths(
    family: &str,
    profile_path: &str,
    user_data_directory: Option<&str>,
    detection_method: &str,
) -> Result<DetectedProfile, String> {
    if !BROWSER_FAMILIES.contains(&family) {
        return Err("Choose a supported browser family.".into());
    }
    let path = PathBuf::from(profile_path.trim());
    if !path.is_dir() {
        return Err(
            "The browser profile path must be an existing directory. No browser data was changed."
                .into(),
        );
    }
    let canonical_path = path
        .canonicalize()
        .map_err(|_| "The browser profile directory could not be resolved.".to_string())?;
    let profile_name = canonical_path
        .file_name()
        .and_then(|v| v.to_str())
        .filter(|s| !s.is_empty())
        .ok_or_else(|| "The browser profile folder needs a valid name.".to_string())?
        .to_owned();
    let canonical_user_data = if matches!(family, "chrome" | "edge" | "brave" | "opera") {
        let root = PathBuf::from(
            user_data_directory
                .ok_or_else(|| {
                    "Select the browser's user-data directory for this Chromium profile."
                        .to_string()
                })?
                .trim(),
        );
        if !root.is_dir() {
            return Err("The browser user-data directory must already exist.".into());
        }
        let root = root
            .canonicalize()
            .map_err(|_| "The browser user-data directory could not be resolved.".to_string())?;
        if canonical_path.parent() != Some(root.as_path()) {
            return Err("For Chromium browsers, select a profile folder directly inside its existing user-data directory.".into());
        }
        Some(root.to_string_lossy().into_owned())
    } else {
        None
    };
    Ok(DetectedProfile {
        profile_name,
        availability_status: "available".into(),
        profile_path: canonical_path.to_string_lossy().into_owned(),
        user_data_directory: canonical_user_data,
        detection_method: detection_method.to_owned(),
    })
}

pub(crate) async fn database(app: &AppHandle) -> Result<SqlitePool, String> {
    let path = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("Could not locate the LEVI data directory: {e}"))?
        .join("command-center.db");
    SqlitePool::connect_with(
        SqliteConnectOptions::new()
            .filename(path)
            .create_if_missing(false)
            .foreign_keys(true)
            .busy_timeout(Duration::from_secs(5)),
    )
    .await
    .map_err(db_error)
}

async fn record_launch(
    pool: &SqlitePool,
    record: &LaunchRow,
    process_id: u32,
) -> Result<(), String> {
    let mut tx = pool.begin().await.map_err(db_error)?;
    sqlx::query("UPDATE accounts SET last_accessed_at=datetime('now'),updated_at=datetime('now') WHERE id=?")
        .bind(&record.account_id).execute(&mut *tx).await.map_err(db_error)?;
    sqlx::query("UPDATE browser_profiles SET last_accessed_at=datetime('now'),availability_status='available',updated_at=datetime('now') WHERE id=?")
        .bind(&record.profile_id).execute(&mut *tx).await.map_err(db_error)?;
    sqlx::query("UPDATE browsers SET last_accessed_at=datetime('now'),availability_status='available',updated_at=datetime('now') WHERE executable_path=?")
        .bind(&record.executable_path).execute(&mut *tx).await.map_err(db_error)?;
    sqlx::query("INSERT INTO activity_logs(id,activity_type,description,source,account_id) VALUES(?, 'account_launch_requested', ?, 'system', ?)")
        .bind(Uuid::new_v4().to_string())
        .bind(format!("Process start requested for {} in {} profile {} (PID {}; executable {}). Browser window and URL were not confirmed.", record.account_name, browser_name(&record.browser_family), record.profile_name, process_id, record.executable_path))
        .bind(&record.account_id).execute(&mut *tx).await.map_err(db_error)?;
    tx.commit().await.map_err(db_error)
}

fn db_error(error: impl std::fmt::Display) -> String {
    format!("Local database error: {error}")
}

fn browser_name(family: &str) -> &'static str {
    match family {
        "chrome" => "Google Chrome",
        "edge" => "Microsoft Edge",
        "brave" => "Brave",
        "opera" => "Opera",
        "firefox" => "Firefox",
        _ => "Browser",
    }
}

fn discover_profiles_for(family: &str) -> Vec<DetectedProfile> {
    match family {
        "chrome" | "edge" | "brave" | "opera" => chromium_user_data_dirs(family)
            .into_iter()
            .flat_map(|root| parse_chromium_profiles(&root))
            .collect(),
        "firefox" => firefox_profiles_ini()
            .into_iter()
            .flat_map(|ini| parse_firefox_profiles(&ini))
            .collect(),
        _ => Vec::new(),
    }
}

fn chromium_user_data_dirs(family: &str) -> Vec<PathBuf> {
    let local = env::var_os("LOCALAPPDATA").map(PathBuf::from);
    let roaming = env::var_os("APPDATA").map(PathBuf::from);
    let paths = match family {
        "chrome" => local
            .into_iter()
            .map(|p| p.join("Google/Chrome/User Data"))
            .collect(),
        "edge" => local
            .into_iter()
            .map(|p| p.join("Microsoft/Edge/User Data"))
            .collect(),
        "brave" => local
            .into_iter()
            .map(|p| p.join("BraveSoftware/Brave-Browser/User Data"))
            .collect(),
        "opera" => roaming
            .into_iter()
            .map(|p| p.join("Opera Software/Opera Stable"))
            .collect(),
        _ => Vec::new(),
    };
    paths
        .into_iter()
        .filter(|p| p.join("Local State").is_file())
        .collect()
}

fn parse_chromium_profiles(root: &Path) -> Vec<DetectedProfile> {
    let Ok(text) = std::fs::read_to_string(root.join("Local State")) else {
        return Vec::new();
    };
    let Ok(state) = serde_json::from_str::<serde_json::Value>(&text) else {
        return Vec::new();
    };
    let Some(cache) = state
        .get("profile")
        .and_then(|v| v.get("info_cache"))
        .and_then(|v| v.as_object())
    else {
        return Vec::new();
    };
    let canonical_root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    cache
        .iter()
        .filter_map(|(key, metadata)| {
            if key.contains('/') || key.contains('\\') || key == "." || key == ".." {
                return None;
            }
            let path = canonical_root.join(key);
            let name = metadata
                .get("name")
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
                .unwrap_or(key);
            Some(DetectedProfile {
                profile_name: name.to_owned(),
                availability_status: if path.is_dir() {
                    "available"
                } else {
                    "unavailable"
                }
                .into(),
                profile_path: path.to_string_lossy().into_owned(),
                user_data_directory: Some(canonical_root.to_string_lossy().into_owned()),
                detection_method: "automatic".into(),
            })
        })
        .collect()
}

fn firefox_profiles_ini() -> Vec<PathBuf> {
    env::var_os("APPDATA")
        .map(|p| vec![PathBuf::from(p).join("Mozilla/Firefox/profiles.ini")])
        .unwrap_or_default()
}

fn parse_firefox_profiles(ini_path: &Path) -> Vec<DetectedProfile> {
    let Ok(text) = std::fs::read_to_string(ini_path) else {
        return Vec::new();
    };
    let root = ini_path.parent().unwrap_or_else(|| Path::new("."));
    let mut profiles = Vec::new();
    let mut name: Option<String> = None;
    let mut path: Option<String> = None;
    let mut relative = true;
    let flush = |name: &mut Option<String>,
                 path: &mut Option<String>,
                 relative: bool,
                 profiles: &mut Vec<DetectedProfile>| {
        if let (Some(name), Some(path)) = (name.take(), path.take()) {
            let profile_path = PathBuf::from(path);
            let profile_path = if relative {
                root.join(profile_path)
            } else {
                profile_path
            };
            let canonical = profile_path.canonicalize().unwrap_or(profile_path);
            profiles.push(DetectedProfile {
                profile_name: name,
                availability_status: if canonical.is_dir() {
                    "available"
                } else {
                    "unavailable"
                }
                .into(),
                profile_path: canonical.to_string_lossy().into_owned(),
                user_data_directory: None,
                detection_method: "automatic".into(),
            });
        }
    };
    for line in text.lines().map(str::trim) {
        if line.starts_with('[') && line.ends_with(']') {
            flush(&mut name, &mut path, relative, &mut profiles);
            relative = true;
        } else if let Some((key, value)) = line.split_once('=') {
            match key.trim() {
                "Name" => name = Some(value.trim().to_owned()),
                "Path" => path = Some(value.trim().to_owned()),
                "IsRelative" => relative = value.trim() != "0",
                _ => {}
            }
        }
    }
    flush(&mut name, &mut path, relative, &mut profiles);
    profiles
}

fn find_executable(family: &str) -> Option<(PathBuf, String)> {
    let mut candidates = registry_executables(family);
    candidates.extend(
        common_executable_paths(family)
            .into_iter()
            .map(|p| (p, "standard location".into())),
    );
    if let Some(path) = env::var_os("PATH") {
        let exe = primary_exe(family);
        candidates.extend(env::split_paths(&path).map(|p| (p.join(exe), "PATH".into())));
    }
    let mut seen = HashSet::new();
    candidates.into_iter().find_map(|(candidate, method)| {
        if !candidate.is_file() {
            return None;
        }
        let canonical = candidate.canonicalize().unwrap_or(candidate);
        let key = canonical.to_string_lossy().to_lowercase();
        if seen.insert(key) {
            Some((canonical, method))
        } else {
            None
        }
    })
}

fn primary_exe(family: &str) -> &'static str {
    match family {
        "chrome" => "chrome.exe",
        "edge" => "msedge.exe",
        "brave" => "brave.exe",
        "opera" => "opera.exe",
        _ => "firefox.exe",
    }
}

#[cfg(windows)]
fn registry_executables(family: &str) -> Vec<(PathBuf, String)> {
    use winreg::{
        enums::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE},
        RegKey,
    };
    let exe = primary_exe(family);
    let mut out = Vec::new();
    for root in [HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE] {
        for prefix in [
            "SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\App Paths",
            "SOFTWARE\\WOW6432Node\\Microsoft\\Windows\\CurrentVersion\\App Paths",
        ] {
            let key = format!("{prefix}\\{exe}");
            if let Ok(entry) = RegKey::predef(root).open_subkey(&key) {
                if let Ok(value) = entry.get_value::<String, _>("") {
                    out.push((
                        PathBuf::from(value.trim_matches('"')),
                        "Windows registry".into(),
                    ));
                }
            }
        }
        if family == "opera" {
            for name in ["launcher.exe", "opera.exe"] {
                if let Ok(entry) = RegKey::predef(root).open_subkey(format!(
                    "SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\App Paths\\{name}"
                )) {
                    if let Ok(value) = entry.get_value::<String, _>("") {
                        out.push((
                            PathBuf::from(value.trim_matches('"')),
                            "Windows registry".into(),
                        ));
                    }
                }
            }
        }
    }
    out
}

#[cfg(not(windows))]
fn registry_executables(_family: &str) -> Vec<(PathBuf, String)> {
    Vec::new()
}

fn common_executable_paths(family: &str) -> Vec<PathBuf> {
    let mut roots = Vec::new();
    for key in [
        "ProgramFiles",
        "ProgramFiles(x86)",
        "LOCALAPPDATA",
        "APPDATA",
    ] {
        if let Some(value) = env::var_os(key) {
            roots.push(PathBuf::from(value));
        }
    }
    let tails: &[&str] = match family {
        "chrome" => &[
            "Google/Chrome/Application/chrome.exe",
            "Google/Chrome Beta/Application/chrome.exe",
        ],
        "edge" => &["Microsoft/Edge/Application/msedge.exe"],
        "brave" => &["BraveSoftware/Brave-Browser/Application/brave.exe"],
        "opera" => &[
            "Programs/Opera/opera.exe",
            "Opera Software/Opera Stable/opera.exe",
            "Programs/Opera/launcher.exe",
        ],
        "firefox" => &[
            "Mozilla Firefox/firefox.exe",
            "Programs/Mozilla Firefox/firefox.exe",
        ],
        _ => &[],
    };
    roots
        .into_iter()
        .flat_map(|root| tails.iter().map(move |tail| root.join(tail)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn chromium_profile_parser_uses_profile_labels_only() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir(dir.path().join("Default")).unwrap();
        fs::write(dir.path().join("Local State"), r#"{"profile":{"info_cache":{"Default":{"name":"Personal","user_name":"person@example.com"}}}}"#).unwrap();
        let profiles = parse_chromium_profiles(dir.path());
        assert_eq!(profiles.len(), 1);
        assert_eq!(profiles[0].profile_name, "Personal");
        assert_eq!(profiles[0].availability_status, "available");
        assert!(!serde_json::to_string(&profiles[0])
            .unwrap()
            .contains("example.com"));
    }

    #[test]
    fn firefox_profiles_ini_supports_relative_and_absolute_locations() {
        let dir = tempfile::tempdir().unwrap();
        let profiles = dir.path().join("Profiles");
        fs::create_dir_all(profiles.join("abc.default")).unwrap();
        let ini = dir.path().join("profiles.ini");
        fs::write(&ini, "[Profile0]\nName=Default\nIsRelative=1\nPath=Profiles/abc.default\n[Profile1]\nName=Work\nIsRelative=0\nPath=C:/Profiles/Work\n").unwrap();
        let result = parse_firefox_profiles(&ini);
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].profile_name, "Default");
        assert_eq!(result[0].availability_status, "available");
        assert_eq!(result[1].profile_name, "Work");
    }

    #[test]
    fn account_url_is_limited_to_https_claude_ai() {
        assert_eq!(
            validate_claude_url("https://claude.ai/chat/abc").unwrap(),
            "https://claude.ai/chat/abc"
        );
        assert!(validate_claude_url("http://claude.ai/").is_err());
        assert!(validate_claude_url("https://example.com/").is_err());
        assert!(validate_claude_url("https://user:pass@claude.ai/").is_err());
        assert!(validate_claude_url("not a url").is_err());
        assert_eq!(
            validate_profile_launch_url("https://example.com/").unwrap(),
            "https://example.com/"
        );
        assert!(validate_profile_launch_url("https://example.com/extra").is_err());
    }

    #[test]
    fn invalid_browser_and_profile_paths_return_actionable_errors() {
        let temp = tempfile::tempdir().unwrap();
        let missing_exe = temp.path().join("missing-browser.exe");
        let error = validate_browser_path("chrome".into(), missing_exe.to_string_lossy().into())
            .unwrap_err();
        assert!(error.contains("existing .exe file"));

        let missing_profile = temp.path().join("missing-profile");
        let error =
            validate_profile_paths("chrome", missing_profile.to_str().unwrap(), None, "manual")
                .unwrap_err();
        assert!(error.contains("existing directory"));

        let profile = temp.path().join("Profile 1");
        fs::create_dir(&profile).unwrap();
        let error = validate_profile_paths("chrome", profile.to_str().unwrap(), None, "manual")
            .unwrap_err();
        assert!(error.contains("user-data directory"));

        let error =
            validate_profile_paths("unsupported", profile.to_str().unwrap(), None, "manual")
                .unwrap_err();
        assert!(error.contains("supported browser family"));
    }

    #[test]
    fn process_start_feedback_keeps_browser_outcome_unknown() {
        let spec = LaunchSpec {
            executable: PathBuf::from(r"C:\Browsers\chrome.exe"),
            args: vec!["https://example.com/".into()],
            profile_name: "Temporary test".into(),
            profile_path: r"C:\Temp\levi-test\Profile 1".into(),
            requested_url: "https://example.com/".into(),
            reused_existing_session: false,
        };
        let feedback = spec.feedback(4242);
        assert_eq!(feedback.process_id, 4242);
        assert!(feedback.process_created);
        assert!(feedback.executable_validated);
        assert!(feedback.profile_validated);
        assert!(feedback.url_navigation_requested);
        assert_eq!(feedback.launch_outcome, "unknown");
        assert!(feedback.message.contains("Process creation succeeded"));
        assert!(feedback.message.contains("Launch outcome is unknown"));
        assert!(feedback.message.contains("Close the test browser manually"));
    }

    #[test]
    fn chromium_launch_uses_exact_registered_profile_arguments() {
        let temp = tempfile::tempdir().unwrap();
        let user_data = temp.path().join("User Data");
        let profile = user_data.join("Profile 2");
        fs::create_dir_all(&profile).unwrap();
        let exe = temp.path().join("chrome.exe");
        fs::write(&exe, b"test executable").unwrap();
        let spec = make_launch_spec(
            "chrome",
            exe.to_str().unwrap(),
            profile.to_str().unwrap(),
            Some(user_data.to_str().unwrap()),
            "Profile 2",
            "https://claude.ai/",
        )
        .unwrap();
        assert!(spec.args.contains(&format!(
            "--user-data-dir={}",
            user_data.canonicalize().unwrap().display()
        )));
        assert!(spec
            .args
            .contains(&"--profile-directory=Profile 2".to_owned()));
        assert_eq!(spec.args.last().unwrap(), "https://claude.ai/");
    }

    #[test]
    fn firefox_launch_isolated_and_never_falls_back_to_default_profile() {
        let temp = tempfile::tempdir().unwrap();
        let profile = temp.path().join("firefox-profile");
        fs::create_dir(&profile).unwrap();
        let exe = temp.path().join("firefox.exe");
        fs::write(&exe, b"test executable").unwrap();
        let spec = make_launch_spec(
            "firefox",
            exe.to_str().unwrap(),
            profile.to_str().unwrap(),
            None,
            "firefox-profile",
            "https://claude.ai/",
        )
        .unwrap();
        assert_eq!(spec.args[0], "-no-remote");
        assert_eq!(spec.args[1], "-profile");
        assert_eq!(
            spec.args[2],
            profile.canonicalize().unwrap().to_string_lossy()
        );
    }

    #[test]
    fn chromium_profile_must_be_direct_child_of_user_data_dir() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("User Data");
        let nested = root.join("Default/Nested");
        fs::create_dir_all(&nested).unwrap();
        assert!(validate_profile_paths(
            "chrome",
            nested.to_str().unwrap(),
            Some(root.to_str().unwrap()),
            "manual"
        )
        .is_err());
    }

    #[test]
    fn chromium_profile_lock_prevents_another_launch_without_removing_marker() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("User Data");
        let profile = root.join("Default");
        fs::create_dir_all(&profile).unwrap();
        let lock = root.join("SingletonLock");
        fs::write(&lock, "simulated active-profile marker").unwrap();
        let exe = temp.path().join("chrome.exe");
        fs::write(&exe, b"test executable").unwrap();

        let result = make_launch_spec(
            "chrome",
            exe.to_str().unwrap(),
            profile.to_str().unwrap(),
            Some(root.to_str().unwrap()),
            "Default",
            "https://claude.ai/",
        );

        let error = result.unwrap_err();
        assert!(error.contains("appears to be in use"));
        assert!(
            lock.exists(),
            "LEVI must not remove the browser lock marker"
        );
    }

    async fn profile_registration_test_pool(
        path: &Path,
        browsers: &[(&str, &str, &str)],
    ) -> SqlitePool {
        let pool = SqlitePoolOptions::new()
            .max_connections(5)
            .connect_with(
                SqliteConnectOptions::new()
                    .filename(path)
                    .create_if_missing(true)
                    .foreign_keys(true)
                    .busy_timeout(Duration::from_secs(10)),
            )
            .await
            .unwrap();
        sqlx::query("CREATE TABLE browsers(id TEXT PRIMARY KEY,name TEXT NOT NULL,family TEXT NOT NULL,availability_status TEXT NOT NULL)")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("CREATE TABLE browser_profiles(id TEXT PRIMARY KEY,browser_id TEXT NOT NULL REFERENCES browsers(id),profile_name TEXT NOT NULL,profile_path TEXT NOT NULL,user_data_directory TEXT,detection_method TEXT NOT NULL,availability_status TEXT NOT NULL,created_at TEXT NOT NULL DEFAULT (datetime('now')),updated_at TEXT NOT NULL DEFAULT (datetime('now')),archived_at TEXT,UNIQUE(browser_id,profile_path))")
            .execute(&pool)
            .await
            .unwrap();
        for (id, name, family) in browsers {
            sqlx::query("INSERT INTO browsers(id,name,family,availability_status) VALUES(?,?,?,'available')")
                .bind(id)
                .bind(name)
                .bind(family)
                .execute(&pool)
                .await
                .unwrap();
        }
        pool
    }

    async fn register_test_profile(
        pool: &SqlitePool,
        browser_id: &str,
        profile_path: &Path,
        user_data_directory: Option<&Path>,
    ) -> Result<DetectedProfile, String> {
        register_browser_profile_transaction(
            pool,
            browser_id,
            "Temporary test profile",
            &profile_path.to_string_lossy(),
            user_data_directory.map(|path| path.to_str().unwrap()),
            "manual",
        )
        .await
    }

    fn create_chromium_test_profile(root: &Path, name: &str) -> PathBuf {
        let profile = root.join(name);
        fs::create_dir_all(&profile).unwrap();
        profile
    }

    #[tokio::test]
    async fn same_directory_under_different_active_browser_families_is_rejected() {
        let temp = tempfile::tempdir().unwrap();
        let pool = profile_registration_test_pool(
            &temp.path().join("profile-registration-test.db"),
            &[
                ("firefox", "Firefox", "firefox"),
                ("opera", "Opera", "opera"),
            ],
        )
        .await;
        let user_data = temp.path().join("firefox-user-data");
        fs::create_dir(&user_data).unwrap();
        let profile = create_chromium_test_profile(&user_data, "Test Profile");

        register_test_profile(&pool, "firefox", &profile, None)
            .await
            .unwrap();
        let error = register_test_profile(&pool, "opera", &profile, Some(&user_data))
            .await
            .unwrap_err();

        assert!(error.contains("already registered as the active profile"));
        assert!(error.contains("Firefox"));
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM browser_profiles")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(count, 1);
    }

    #[tokio::test]
    async fn same_directory_under_same_browser_preserves_upsert_behavior() {
        let temp = tempfile::tempdir().unwrap();
        let pool = profile_registration_test_pool(
            &temp.path().join("profile-registration-test.db"),
            &[("chrome", "Chrome", "chrome")],
        )
        .await;
        let user_data = temp.path().join("chrome-user-data");
        fs::create_dir(&user_data).unwrap();
        let profile = create_chromium_test_profile(&user_data, "Test Profile");
        let first = register_test_profile(&pool, "chrome", &profile, Some(&user_data))
            .await
            .unwrap();
        let equivalent = user_data.join("Nested").join("..").join("Test Profile");
        let nested = user_data.join("Nested");
        fs::create_dir(&nested).unwrap();
        let second = register_browser_profile_transaction(
            &pool,
            "chrome",
            "Updated test profile",
            &equivalent.to_string_lossy(),
            Some(user_data.to_str().unwrap()),
            "manual",
        )
        .await
        .unwrap();

        assert_eq!(first.profile_path, second.profile_path);
        let rows: Vec<(String, String)> = sqlx::query_as(
            "SELECT id,profile_name FROM browser_profiles WHERE browser_id='chrome'",
        )
        .fetch_all(&pool)
        .await
        .unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].1, "Updated test profile");
    }

    #[tokio::test]
    async fn different_directories_under_different_browsers_are_accepted() {
        let temp = tempfile::tempdir().unwrap();
        let pool = profile_registration_test_pool(
            &temp.path().join("profile-registration-test.db"),
            &[("chrome", "Chrome", "chrome"), ("edge", "Edge", "edge")],
        )
        .await;
        let chrome_root = temp.path().join("chrome-user-data");
        let edge_root = temp.path().join("edge-user-data");
        fs::create_dir(&chrome_root).unwrap();
        fs::create_dir(&edge_root).unwrap();
        let chrome_profile = create_chromium_test_profile(&chrome_root, "Test Profile");
        let edge_profile = create_chromium_test_profile(&edge_root, "Test Profile");

        register_test_profile(&pool, "chrome", &chrome_profile, Some(&chrome_root))
            .await
            .unwrap();
        register_test_profile(&pool, "edge", &edge_profile, Some(&edge_root))
            .await
            .unwrap();
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM browser_profiles")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(count, 2);
    }

    #[tokio::test]
    async fn archived_cross_family_profile_does_not_block_registration() {
        let temp = tempfile::tempdir().unwrap();
        let pool = profile_registration_test_pool(
            &temp.path().join("profile-registration-test.db"),
            &[
                ("firefox", "Firefox", "firefox"),
                ("opera", "Opera", "opera"),
            ],
        )
        .await;
        let root = temp.path().join("firefox-user-data");
        fs::create_dir(&root).unwrap();
        let profile = create_chromium_test_profile(&root, "Test Profile");
        sqlx::query("INSERT INTO browser_profiles(id,browser_id,profile_name,profile_path,user_data_directory,detection_method,availability_status,archived_at) VALUES('archived-firefox','firefox','Old Firefox profile',?,?, 'manual','available',datetime('now'))")
            .bind(profile.to_string_lossy().as_ref())
            .bind(root.to_string_lossy().as_ref())
            .execute(&pool)
            .await
            .unwrap();

        register_test_profile(&pool, "opera", &profile, Some(&root))
            .await
            .unwrap();

        let archived_at: Option<String> = sqlx::query_scalar(
            "SELECT archived_at FROM browser_profiles WHERE id='archived-firefox'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert!(archived_at.is_some());
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM browser_profiles")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(count, 2);
    }

    #[cfg(windows)]
    #[test]
    fn windows_path_keys_ignore_case_prefix_and_dot_segments() {
        let canonical =
            normalize_windows_path_text(r"\\?\C:\Users\Test\Temp\Root\.\Profile\..\Profile");
        let equivalent = normalize_windows_path_text(r"c:/users/test/temp/root/profile");
        assert_eq!(canonical, equivalent);
    }

    #[tokio::test]
    async fn nonexistent_profile_directory_keeps_existing_validation_error() {
        let temp = tempfile::tempdir().unwrap();
        let pool = profile_registration_test_pool(
            &temp.path().join("profile-registration-test.db"),
            &[("firefox", "Firefox", "firefox")],
        )
        .await;
        let missing = temp.path().join("missing-profile");

        let error = register_test_profile(&pool, "firefox", &missing, None)
            .await
            .unwrap_err();

        assert!(error.contains("existing directory"));
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM browser_profiles")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(count, 0);
    }

    #[tokio::test]
    async fn concurrent_cross_family_registrations_are_serialized() {
        let temp = tempfile::tempdir().unwrap();
        let pool = profile_registration_test_pool(
            &temp.path().join("profile-registration-test.db"),
            &[
                ("firefox", "Firefox", "firefox"),
                ("opera", "Opera", "opera"),
            ],
        )
        .await;
        let root = temp.path().join("test-user-data");
        fs::create_dir(&root).unwrap();
        let profile = create_chromium_test_profile(&root, "Test Profile");

        let (firefox, opera) = tokio::join!(
            register_test_profile(&pool, "firefox", &profile, None),
            register_test_profile(&pool, "opera", &profile, Some(&root)),
        );
        assert_ne!(firefox.is_ok(), opera.is_ok());
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM browser_profiles")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(count, 1);
    }

    fn account_input(name: &str, profile_id: &str) -> AccountInput {
        AccountInput {
            id: "test-account".into(),
            name: name.into(),
            identifier: Some("test-label".into()),
            browser_profile_id: profile_id.into(),
            conversation_url: Some("https://claude.ai/new".into()),
            status: "idle".into(),
            priority: 2,
            notes: "temporary test data".into(),
            project_id: Some("test-project".into()),
        }
    }

    async fn account_test_pool(path: &Path) -> SqlitePool {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(
                SqliteConnectOptions::new()
                    .filename(path)
                    .create_if_missing(true)
                    .foreign_keys(true),
            )
            .await
            .unwrap();
        for ddl in [
            "CREATE TABLE browsers(id TEXT PRIMARY KEY, availability_status TEXT NOT NULL)",
            "CREATE TABLE browser_profiles(id TEXT PRIMARY KEY, browser_id TEXT NOT NULL REFERENCES browsers(id), availability_status TEXT NOT NULL, archived_at TEXT)",
            "CREATE TABLE projects(id TEXT PRIMARY KEY, status TEXT NOT NULL, archived_at TEXT)",
            "CREATE TABLE accounts(id TEXT PRIMARY KEY, name TEXT NOT NULL, identifier TEXT, browser_profile_id TEXT NOT NULL REFERENCES browser_profiles(id), conversation_url TEXT, status TEXT NOT NULL, priority INTEGER NOT NULL, notes TEXT NOT NULL, updated_at TEXT NOT NULL DEFAULT '', archived_at TEXT)",
            "CREATE TABLE project_accounts(project_id TEXT NOT NULL REFERENCES projects(id), account_id TEXT NOT NULL REFERENCES accounts(id), PRIMARY KEY(project_id,account_id))",
            "CREATE TABLE activity_logs(id TEXT PRIMARY KEY, activity_type TEXT NOT NULL, description TEXT NOT NULL, source TEXT NOT NULL, account_id TEXT REFERENCES accounts(id))",
        ] {
            sqlx::query(ddl).execute(&pool).await.unwrap();
        }
        sqlx::query(
            "INSERT INTO browsers(id,availability_status) VALUES('test-browser','available')",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query("INSERT INTO browser_profiles(id,browser_id,availability_status) VALUES('profile-one','test-browser','available'),('profile-two','test-browser','available')")
            .execute(&pool).await.unwrap();
        sqlx::query("INSERT INTO projects(id,status) VALUES('test-project','active')")
            .execute(&pool)
            .await
            .unwrap();
        pool
    }

    #[tokio::test]
    async fn account_create_edit_persist_and_activity_are_atomic() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("account-transaction-test.db");
        let pool = account_test_pool(&path).await;

        save_account_transaction(
            &pool,
            &account_input("Temporary account", "profile-one"),
            false,
        )
        .await
        .unwrap();
        save_account_transaction(
            &pool,
            &account_input("Temporary account edited", "profile-two"),
            true,
        )
        .await
        .unwrap();

        let saved: (String, String) =
            sqlx::query_as("SELECT name,browser_profile_id FROM accounts WHERE id='test-account'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(
            saved,
            ("Temporary account edited".into(), "profile-two".into())
        );
        let activities: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM activity_logs WHERE account_id='test-account'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(activities, 2);

        sqlx::query("CREATE TRIGGER reject_activity BEFORE INSERT ON activity_logs BEGIN SELECT RAISE(ABORT,'forced activity failure'); END")
            .execute(&pool).await.unwrap();
        let failed_update = account_input("Should roll back", "profile-one");
        assert!(save_account_transaction(&pool, &failed_update, true)
            .await
            .is_err());
        let after_failure: (String, String) =
            sqlx::query_as("SELECT name,browser_profile_id FROM accounts WHERE id='test-account'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(after_failure, saved);
        let activities_after_failure: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM activity_logs WHERE account_id='test-account'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(activities_after_failure, 2);

        assert!(archive_account_transaction(&pool, "test-account")
            .await
            .is_err());
        let archived_at: Option<String> =
            sqlx::query_scalar("SELECT archived_at FROM accounts WHERE id='test-account'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert!(archived_at.is_none());
        pool.close().await;

        let reopened = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(
                SqliteConnectOptions::new()
                    .filename(&path)
                    .foreign_keys(true),
            )
            .await
            .unwrap();
        let persisted: (String, String) =
            sqlx::query_as("SELECT name,browser_profile_id FROM accounts WHERE id='test-account'")
                .fetch_one(&reopened)
                .await
                .unwrap();
        assert_eq!(persisted, saved);
        let mapping_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM project_accounts WHERE account_id='test-account' AND project_id='test-project'")
            .fetch_one(&reopened).await.unwrap();
        assert_eq!(mapping_count, 1);
        let final_activity_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM activity_logs WHERE account_id='test-account'",
        )
        .fetch_one(&reopened)
        .await
        .unwrap();
        assert_eq!(final_activity_count, 2);
    }
}
