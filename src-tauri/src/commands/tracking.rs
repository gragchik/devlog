use tauri::{AppHandle, Emitter, State};

use crate::db::app_database::AppDatabase;
use crate::tracking::pinned_task;
use crate::tracking::tracker::ActivityTrackerHandle;

#[tauri::command]
pub fn get_tracking_paused(tracker: State<ActivityTrackerHandle>) -> bool {
    tracker.is_paused()
}

#[tauri::command]
pub fn set_tracking_paused(app: AppHandle, tracker: State<ActivityTrackerHandle>, paused: bool) -> bool {
    tracker.set_paused(paused);
    let _ = app.emit("tracking:changed", ());
    tracker.is_paused()
}

#[tauri::command]
pub fn get_pinned_issue(db: State<AppDatabase>) -> Result<Option<String>, String> {
    let conn = db.0.lock().map_err(|_| "database lock poisoned".to_string())?;
    Ok(pinned_task::get_pinned_issue_key(&conn))
}

/// FR-06.10: закрепление задачи в overlay — немедленно завершает прежнюю
/// сессию и начинает новую (через то, что следующий же poll трекера
/// увидит новый пин и смена ключа группировки сама закроет текущий run —
/// см. `tracking::tracker::resolve_git_context`, `session_engine::grouping`).
#[tauri::command]
pub fn set_pinned_issue(app: AppHandle, db: State<AppDatabase>, issue_key: Option<String>) -> Result<(), String> {
    let conn = db.0.lock().map_err(|_| "database lock poisoned".to_string())?;
    pinned_task::set_pinned_issue_key(&conn, issue_key.as_deref()).map_err(|e| e.to_string())?;
    let _ = app.emit("tracking:changed", ());
    Ok(())
}
