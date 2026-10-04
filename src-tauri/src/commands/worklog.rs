//! IPC Итерации 8: шаблоны комментариев, пересчёт черновика, напоминание о
//! неотчитанном дне. Отправка в Jira и CRUD черновиков — `commands/jira.rs`.
//! Как и там, только главное окно.

use tauri::{State, WebviewWindow};

use super::jira::{ensure_main_window, lock};
use crate::db::app_database::AppDatabase;
use crate::domain::worklog_draft::WorklogDraft;
use crate::tracking::session_engine;
use crate::worklog::reminder::{self, WorklogReminder};
use crate::worklog::templates::{self, TemplateSettings};
use crate::worklog::drafts;

#[tauri::command]
pub fn worklog_get_templates(window: WebviewWindow, db: State<AppDatabase>) -> Result<TemplateSettings, String> {
    ensure_main_window(&window)?;
    Ok(templates::load(&*lock(&db)?))
}

/// Заменяет список целиком (как whitelist): UI редактирует копию и
/// сохраняет её одним вызовом.
#[tauri::command]
pub fn worklog_save_templates(window: WebviewWindow, db: State<AppDatabase>, value: TemplateSettings) -> Result<TemplateSettings, String> {
    ensure_main_window(&window)?;
    templates::save(&*lock(&db)?, value)
}

#[tauri::command]
pub fn worklog_apply_template(window: WebviewWindow, db: State<AppDatabase>, draft_id: String, template_id: String) -> Result<WorklogDraft, String> {
    ensure_main_window(&window)?;
    drafts::apply_template(&*lock(&db)?, &draft_id, &template_id)
}

#[tauri::command]
pub fn worklog_recalculate_draft(window: WebviewWindow, db: State<AppDatabase>, draft_id: String) -> Result<WorklogDraft, String> {
    ensure_main_window(&window)?;
    let conn = lock(&db)?;
    let local_day = crate::db::repositories::worklog_drafts::get_by_id(&conn, &draft_id).map_err(|e| e.to_string())?.ok_or("черновик не найден")?.local_day;
    let (start, end, _) = session_engine::resolve_local_day_range(&local_day)?;
    drafts::recalculate(&conn, &draft_id, start, end)
}

#[tauri::command]
pub fn worklog_get_reminder(window: WebviewWindow, db: State<AppDatabase>) -> Result<Option<WorklogReminder>, String> {
    ensure_main_window(&window)?;
    let today = chrono::Local::now().date_naive();
    reminder::compute(&*lock(&db)?, today, session_engine::local_date_range_utc)
}

#[tauri::command]
pub fn worklog_dismiss_reminder(window: WebviewWindow, db: State<AppDatabase>, local_date: String) -> Result<(), String> {
    ensure_main_window(&window)?;
    reminder::dismiss(&*lock(&db)?, &local_date)
}
