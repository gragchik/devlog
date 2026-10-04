use rusqlite::Connection;
use std::sync::Mutex;
use tauri::{AppHandle, Manager};

use super::database::create_database;
use super::ids::now_utc_seconds;
use super::repositories::{jira_submissions, session_edits};

const SESSION_EDITS_RETENTION_DAYS: i64 = 30;
const SECONDS_PER_DAY: i64 = 86_400;

/// Единственный экземпляр БД приложения — управляется Tauri как managed
/// state (`app.manage(...)`), доступен командам через `tauri::State`.
/// Main window и overlay работают через один и тот же процесс и, значит,
/// через один и тот же `AppDatabase` — отдельного хранилища для overlay
/// нет и не будет (ТЗ: "Все изменения из overlay и основного окна
/// выполняются через один сервис"). `Mutex`, а не `RwLock` — rusqlite
/// `Connection` не `Sync`, нужен эксклюзивный доступ на каждый вызов.
pub struct AppDatabase(pub Mutex<Connection>);

/// Открывает (или создаёт) БД по реальному пути приложения
/// (`app.path().app_data_dir()`, аналог Electron `app.getPath('userData')`)
/// и чистит `session_edits` старше 30 дней (FR-04.8) — одноразово, при
/// каждом холодном старте, не дожидаясь отдельного maintenance-задания.
pub fn init(app: &AppHandle) -> anyhow::Result<AppDatabase> {
    let dir = app.path().app_data_dir()?;
    std::fs::create_dir_all(&dir)?;
    let db_path = dir.join("devlog.sqlite3");

    let conn = create_database(db_path)?;
    session_edits::prune_older_than(&conn, now_utc_seconds() - SESSION_EDITS_RETENTION_DAYS * SECONDS_PER_DAY)?;
    // FR-07.6: POST, прерванный падением процесса, — результат неизвестен.
    jira_submissions::mark_stale_pending_as_unknown(&conn)?;

    Ok(AppDatabase(Mutex::new(conn)))
}
