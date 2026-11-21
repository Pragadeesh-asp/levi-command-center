//! LEVI ecosystem bridge, Command Center side.
//!
//! - Publishes `%LOCALAPPDATA%\LEVI\ecosystem\command-center.json` so the other
//!   LEVI apps can find this executable.
//! - Reads LEVI Browser's `browser.json` to list its Spaces, and asks the Browser
//!   to open a page in a Space by starting it with `--space <id> --open <url>`.
//!   A running Browser receives the request through its single-instance plugin.
//!   Only Spaces listed in that manifest are accepted, and the executable must be
//!   an existing `levi…browser….exe`.
//! - Stores where LEVI OS lives (`levi_os_url`) and which Space opens it
//!   (`levi_os_space`) in the settings table.

use crate::db::database;
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::ErrorKind,
    path::{Path, PathBuf},
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};
use tauri::{AppHandle, Manager, Runtime};
use url::{Host, Url};
use uuid::Uuid;

pub const MANIFEST_SCHEMA: u32 = 1;
const BROWSER_MANIFEST: &str = "browser.json";
const OWN_MANIFEST: &str = "command-center.json";
const MAX_URL_LEN: usize = 2048;
const LEVI_OS_URL_KEY: &str = "levi_os_url";
const LEVI_OS_SPACE_KEY: &str = "levi_os_space";
const NOT_REGISTERED: &str =
    "LEVI Browser hasn't registered on this PC yet. Start LEVI Browser once, then try again.";

/// `%LOCALAPPDATA%\LEVI\ecosystem`, shared by every LEVI desktop app.
pub fn manifest_dir(local_data_dir: &Path) -> PathBuf {
    local_data_dir.join("LEVI").join("ecosystem")
}

