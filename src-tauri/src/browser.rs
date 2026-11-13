use serde::{Deserialize, Serialize};
use sqlx::{sqlite::SqliteConnectOptions, FromRow, SqlitePool};
use std::{
    collections::HashSet,
    env,
    path::{Path, PathBuf},
    process::Command,
    time::Duration,
};
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
}

#[tauri::command]
pub fn detect_browsers() -> Vec<DetectedBrowser> {
    BROWSER_FAMILIES
        .iter()
        .filter_map(|family| {
            find_executable(family).map(|(path, method)| DetectedBrowser {
                family: (*family).to_owned(),
                name: browser_name(family).to_owned(),
                browser_version: None,
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
    Ok(ValidatedPath {
        canonical_path: path.to_string_lossy().into_owned(),
        browser_version: None,
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
pub fn validate_profile(
    family: String,
    profile_path: String,
    user_data_directory: Option<String>,
) -> Result<DetectedProfile, String> {
    validate_profile_paths(
        &family,
        &profile_path,
        user_data_directory.as_deref(),
        "manual",
    )
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
pub async fn launch_account(app: AppHandle, account_id: String) -> Result<(), String> {
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
    let spec = make_launch_spec(
        &record.browser_family,
        &record.executable_path,
        &record.profile_path,
        record.user_data_directory.as_deref(),
        &record.profile_name,
        url,
    )?;
    spec.spawn()?;
    record_launch(&pool, &record).await?;
    Ok(())
}

#[tauri::command]
pub async fn test_profile_launch(app: AppHandle, profile_id: String) -> Result<(), String> {
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
    let spec = make_launch_spec(
        &row.0,
        &row.1,
        &row.5,
        row.3.as_deref(),
        &row.4,
        "about:blank",
    )?;
    spec.spawn()?;
    sqlx::query("UPDATE browser_profiles SET last_accessed_at=datetime('now'),availability_status='available',updated_at=datetime('now') WHERE id=?")
        .bind(profile_id).execute(&pool).await.map_err(db_error)?;
    Ok(())
}

impl LaunchSpec {
    fn spawn(&self) -> Result<(), String> {
        let mut command = Command::new(&self.executable);
        command.args(&self.args);
        command.spawn().map(|_| ()).map_err(|e| format!("Could not launch this browser profile: {e}. Check the executable path and close any browser window that reports the profile is already in use."))
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
    let _display_name = profile_name;
    let url = if url == "about:blank" {
        url.to_owned()
    } else {
        validate_claude_url(url)?
    };
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
            args.push(profile.profile_path);
            args.push("-new-window".into());
        }
        _ => return Err("This browser family is not supported for profile launching.".into()),
    }
    args.push(url);
    Ok(LaunchSpec {
        executable: PathBuf::from(executable.canonical_path),
        args,
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

async fn database(app: &AppHandle) -> Result<SqlitePool, String> {
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

async fn record_launch(pool: &SqlitePool, record: &LaunchRow) -> Result<(), String> {
    let mut tx = pool.begin().await.map_err(db_error)?;
    sqlx::query("UPDATE accounts SET last_accessed_at=datetime('now'),updated_at=datetime('now') WHERE id=?")
        .bind(&record.account_id).execute(&mut *tx).await.map_err(db_error)?;
    sqlx::query("UPDATE browser_profiles SET last_accessed_at=datetime('now'),availability_status='available',updated_at=datetime('now') WHERE id=?")
        .bind(&record.profile_id).execute(&mut *tx).await.map_err(db_error)?;
    sqlx::query("UPDATE browsers SET last_accessed_at=datetime('now'),availability_status='available',updated_at=datetime('now') WHERE executable_path=?")
        .bind(&record.executable_path).execute(&mut *tx).await.map_err(db_error)?;
    sqlx::query("INSERT INTO activity_logs(id,activity_type,description,source,account_id) VALUES(?, 'account_launched', ?, 'system', ?)")
        .bind(Uuid::new_v4().to_string())
        .bind(format!("Opened {} in {} profile {}", record.account_name, browser_name(&record.browser_family), record.profile_name))
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
}
