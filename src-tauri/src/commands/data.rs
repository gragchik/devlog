//! IPC Итерации 9: диагностика, экспорт, резервная копия, удаление данных.
//! Только главное окно. Пути к файлам выбирает пользователь в системном
//! диалоге, который открывает Rust — frontend не может передать
//! произвольный путь для записи.

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, State, WebviewWindow};
use tauri_plugin_dialog::DialogExt;

use super::jira::ensure_main_window;
use crate::db::app_database::{AppDatabase, StartupReport};
use crate::db::maintenance::{self, ConsistencyIssue, DeleteScope, DeleteSummary};
use crate::integrations::jira::credentials;
use crate::logging;
use crate::tracking::session_engine;

const LOG_TAIL_LINES: usize = 200;
/// Слово, которое пользователь вводит для подтверждения удаления.
/// Проверяется и здесь, а не только в UI: случайный/ошибочный вызов
/// команды не должен стирать историю.
const DELETE_CONFIRMATION: &str = "УДАЛИТЬ";

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiagnosticsView {
    pub app_version: String,
    pub db_path: String,
    pub db_size_bytes: u64,
    pub schema_version: i64,
    pub latest_schema_version: i64,
    pub log_path: Option<String>,
    pub startup: StartupReport,
    /// Свежий `quick_check` (не только стартовый).
    pub integrity_error: Option<String>,
    pub issues: Vec<ConsistencyIssue>,
    pub log_tail: Vec<String>,
}

#[tauri::command]
pub fn diagnostics_get(window: WebviewWindow, app: AppHandle, db: State<AppDatabase>) -> Result<DiagnosticsView, String> {
    ensure_main_window(&window)?;
    let startup = app.state::<StartupReport>().inner().clone();
    let conn = db.lock();
    let schema_version: i64 = conn.query_row("SELECT COALESCE(MAX(version), 0) FROM _migrations", [], |r| r.get(0)).map_err(|e| e.to_string())?;
    let db_size_bytes = std::fs::metadata(&startup.db_path).map(|m| m.len()).unwrap_or(0);
    Ok(DiagnosticsView {
        app_version: env!("CARGO_PKG_VERSION").to_string(),
        db_path: startup.db_path.clone(),
        db_size_bytes,
        schema_version,
        latest_schema_version: maintenance::latest_schema_version(),
        log_path: logging::log_file_path().map(|p| p.display().to_string()),
        integrity_error: maintenance::sqlite_quick_check(&conn).err(),
        issues: maintenance::consistency_report(&conn)?,
        log_tail: logging::tail(LOG_TAIL_LINES),
        startup,
    })
}

#[derive(Deserialize, Clone, Copy)]
#[serde(rename_all = "camelCase")]
pub enum ExportKind {
    Json,
    Csv,
    Backup,
}

/// Возвращает путь сохранённого файла или `None`, если пользователь
/// закрыл диалог. `async` — системный диалог блокирующий и не должен
/// выполняться в главном потоке.
#[tauri::command]
pub async fn data_export(window: WebviewWindow, app: AppHandle, db: State<'_, AppDatabase>, kind: ExportKind) -> Result<Option<String>, String> {
    ensure_main_window(&window)?;
    let stamp = chrono::Local::now().format("%Y-%m-%d");
    let (name, filter_name, ext) = match kind {
        ExportKind::Json => (format!("devlog-export-{stamp}.json"), "JSON", "json"),
        ExportKind::Csv => (format!("devlog-sessions-{stamp}.csv"), "CSV", "csv"),
        ExportKind::Backup => (format!("devlog-backup-{stamp}.sqlite3"), "SQLite", "sqlite3"),
    };
    let Some(picked) = app.dialog().file().set_parent(&window).set_file_name(&name).add_filter(filter_name, &[ext]).blocking_save_file() else {
        return Ok(None);
    };
    let path = picked.into_path().map_err(|e| e.to_string())?;

    let conn = db.lock();
    match kind {
        ExportKind::Json => std::fs::write(&path, maintenance::export_json(&conn)?).map_err(|e| e.to_string())?,
        // BOM — чтобы Excel открыл кириллицу в UTF-8 без танцев с импортом.
        ExportKind::Csv => std::fs::write(&path, format!("\u{feff}{}", maintenance::export_sessions_csv(&conn)?)).map_err(|e| e.to_string())?,
        ExportKind::Backup => maintenance::backup_to(&conn, &path)?,
    }
    logging::info("data", format!("экспорт выполнен: {}", path.display()));
    Ok(Some(path.display().to_string()))
}

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum DeleteRequest {
    /// Всё, что началось раньше этого локального дня (`YYYY-MM-DD`).
    #[serde(rename_all = "camelCase")]
    Before { local_date: String },
    AllHistory,
    Everything,
}

#[tauri::command]
pub fn data_delete(window: WebviewWindow, app: AppHandle, db: State<AppDatabase>, request: DeleteRequest, confirmation: String) -> Result<DeleteSummary, String> {
    ensure_main_window(&window)?;
    if confirmation.trim() != DELETE_CONFIRMATION {
        return Err(format!("для удаления введите «{DELETE_CONFIRMATION}»"));
    }
    let scope = match &request {
        DeleteRequest::Before { local_date } => DeleteScope::HistoryBefore(session_engine::resolve_local_day_range(local_date)?.0),
        DeleteRequest::AllHistory => DeleteScope::AllHistory,
        DeleteRequest::Everything => DeleteScope::Everything,
    };
    let summary = maintenance::delete_data(&mut db.lock(), scope)?;
    if scope == DeleteScope::Everything {
        credentials::delete_api_token()?;
    }
    logging::info("data", format!("удаление данных ({scope:?}): {summary:?}"));
    let _ = app.emit("sessions:changed", ());
    Ok(summary)
}