fn ecosystem_dir<R: Runtime>(app: &AppHandle<R>) -> Result<PathBuf, String> {
    app.path()
        .local_data_dir()
        .map(|local| manifest_dir(&local))
        .map_err(|error| format!("Could not locate the local data folder: {error}"))
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Space {
    pub id: String,
    pub name: String,
    pub avatar: String,
    pub accent: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BrowserManifest {
    schema: u32,
    app: String,
    version: String,
    executable: String,
    updated_at_unix: u64,
    #[serde(default)]
    spaces: Vec<Space>,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct BrowserStatus {
    pub registered: bool,
    pub launchable: bool,
    pub version: Option<String>,
    pub updated_at_unix: Option<u64>,
    pub spaces: Vec<Space>,
    pub problem: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EcosystemStatus {
    pub browser: BrowserStatus,
    pub levi_os_url: Option<String>,
    pub levi_os_space: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenInSpaceResult {
    pub process_id: u32,
    pub space_name: String,
    pub host: String,
}

fn valid_space_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 80
        && id
            .chars()
            .all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '-')
}

fn valid_accent(accent: &str) -> bool {
    accent.len() == 7
        && accent.starts_with('#')
        && accent[1..].chars().all(|ch| ch.is_ascii_hexdigit())
}

fn valid_space(space: &Space) -> bool {
    valid_space_id(&space.id)
        && !space.name.trim().is_empty()
        && space.name.chars().count() <= 80
        && !space.avatar.trim().is_empty()
        && space.avatar.chars().count() <= 3
        && valid_accent(&space.accent)
}

/// A page another app may open in a Space: `http`/`https`, a host, no credentials.
pub fn validate_page_url(raw: &str) -> Result<Url, String> {
    let raw = raw.trim();
    if raw.is_empty() || raw.len() > MAX_URL_LEN {
        return Err("The page address is empty or too long.".into());
    }
    let url = Url::parse(raw).map_err(|_| "The page address is malformed.".to_owned())?;
    if !matches!(url.scheme(), "http" | "https") || url.host_str().is_none() {
        return Err("Only http and https page addresses can be opened.".into());
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err("Page addresses cannot contain a user name or password.".into());
    }
    Ok(url)
}

/// Where LEVI OS runs: HTTPS, or plain HTTP only on this PC for local development.
pub fn validate_levi_os_url(raw: &str) -> Result<Url, String> {
    let url = validate_page_url(raw)?;
    let loopback = match url.host() {
        Some(Host::Domain(domain)) => domain.eq_ignore_ascii_case("localhost"),
        Some(Host::Ipv4(ip)) => ip.is_loopback(),
        Some(Host::Ipv6(ip)) => ip.is_loopback(),
        None => false,
    };
    if url.scheme() == "http" && !loopback {
        return Err(
            "Use an https:// address for LEVI OS. Plain http is allowed only for localhost.".into(),
        );
    }
    Ok(url)
}

/// `Ok(None)` when the Browser has never registered. Invalid Spaces are dropped.
fn load_browser_manifest(dir: &Path) -> Result<Option<BrowserManifest>, String> {
    let bytes = match fs::read(dir.join(BROWSER_MANIFEST)) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(None),
        Err(error) => {
            return Err(format!(
                "Could not read LEVI Browser's registration: {error}"
            ))
        }
    };
    let mut manifest: BrowserManifest = serde_json::from_slice(&bytes).map_err(|_| {
        "LEVI Browser's registration file is not valid. Start LEVI Browser again to rewrite it."
            .to_owned()
    })?;
    if manifest.schema != MANIFEST_SCHEMA || manifest.app != "browser" {
        return Err("LEVI Browser's registration uses a format this Command Center doesn't understand. Update both apps.".into());
    }
    manifest.spaces.retain(valid_space);
    Ok(Some(manifest))
}

/// The Browser executable named in its manifest, if it is safe to start.
fn trusted_browser_executable(raw: &str) -> Result<PathBuf, String> {
    let path = PathBuf::from(raw);
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    let looks_right = name.contains("levi")
        && name.contains("browser")
        && (!cfg!(windows) || name.ends_with(".exe"));
    if !path.is_absolute() || !looks_right {
        return Err("LEVI Browser's registration doesn't point to a LEVI Browser program.".into());
    }
    if !path.is_file() {
        return Err(
            "LEVI Browser was moved or removed. Start it once from its new location.".into(),
        );
    }
    Ok(path)
}

fn browser_status(dir: &Path) -> BrowserStatus {
    match load_browser_manifest(dir) {
        Ok(None) => BrowserStatus {
            registered: false,
            launchable: false,
            version: None,
            updated_at_unix: None,
            spaces: Vec::new(),
            problem: Some(NOT_REGISTERED.into()),
        },
        Err(problem) => BrowserStatus {
            registered: false,
            launchable: false,
            version: None,
            updated_at_unix: None,
            spaces: Vec::new(),
            problem: Some(problem),
        },
        Ok(Some(manifest)) => {
            let executable = trusted_browser_executable(&manifest.executable);
            BrowserStatus {
                registered: true,
                launchable: executable.is_ok(),
                version: Some(manifest.version),
                updated_at_unix: Some(manifest.updated_at_unix),
                spaces: manifest.spaces,
                problem: executable.err(),
            }
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct OwnManifest<'a> {
    schema: u32,
    app: &'static str,
    name: &'static str,
    version: &'a str,
    executable: String,
    updated_at_unix: u64,
}

fn write_own_manifest(dir: &Path, version: &str, executable: &Path) -> Result<PathBuf, String> {
    fs::create_dir_all(dir)
        .map_err(|error| format!("Could not create {}: {error}", dir.display()))?;
    let manifest = OwnManifest {
        schema: MANIFEST_SCHEMA,
        app: "command-center",
        name: "LEVI Command Center",
        version,
        executable: executable.display().to_string(),
        updated_at_unix: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|elapsed| elapsed.as_secs())
            .unwrap_or_default(),
    };
    let json = serde_json::to_vec_pretty(&manifest)
        .map_err(|error| format!("Could not serialize the ecosystem manifest: {error}"))?;
    let target = dir.join(OWN_MANIFEST);
    let temporary = dir.join(format!("{OWN_MANIFEST}.tmp"));
    fs::write(&temporary, json)
        .map_err(|error| format!("Could not write {}: {error}", temporary.display()))?;
    fs::rename(&temporary, &target)
        .map_err(|error| format!("Could not replace {}: {error}", target.display()))?;
    Ok(target)
}

/// Writes `command-center.json` at startup. Failures are logged, never fatal.
pub fn publish_manifest<R: Runtime>(app: &AppHandle<R>) {
    let result = ecosystem_dir(app).and_then(|dir| {
        let executable = std::env::current_exe()
            .map_err(|error| format!("Could not resolve this executable: {error}"))?;
        write_own_manifest(&dir, &app.package_info().version.to_string(), &executable)
    });
    if let Err(error) = result {
        log::warn!("Could not publish the LEVI ecosystem manifest: {error}");
    }
}

async fn read_levi_os_settings(
    app: &AppHandle,
) -> Result<(Option<String>, Option<String>), String> {
    let pool = database(app).await?;
    let rows: Vec<(String, String)> =
        sqlx::query_as("SELECT key,value FROM settings WHERE key IN (?,?)")
            .bind(LEVI_OS_URL_KEY)
            .bind(LEVI_OS_SPACE_KEY)
            .fetch_all(&pool)
            .await
            .map_err(db_error)?;
    let value = |key: &str| rows.iter().find(|(k, _)| k == key).map(|(_, v)| v.clone());
    Ok((value(LEVI_OS_URL_KEY), value(LEVI_OS_SPACE_KEY)))
}

#[tauri::command]
pub async fn get_ecosystem(app: AppHandle) -> Result<EcosystemStatus, String> {
    let browser = browser_status(&ecosystem_dir(&app)?);
    let (levi_os_url, levi_os_space) = read_levi_os_settings(&app).await?;
    Ok(EcosystemStatus {
        browser,
        levi_os_url,
        levi_os_space,
    })
}

/// Saves (or clears, when empty) the LEVI OS address and the Space that opens it.
#[tauri::command]
pub async fn save_levi_os(
    app: AppHandle,
    url: Option<String>,
    space_id: Option<String>,
) -> Result<EcosystemStatus, String> {
    let url = match url
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        Some(raw) => Some(validate_levi_os_url(raw)?.to_string()),
        None => None,
    };
    let space = match space_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        Some(id) if valid_space_id(id) => Some(id.to_owned()),
        Some(_) => return Err("That Space identifier is invalid.".into()),
        None => None,
    };
    let pool = database(&app).await?;
    let mut tx = pool.begin_with("BEGIN IMMEDIATE").await.map_err(db_error)?;
    for (key, value) in [(LEVI_OS_URL_KEY, url), (LEVI_OS_SPACE_KEY, space)] {
        match value {
            Some(value) => sqlx::query("INSERT INTO settings(key,value) VALUES(?,?) ON CONFLICT(key) DO UPDATE SET value=excluded.value,updated_at=datetime('now')")
                .bind(key)
                .bind(value)
                .execute(&mut *tx)
                .await
                .map_err(db_error)?,
            None => sqlx::query("DELETE FROM settings WHERE key=?")
                .bind(key)
                .execute(&mut *tx)
                .await
                .map_err(db_error)?,
        };
    }
    tx.commit().await.map_err(db_error)?;
    get_ecosystem(app).await
}

/// Asks LEVI Browser to open `url` in `space_id`. The opening itself is not
/// confirmed: the Browser may still refuse an invalid request.
#[tauri::command]
pub async fn open_in_levi_browser(
    app: AppHandle,
    space_id: String,
    url: String,
    account_id: Option<String>,
) -> Result<OpenInSpaceResult, String> {
    let url = validate_page_url(&url)?;
    if !valid_space_id(&space_id) {
        return Err("That Space identifier is invalid.".into());
    }
    let manifest =
        load_browser_manifest(&ecosystem_dir(&app)?)?.ok_or_else(|| NOT_REGISTERED.to_owned())?;
    let space = manifest
        .spaces
        .iter()
        .find(|space| space.id == space_id)
        .cloned()
        .ok_or_else(|| {
            "That Space is no longer in LEVI Browser. Choose another Space.".to_owned()
        })?;
    let executable = trusted_browser_executable(&manifest.executable)?;
    let child = Command::new(&executable)
        .args(["--space", space.id.as_str(), "--open", url.as_str()])
        .spawn()
        .map_err(|error| format!("Could not start LEVI Browser: {error}"))?;
    let host = url.host_str().unwrap_or_default().to_owned();
    record_open(&app, &space, &host, account_id.as_deref()).await;
    Ok(OpenInSpaceResult {
        process_id: child.id(),
        space_name: space.name,
        host,
    })
}

/// Starts LEVI Browser without a request; a running Browser brings its
/// profile manager forward.
#[tauri::command]
pub async fn open_levi_browser(app: AppHandle) -> Result<u32, String> {
    let manifest =
        load_browser_manifest(&ecosystem_dir(&app)?)?.ok_or_else(|| NOT_REGISTERED.to_owned())?;
    let executable = trusted_browser_executable(&manifest.executable)?;
    Command::new(&executable)
        .spawn()
        .map(|child| child.id())
        .map_err(|error| format!("Could not start LEVI Browser: {error}"))
}

/// Best-effort activity record. Logs the host only, never the full address.
async fn record_open(app: &AppHandle, space: &Space, host: &str, account_id: Option<&str>) {
    let result: Result<(), String> = async {
        let pool = database(app).await?;
        let account = match account_id {
            Some(id) => sqlx::query_scalar::<_, String>("SELECT id FROM accounts WHERE id=?")
                .bind(id)
                .fetch_optional(&pool)
                .await
                .map_err(db_error)?,
            None => None,
        };
        sqlx::query("INSERT INTO activity_logs(id,activity_type,description,source,account_id) VALUES(?,'levi_browser_open_requested',?,'system',?)")
            .bind(Uuid::new_v4().to_string())
            .bind(format!(
                "Asked LEVI Browser to open {host} in Space \"{}\". The page opening is not confirmed.",
                space.name
            ))
            .bind(account)
            .execute(&pool)
            .await
            .map_err(db_error)?;
        Ok(())
    }
    .await;
    if let Err(error) = result {
        log::warn!("Could not record the LEVI Browser request: {error}");
    }
}

fn db_error(error: sqlx::Error) -> String {
    format!("Local database error: {error}")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "levi-cc-ecosystem-{name}-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn page_urls_must_be_plain_web_addresses() {
        assert!(validate_page_url("https://claude.ai/chat/abc").is_ok());
        assert!(validate_page_url("http://localhost:3000/dashboard").is_ok());
        for bad in [
            "",
            "javascript:alert(1)",
            "file:///C:/Windows/win.ini",
            "levi-browser://open",
            "https://user:pass@example.com",
        ] {
            assert!(validate_page_url(bad).is_err(), "{bad} must be rejected");
        }
        assert!(validate_page_url(&format!("https://a.com/{}", "x".repeat(MAX_URL_LEN))).is_err());
    }

    #[test]
    fn levi_os_needs_https_except_on_this_pc() {
        assert!(validate_levi_os_url("https://levi-os.example.app").is_ok());
        assert!(validate_levi_os_url("http://localhost:3000").is_ok());
        assert!(validate_levi_os_url("http://127.0.0.1:3000").is_ok());
        assert!(validate_levi_os_url("http://levi-os.example.app").is_err());
    }

    #[test]
    fn browser_manifest_is_optional_and_filters_bad_spaces() {
        let dir = temp_dir("manifest");
        assert!(load_browser_manifest(&dir).unwrap().is_none());
        assert!(!browser_status(&dir).registered);

        fs::write(
            dir.join(BROWSER_MANIFEST),
            r##"{"schema":1,"app":"browser","version":"0.1.0","executable":"C:\\nowhere\\levi-browser.exe","updatedAtUnix":1,
                 "spaces":[{"id":"alpha","name":"Claude · Nova","avatar":"N","accent":"#E3C36B"},
                           {"id":"../evil","name":"x","avatar":"x","accent":"#000000"},
                           {"id":"beta","name":"Beta","avatar":"B","accent":"red; position:fixed"}]}"##,
        )
        .unwrap();
        let manifest = load_browser_manifest(&dir).unwrap().unwrap();
        assert_eq!(manifest.spaces.len(), 1);
        assert_eq!(manifest.spaces[0].id, "alpha");
        let status = browser_status(&dir);
        assert!(status.registered && !status.launchable);
        assert!(status.problem.unwrap().contains("moved or removed"));

        fs::write(
            dir.join(BROWSER_MANIFEST),
            r#"{"schema":2,"app":"browser","version":"9","executable":"x","updatedAtUnix":1}"#,
        )
        .unwrap();
        assert!(load_browser_manifest(&dir).is_err());
        fs::write(dir.join(BROWSER_MANIFEST), "not json").unwrap();
        assert!(load_browser_manifest(&dir).is_err());
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn only_an_existing_levi_browser_program_is_started() {
        let dir = temp_dir("exe");
        let fake = dir.join("levi-browser-foundation.exe");
        fs::write(&fake, b"not really a program").unwrap();
        assert_eq!(
            trusted_browser_executable(fake.to_str().unwrap()).unwrap(),
            fake
        );
        let other = dir.join("calc.exe");
        fs::write(&other, b"").unwrap();
        assert!(trusted_browser_executable(other.to_str().unwrap()).is_err());
        assert!(trusted_browser_executable("levi-browser.exe").is_err());
        assert!(
            trusted_browser_executable(dir.join("levi-browser-missing.exe").to_str().unwrap())
                .is_err()
        );
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn own_manifest_names_this_app() {
        let dir = temp_dir("own");
        let path = write_own_manifest(
            &dir,
            "0.1.0",
            Path::new("C:\\LEVI\\levi-command-center.exe"),
        )
        .unwrap();
        let value: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        assert_eq!(value["app"], "command-center");
        assert_eq!(value["schema"], 1);
        assert_eq!(value["executable"], "C:\\LEVI\\levi-command-center.exe");
        assert!(!dir.join(format!("{OWN_MANIFEST}.tmp")).exists());
        fs::remove_dir_all(&dir).unwrap();
    }
}
