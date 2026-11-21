//! Shared SQLite access for the Tauri host.
//!
//! tauri-plugin-sql creates and migrates `command-center.db` while the app
//! starts (tauri.conf.json preloads it). Rust commands then share one pool
//! instead of opening a new pool on every command.
//!
//! The `levi-db-guard` plugin is registered before the SQL plugin, so it runs
//! first. It snapshots the database with `VACUUM INTO` before any pending
//! migration (startup stops if that snapshot fails, leaving the database
//! untouched), keeps one daily snapshot (14 retained) and switches the file to
//! WAL so the plugin's pool and the Rust pool stop blocking each other.

use sqlx::{
    sqlite::{SqliteConnectOptions, SqlitePoolOptions},
    ConnectOptions, Connection, SqliteConnection, SqlitePool,
};
use std::{
    fs,
    path::{Path, PathBuf},
    time::{Duration, SystemTime},
};
use tauri::{
    plugin::{Builder as PluginBuilder, TauriPlugin},
    AppHandle, Manager, Runtime,
};
use tokio::sync::OnceCell;

pub const DATABASE_FILE: &str = "command-center.db";
const BACKUP_DIR: &str = "backups";
const DAILY_PREFIX: &str = "command-center-daily-";
const PRE_MIGRATION_PREFIX: &str = "command-center-pre-migration-";
const DAILY_BACKUPS_KEPT: usize = 14;
const DAILY_INTERVAL: Duration = Duration::from_secs(24 * 60 * 60);
const BUSY_TIMEOUT: Duration = Duration::from_secs(5);
const WAL_ATTEMPTS: u32 = 5;

/// One pool for every Rust command, created on first use.
#[derive(Default)]
pub struct DbState(OnceCell<SqlitePool>);

/// tauri-plugin-sql resolves `sqlite:` paths against `app_config_dir`, so the
/// Rust side uses the same directory (on Windows it equals `app_data_dir`).
pub fn database_dir<R: Runtime>(app: &AppHandle<R>) -> Result<PathBuf, String> {
    app.path()
        .app_config_dir()
        .map_err(|error| format!("Could not locate the LEVI data directory: {error}"))
}

pub(crate) async fn database(app: &AppHandle) -> Result<SqlitePool, String> {
    app.state::<DbState>()
        .0
        .get_or_try_init(|| async {
            let path = database_dir(app)?.join(DATABASE_FILE);
            open_pool(&path).await
        })
        .await
        .cloned()
}

fn connect_options(path: &Path) -> SqliteConnectOptions {
    SqliteConnectOptions::new()
        .filename(path)
        .create_if_missing(false)
        .foreign_keys(true)
        .busy_timeout(BUSY_TIMEOUT)
}

