//! IPC для Jira и Worklog Review (Итерация 7). Логика — в `worklog::*` и
//! `integrations::jira`; здесь только валидация аргументов, проверка
//! вызывающего окна и сборка провайдера из сохранённых настроек.
//!
//! NFR (раздел 7): "доступ к Jira HTTP-клиенту только там, где реально
//! нужен" — все команды этого файла разрешены только главному окну.
//! Токен никогда не возвращается во frontend: команды отдают только
//! `JiraConnectionStatus` без секрета.

use serde::{Deserialize, Serialize};
use tauri::{State, WebviewWindow};

use crate::db::app_database::AppDatabase;
use crate::db::repositories::{issues_cache, jira_submissions, worklog_drafts};
use crate::domain::jira::{JiraConnectionInput, JiraConnectionStatus, JiraIssueSummary, JiraSubmission, JiraUser};
use crate::domain::worklog_draft::{UpdateWorklogDraftPatch, WorklogDraft};
use crate::integrations::jira::{credentials, JiraCloudProvider, JiraProvider, JIRA_CLOUD_PROVIDER_ID};
use crate::tracking::issue_key::is_valid_issue_key;
use crate::tracking::session_engine;
use crate::windows::main_window::MAIN_LABEL;
use crate::worklog::drafts::{self, MIN_WORKLOG_SECONDS};
use crate::worklog::submission::{self, DraftSubmitResult, SubmitOutcome};

pub(super) fn ensure_main_window(window: &WebviewWindow) -> Result<(), String> {
    if window.label() == MAIN_LABEL {
        Ok(())
    } else {
        Err("Jira доступна только из главного окна".into())
    }
}

pub(super) fn lock(db: &AppDatabase) -> Result<std::sync::MutexGuard<'_, rusqlite::Connection>, String> {
    db.0.lock().map_err(|_| "database lock poisoned".to_string())
}

fn build_provider(db: &AppDatabase) -> Result<JiraCloudProvider, String> {
    let (base_url, email) = credentials::load_connection_details(&*lock(db)?);
    let token = credentials::load_api_token();
    match (base_url, email, token) {
        (Some(base_url), Some(email), Some(token)) => Ok(JiraCloudProvider::new(base_url, email, token)),
        _ => Err(crate::domain::jira::JiraError::NotConfigured.to_string()),
    }
}

/// `https://company.atlassian.net/` → `https://company.atlassian.net`.
/// Только https: Basic-auth с токеном по открытому каналу недопустим.
fn normalize_base_url(raw: &str) -> Result<String, String> {
    let trimmed = raw.trim().trim_end_matches('/');
    let rest = trimmed.strip_prefix("https://").ok_or("адрес Jira должен начинаться с https://")?;
    if rest.is_empty() || rest.contains(char::is_whitespace) || rest.contains(['?', '#', '@']) {
        return Err("некорректный адрес Jira".into());
    }
    Ok(trimmed.to_string())
}

// ---------- Соединение ----------

#[tauri::command]
pub fn jira_get_connection_status(window: WebviewWindow, db: State<AppDatabase>) -> Result<JiraConnectionStatus, String> {
    ensure_main_window(&window)?;
    let (base_url, email) = credentials::load_connection_details(&*lock(&db)?);
    let configured = base_url.is_some() && email.is_some() && credentials::load_api_token().is_some();
    Ok(JiraConnectionStatus { configured, base_url, email })
}

/// Сохраняет адрес/email в settings, токен — в Windows Credential Manager
/// (FR-07.4). Пустой токен — оставить уже сохранённый (UI не умеет и не
/// должен показывать старый токен, чтобы его "переотправить").
#[tauri::command]
pub fn jira_save_connection(window: WebviewWindow, db: State<AppDatabase>, input: JiraConnectionInput) -> Result<JiraConnectionStatus, String> {
    ensure_main_window(&window)?;
    let base_url = normalize_base_url(&input.base_url)?;
    let email = input.email.trim();
    if !email.contains('@') {
        return Err("некорректный email".into());
    }
    let token = input.api_token.trim();
    if token.is_empty() && credentials::load_api_token().is_none() {
        return Err("укажите API token".into());
    }
    if !token.is_empty() {
        credentials::store_api_token(token)?;
    }
    credentials::store_connection_details(&*lock(&db)?, &base_url, email).map_err(|e| e.to_string())?;
    Ok(JiraConnectionStatus { configured: true, base_url: Some(base_url), email: Some(email.to_string()) })
}