async fn open_pool(path: &Path) -> Result<SqlitePool, String> {
    let pool = SqlitePoolOptions::new()
        .connect_with(connect_options(path))
        .await
        .map_err(db_error)?;
    // Existing databases are switched by the startup guard. A database the SQL
    // plugin created during this launch is switched here; the plugin's pool
    // may be reading at that moment, so retry briefly before giving up.
    let mut last_error = String::new();
    for attempt in 1..=WAL_ATTEMPTS {
        let result = match pool.acquire().await {
            Ok(mut connection) => enable_wal(&mut connection).await,
            Err(error) => Err(db_error(error)),
        };
        match result {
            Ok(mode) => {
                if mode != "wal" {
                    log::warn!("SQLite journal mode is {mode}; WAL was requested.");
                }
                return Ok(pool);
            }
            Err(error) => last_error = error,
        }
        if attempt < WAL_ATTEMPTS {
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }
    log::warn!("Could not switch SQLite to WAL yet ({last_error}); retrying on next launch.");
    Ok(pool)
}

async fn enable_wal(connection: &mut SqliteConnection) -> Result<String, String> {
    sqlx::query_scalar("PRAGMA journal_mode=WAL")
        .fetch_one(&mut *connection)
        .await
        .map_err(db_error)
}

/// Registers the startup guard. `latest_migration` is the highest migration
/// version the SQL plugin is about to apply.
pub fn guard_plugin<R: Runtime>(latest_migration: i64) -> TauriPlugin<R> {
    PluginBuilder::new("levi-db-guard")
        .setup(move |app, _api| {
            let dir = database_dir(app)?;
            let outcome = tauri::async_runtime::block_on(prepare_database(
                &dir.join(DATABASE_FILE),
                &dir.join(BACKUP_DIR),
                latest_migration,
                SystemTime::now(),
            ));
            match outcome {
                Ok(report) => {
                    report.log();
                    Ok(())
                }
                Err(error) => {
                    log::error!("{error}");
                    Err(error.into())
                }
            }
        })
        .build()
}

#[derive(Debug, Default)]
pub struct PrepareReport {
    pub applied_version: Option<i64>,
    pub pre_migration_backup: Option<PathBuf>,
    pub daily_backup: Option<PathBuf>,
    pub daily_backup_error: Option<String>,
    pub journal_mode: Option<String>,
}

impl PrepareReport {
    fn log(&self) {
        log::debug!(
            "Migration ledger before startup: {:?}",
            self.applied_version
        );
        if let Some(path) = &self.pre_migration_backup {
            log::info!("Pre-migration backup saved: {}", path.display());
        }
        if let Some(path) = &self.daily_backup {
            log::info!("Daily backup saved: {}", path.display());
        }
        if let Some(error) = &self.daily_backup_error {
            log::warn!("Daily backup failed; LEVI will retry next launch: {error}");
        }
        match self.journal_mode.as_deref() {
            Some("wal") | None => {}
            Some(mode) => log::warn!("SQLite journal mode is {mode}; WAL was requested."),
        }
    }
}

/// Runs before the SQL plugin opens or migrates the database.
pub async fn prepare_database(
    database: &Path,
    backups: &Path,
    latest_migration: i64,
    now: SystemTime,
) -> Result<PrepareReport, String> {
    let mut report = PrepareReport::default();
    if !database.exists() {
        // First launch: the SQL plugin creates and migrates a new file.
        return Ok(report);
    }
    let mut connection = connect_options(database).connect().await.map_err(|error| {
        format!("LEVI could not open its database to check for upgrades: {error}")
    })?;
    let applied = applied_migration_version(&mut connection).await?;
    report.applied_version = applied;
    let from = applied.unwrap_or(0);
    if from < latest_migration {
        let prefix = format!("{PRE_MIGRATION_PREFIX}v{from:03}-to-v{latest_migration:03}-");
        let path = snapshot(&mut connection, backups, &prefix).await.map_err(|error| {
            format!(
                "LEVI did not upgrade its database because the safety backup failed ({error}). \
                 The database was not changed. Free disk space or fix folder permissions for {}, then start LEVI again.",
                backups.display()
            )
        })?;
        report.pre_migration_backup = Some(path);
    } else if daily_backup_due(backups, now) {
        match snapshot(&mut connection, backups, DAILY_PREFIX).await {
            Ok(path) => report.daily_backup = Some(path),
            Err(error) => report.daily_backup_error = Some(error),
        }
        if let Err(error) = prune_daily_backups(backups, DAILY_BACKUPS_KEPT) {
            log::warn!("Could not remove old daily backups: {error}");
        }
    }
    report.journal_mode = match enable_wal(&mut connection).await {
        Ok(mode) => Some(mode),
        Err(error) => {
            log::warn!("Could not switch SQLite to WAL at startup: {error}");
            None
        }
    };
    let _ = connection.close().await;
    Ok(report)
}

async fn applied_migration_version(
    connection: &mut SqliteConnection,
) -> Result<Option<i64>, String> {
    let ledger: Option<String> = sqlx::query_scalar(
        "SELECT name FROM sqlite_master WHERE type='table' AND name='_sqlx_migrations'",
    )
    .fetch_optional(&mut *connection)
    .await
    .map_err(db_error)?;
    if ledger.is_none() {
        return Ok(None);
    }
    sqlx::query_scalar("SELECT MAX(version) FROM _sqlx_migrations WHERE success=1")
        .fetch_one(&mut *connection)
        .await
        .map_err(db_error)
}

/// Writes a consistent copy with `VACUUM INTO`, then reopens it, switches it
/// to a single-file rollback journal and runs `integrity_check`.
async fn snapshot(
    connection: &mut SqliteConnection,
    dir: &Path,
    prefix: &str,
) -> Result<PathBuf, String> {
    fs::create_dir_all(dir)
        .map_err(|error| format!("could not create {}: {error}", dir.display()))?;
    let stamp: String = sqlx::query_scalar("SELECT strftime('%Y%m%d-%H%M%S','now','localtime')")
        .fetch_one(&mut *connection)
        .await
        .map_err(db_error)?;
    let target = unused_path(dir, &format!("{prefix}{stamp}"));
    let target_text = target
        .to_str()
        .ok_or_else(|| "the backup path is not valid Unicode".to_owned())?;
    sqlx::query("VACUUM INTO ?")
        .bind(target_text)
        .execute(&mut *connection)
        .await
        .map_err(db_error)?;
    if let Err(error) = verify_backup(&target).await {
        let _ = fs::remove_file(&target);
        return Err(error);
    }
    Ok(target)
}

async fn verify_backup(path: &Path) -> Result<(), String> {
    let mut backup = SqliteConnectOptions::new()
        .filename(path)
        .create_if_missing(false)
        .connect()
        .await
        .map_err(db_error)?;
    let _: String = sqlx::query_scalar("PRAGMA journal_mode=DELETE")
        .fetch_one(&mut backup)
        .await
        .map_err(db_error)?;
    let integrity: String = sqlx::query_scalar("PRAGMA integrity_check")
        .fetch_one(&mut backup)
        .await
        .map_err(db_error)?;
    let _ = backup.close().await;
    if integrity == "ok" {
        Ok(())
    } else {
        Err(format!(
            "the backup failed its integrity check: {integrity}"
        ))
    }
}

fn unused_path(dir: &Path, stem: &str) -> PathBuf {
    let mut candidate = dir.join(format!("{stem}.db"));
    let mut suffix = 2;
    while candidate.exists() {
        candidate = dir.join(format!("{stem}-{suffix}.db"));
        suffix += 1;
    }
    candidate
}

fn daily_backups(dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut files: Vec<PathBuf> = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.is_file()
                && path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| name.starts_with(DAILY_PREFIX) && name.ends_with(".db"))
        })
        .collect();
    // Names embed a sortable local timestamp, so name order is age order.
    files.sort();
    files
}

fn daily_backup_due(dir: &Path, now: SystemTime) -> bool {
    let newest = daily_backups(dir)
        .iter()
        .filter_map(|path| fs::metadata(path).and_then(|meta| meta.modified()).ok())
        .max();
    match newest {
        None => true,
        Some(modified) => now
            .duration_since(modified)
            .map(|age| age >= DAILY_INTERVAL)
            .unwrap_or(false),
    }
}

fn prune_daily_backups(dir: &Path, keep: usize) -> Result<(), String> {
    let files = daily_backups(dir);
    let excess = files.len().saturating_sub(keep);
    for path in files.into_iter().take(excess) {
        fs::remove_file(&path).map_err(|error| format!("{}: {error}", path.display()))?;
    }
    Ok(())
}

fn db_error(error: impl std::fmt::Display) -> String {
    format!("Local database error: {error}")
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn create_database(path: &Path, applied_versions: &[i64]) {
        let mut connection = SqliteConnectOptions::new()
            .filename(path)
            .create_if_missing(true)
            .connect()
            .await
            .unwrap();
        sqlx::query("CREATE TABLE _sqlx_migrations (version BIGINT PRIMARY KEY, description TEXT NOT NULL, installed_on TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP, success BOOLEAN NOT NULL, checksum BLOB NOT NULL, execution_time BIGINT NOT NULL)")
            .execute(&mut connection)
            .await
            .unwrap();
        for version in applied_versions {
            sqlx::query("INSERT INTO _sqlx_migrations(version,description,success,checksum,execution_time) VALUES(?,'test',1,x'00',0)")
                .bind(version)
                .execute(&mut connection)
                .await
                .unwrap();
        }
        sqlx::query("CREATE TABLE projects(id TEXT PRIMARY KEY, name TEXT NOT NULL)")
            .execute(&mut connection)
            .await
            .unwrap();
        sqlx::query("INSERT INTO projects VALUES('p1','LEVI Browser')")
            .execute(&mut connection)
            .await
            .unwrap();
        connection.close().await.unwrap();
    }

    async fn journal_mode(path: &Path) -> String {
        let mut connection = connect_options(path).connect().await.unwrap();
        let mode: String = sqlx::query_scalar("PRAGMA journal_mode")
            .fetch_one(&mut connection)
            .await
            .unwrap();
        connection.close().await.unwrap();
        mode
    }

    async fn project_names(path: &Path) -> Vec<String> {
        let mut connection = connect_options(path).connect().await.unwrap();
        let names = sqlx::query_scalar("SELECT name FROM projects ORDER BY id")
            .fetch_all(&mut connection)
            .await
            .unwrap();
        connection.close().await.unwrap();
        names
    }

    #[test]
    fn guard_and_sql_plugin_point_at_the_same_database() {
        let config: serde_json::Value =
            serde_json::from_str(include_str!("../tauri.conf.json")).unwrap();
        assert_eq!(
            config["plugins"]["sql"]["preload"][0],
            format!("sqlite:{DATABASE_FILE}")
        );
    }

    #[tokio::test]
    async fn first_launch_without_a_database_is_left_to_the_sql_plugin() {
        let dir = tempfile::tempdir().unwrap();
        let report = prepare_database(
            &dir.path().join(DATABASE_FILE),
            &dir.path().join(BACKUP_DIR),
            9,
            SystemTime::now(),
        )
        .await
        .unwrap();
        assert!(report.pre_migration_backup.is_none() && report.daily_backup.is_none());
        assert!(!dir.path().join(DATABASE_FILE).exists());
        assert!(!dir.path().join(BACKUP_DIR).exists());
    }

    #[tokio::test]
    async fn pending_migration_takes_a_verified_backup_first() {
        let dir = tempfile::tempdir().unwrap();
        let database = dir.path().join(DATABASE_FILE);
        create_database(&database, &[1, 2, 3, 8]).await;
        let report = prepare_database(
            &database,
            &dir.path().join(BACKUP_DIR),
            9,
            SystemTime::now(),
        )
        .await
        .unwrap();
        assert_eq!(report.applied_version, Some(8));
        let backup = report.pre_migration_backup.expect("pre-migration backup");
        let name = backup.file_name().unwrap().to_str().unwrap();
        assert!(
            name.starts_with("command-center-pre-migration-v008-to-v009-"),
            "{name}"
        );
        assert_eq!(project_names(&backup).await, ["LEVI Browser"]);
        assert_eq!(journal_mode(&backup).await, "delete");
        assert_eq!(report.journal_mode.as_deref(), Some("wal"));
        assert_eq!(journal_mode(&database).await, "wal");
        assert_eq!(project_names(&database).await, ["LEVI Browser"]);
    }

    #[tokio::test]
    async fn failed_pre_migration_backup_stops_startup_without_touching_the_database() {
        let dir = tempfile::tempdir().unwrap();
        let database = dir.path().join(DATABASE_FILE);
        create_database(&database, &[1, 2]).await;
        let blocked = dir.path().join(BACKUP_DIR);
        fs::write(&blocked, b"a file where the backup folder should be").unwrap();
        let error = prepare_database(&database, &blocked, 9, SystemTime::now())
            .await
            .unwrap_err();
        assert!(error.contains("did not upgrade"), "{error}");
        assert_eq!(journal_mode(&database).await, "delete");
    }

    #[tokio::test]
    async fn daily_backup_runs_once_per_day_and_keeps_fourteen() {
        let dir = tempfile::tempdir().unwrap();
        let database = dir.path().join(DATABASE_FILE);
        let backups = dir.path().join(BACKUP_DIR);
        create_database(&database, &[1, 9]).await;
        fs::create_dir_all(&backups).unwrap();
        // Old snapshots are dated 2020 so they sort before one taken today.
        for day in 1..=15 {
            fs::write(
                backups.join(format!("{DAILY_PREFIX}202001{day:02}-090000.db")),
                b"old",
            )
            .unwrap();
        }
        fs::write(
            backups.join("command-center-pre-migration-v008-to-v009-20200101-090000.db"),
            b"keep",
        )
        .unwrap();
        let later = SystemTime::now() + DAILY_INTERVAL + Duration::from_secs(60);

        let first = prepare_database(&database, &backups, 9, later)
            .await
            .unwrap();
        let created = first.daily_backup.expect("daily backup");
        assert!(first.pre_migration_backup.is_none());
        assert_eq!(project_names(&created).await, ["LEVI Browser"]);
        let kept = daily_backups(&backups);
        assert_eq!(kept.len(), DAILY_BACKUPS_KEPT);
        assert!(kept.contains(&created));
        assert!(!backups
            .join(format!("{DAILY_PREFIX}20200101-090000.db"))
            .exists());
        assert!(!backups
            .join(format!("{DAILY_PREFIX}20200102-090000.db"))
            .exists());
        assert!(backups
            .join(format!("{DAILY_PREFIX}20200103-090000.db"))
            .exists());
        assert!(backups
            .join("command-center-pre-migration-v008-to-v009-20200101-090000.db")
            .exists());

        let second = prepare_database(&database, &backups, 9, SystemTime::now())
            .await
            .unwrap();
        assert!(
            second.daily_backup.is_none(),
            "a backup newer than 24 hours already exists"
        );
    }

    #[tokio::test]
    async fn shared_pool_enforces_foreign_keys_and_uses_wal() {
        let dir = tempfile::tempdir().unwrap();
        let database = dir.path().join(DATABASE_FILE);
        create_database(&database, &[1]).await;
        let pool = open_pool(&database).await.unwrap();
        let foreign_keys: i64 = sqlx::query_scalar("PRAGMA foreign_keys")
            .fetch_one(&pool)
            .await
            .unwrap();
        let mode: String = sqlx::query_scalar("PRAGMA journal_mode")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!((foreign_keys, mode.as_str()), (1, "wal"));
        pool.close().await;
    }
}