#[tauri::command]
pub fn jira_clear_connection(window: WebviewWindow, db: State<AppDatabase>) -> Result<(), String> {
    ensure_main_window(&window)?;
    credentials::delete_api_token()?;
    credentials::clear_connection_details(&*lock(&db)?).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn jira_test_connection(window: WebviewWindow, db: State<'_, AppDatabase>) -> Result<JiraUser, String> {
    ensure_main_window(&window)?;
    let provider = build_provider(&db)?;
    provider.verify_connection().await.map_err(|e| e.to_string())
}

/// Название задачи из Jira → в `issues_cache` (для офлайн-preview).
#[tauri::command]
pub async fn jira_fetch_issue(window: WebviewWindow, db: State<'_, AppDatabase>, issue_key: String) -> Result<JiraIssueSummary, String> {
    ensure_main_window(&window)?;
    if !is_valid_issue_key(&issue_key) {
        return Err(format!("некорректный ключ задачи «{issue_key}»"));
    }
    let provider = build_provider(&db)?;
    let issue = provider.get_issue(&issue_key).await.map_err(|e| e.to_string())?;
    issues_cache::upsert(&*lock(&db)?, &issue.issue_key, &issue.title, JIRA_CLOUD_PROVIDER_ID).map_err(|e| e.to_string())?;
    Ok(issue)
}

// ---------- Черновики ----------

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorklogDraftView {
    #[serde(flatten)]
    pub draft: WorklogDraft,
    pub issue_title: Option<String>,
    /// Последняя попытка отправки (если была) — для статуса и
    /// reconciliation-кнопок.
    pub last_submission: Option<JiraSubmission>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorklogDayView {
    pub local_date: String,
    pub drafts: Vec<WorklogDraftView>,
}

fn build_day_view(conn: &rusqlite::Connection, local_date: String, list: Vec<WorklogDraft>) -> Result<WorklogDayView, String> {
    let mut views = Vec::with_capacity(list.len());
    for draft in list {
        // `list_by_local_draft` отсортирован от новых к старым.
        let last_submission = jira_submissions::list_by_local_draft(conn, &draft.id).map_err(|e| e.to_string())?.into_iter().next();
        views.push(WorklogDraftView { issue_title: drafts::cached_title(conn, &draft.issue_key), last_submission, draft });
    }
    Ok(WorklogDayView { local_date, drafts: views })
}

#[tauri::command]
pub fn worklog_get_day(window: WebviewWindow, db: State<AppDatabase>, local_date: String) -> Result<WorklogDayView, String> {
    ensure_main_window(&window)?;
    let (_, _, resolved) = session_engine::resolve_local_day_range(&local_date)?;
    let conn = lock(&db)?;
    let list = worklog_drafts::list_by_local_day(&conn, &resolved).map_err(|e| e.to_string())?;
    build_day_view(&conn, resolved, list)
}

#[tauri::command]
pub fn worklog_generate_drafts(window: WebviewWindow, db: State<AppDatabase>, local_date: String) -> Result<WorklogDayView, String> {
    ensure_main_window(&window)?;
    let (start, end, resolved) = session_engine::resolve_local_day_range(&local_date)?;
    let conn = lock(&db)?;
    let list = drafts::generate_for_day(&conn, &resolved, start, end).map_err(|e| e.to_string())?;
    build_day_view(&conn, resolved, list)
}

/// Правка черновика перед отправкой (FR-07.5 / DoD: "перед публикацией
/// можно изменить сумму и описание").
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorklogDraftEdit {
    pub issue_key: Option<String>,
    pub time_spent_seconds: Option<i64>,
    pub started_at_utc: Option<i64>,
    pub comment: Option<String>,
}

#[tauri::command]
pub fn worklog_update_draft(window: WebviewWindow, db: State<AppDatabase>, id: String, edit: WorklogDraftEdit) -> Result<WorklogDraft, String> {
    ensure_main_window(&window)?;
    if let Some(key) = &edit.issue_key {
        if !is_valid_issue_key(key) {
            return Err(format!("некорректный ключ задачи «{key}»"));
        }
    }
    if let Some(seconds) = edit.time_spent_seconds {
        if seconds < MIN_WORKLOG_SECONDS || seconds % 60 != 0 {
            return Err("время — целое число минут, не меньше одной".into());
        }
    }
    let conn = lock(&db)?;
    let draft = worklog_drafts::get_by_id(&conn, &id).map_err(|e| e.to_string())?.ok_or("черновик не найден")?;
    drafts::ensure_editable(&conn, &draft)?;
    let patch = UpdateWorklogDraftPatch {
        issue_key: edit.issue_key,
        time_spent_seconds: edit.time_spent_seconds,
        started_at_utc: edit.started_at_utc,
        comment: edit.comment.map(|c| Some(c).filter(|c| !c.trim().is_empty())),
        ..Default::default()
    };
    worklog_drafts::update(&conn, &id, &patch).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn worklog_delete_draft(window: WebviewWindow, db: State<AppDatabase>, id: String) -> Result<(), String> {
    ensure_main_window(&window)?;
    let conn = lock(&db)?;
    let draft = worklog_drafts::get_by_id(&conn, &id).map_err(|e| e.to_string())?.ok_or("черновик не найден")?;
    drafts::ensure_editable(&conn, &draft)?;
    worklog_drafts::remove(&conn, &id).map_err(|e| e.to_string())
}

// ---------- Отправка ----------

/// FR-07.5: вызывается только по явному нажатию «Отправить» после preview,
/// с конкретным списком выбранных черновиков.
#[tauri::command]
pub async fn jira_submit_drafts(window: WebviewWindow, db: State<'_, AppDatabase>, draft_ids: Vec<String>) -> Result<Vec<DraftSubmitResult>, String> {
    ensure_main_window(&window)?;
    if draft_ids.is_empty() {
        return Err("не выбрано ни одной записи".into());
    }
    let provider = build_provider(&db)?;
    Ok(submission::submit_drafts(&db.0, &provider, &draft_ids).await)
}

#[tauri::command]
pub async fn jira_reconcile_submission(window: WebviewWindow, db: State<'_, AppDatabase>, submission_id: String) -> Result<SubmitOutcome, String> {
    ensure_main_window(&window)?;
    let provider = build_provider(&db)?;
    submission::reconcile(&db.0, &provider, &submission_id).await
}

/// Все неразрешённые попытки за любые дни — чтобы `unknown` за позавчера
/// не потерялся, пока пользователь смотрит "вчера".
#[tauri::command]
pub fn jira_list_unknown_submissions(window: WebviewWindow, db: State<AppDatabase>) -> Result<Vec<UnknownSubmissionView>, String> {
    ensure_main_window(&window)?;
    let conn = lock(&db)?;
    let unknown = jira_submissions::list_unknown(&conn).map_err(|e| e.to_string())?;
    Ok(unknown
        .into_iter()
        .map(|submission| {
            let local_day = worklog_drafts::get_by_id(&conn, &submission.local_draft_id).ok().flatten().map(|d| d.local_day);
            UnknownSubmissionView { submission, local_day }
        })
        .collect())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UnknownSubmissionView {
    #[serde(flatten)]
    pub submission: JiraSubmission,
    pub local_day: Option<String>,
}

#[tauri::command]
pub fn jira_resolve_submission_manually(window: WebviewWindow, db: State<AppDatabase>, submission_id: String, posted: bool) -> Result<SubmitOutcome, String> {
    ensure_main_window(&window)?;
    let mut conn = lock(&db)?;
    submission::resolve_manually(&mut conn, &submission_id, posted)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base_url_must_be_https_and_is_normalized() {
        assert_eq!(normalize_base_url(" https://acme.atlassian.net/ ").unwrap(), "https://acme.atlassian.net");
        assert!(normalize_base_url("http://acme.atlassian.net").is_err());
        assert!(normalize_base_url("acme.atlassian.net").is_err());
        assert!(normalize_base_url("https://").is_err());
        assert!(normalize_base_url("https://user@evil.example").is_err());
    }
}
