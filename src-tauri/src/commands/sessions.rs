use serde::Serialize;
use tauri::{AppHandle, Emitter, State};

use crate::db::app_database::AppDatabase;
use crate::db::repositories::work_sessions;
use crate::domain::work_session::{
    CreateWorkSessionInput, UpdateWorkSessionPatch, WorkSession, WorkSessionReviewStatus, WorkSessionSource,
};
use crate::tracking::session_engine;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DaySessionsView {
    pub local_date: String,
    pub sessions: Vec<WorkSession>,
    /// Всё, что не `Excluded` (Detected + Unassigned + Reviewed) — это и
    /// есть "итог за день" (FR-05.1). Excluded-время — не работа, в сумму
    /// не входит.
    pub total_active_seconds: i64,
    /// Подмножество `total_active_seconds`, где задача не определена —
    /// отдельная цифра на Dashboard (FR-05.1 "количество нераспределённого времени").
    pub unassigned_seconds: i64,
    pub excluded_seconds: i64,
}

/// (total_active, unassigned, excluded, session_count) — общий расчёт для
/// `DaySessionsView` (полный день) и `DaySummary` (сводка в аккордеоне
/// Timeline). Мягко удалённые (FR-05.4) не считаются ни во что, но из
/// `sessions`-списка их не убирают — см. комментарий у `DaySessionsView`.
fn compute_day_totals(sessions: &[WorkSession]) -> (i64, i64, i64, i64) {
    let mut total = 0i64;
    let mut unassigned = 0i64;
    let mut excluded = 0i64;
    let mut count = 0i64;
    for s in sessions {
        if s.deleted_at.is_some() {
            continue;
        }
        count += 1;
        match s.review_status {
            WorkSessionReviewStatus::Excluded => excluded += s.active_seconds,
            WorkSessionReviewStatus::Unassigned => {
                unassigned += s.active_seconds;
                total += s.active_seconds;
            }
            _ => total += s.active_seconds,
        }
    }
    (total, unassigned, excluded, count)
}

fn build_day_view(local_date: String, sessions: Vec<WorkSession>) -> DaySessionsView {
    // Мягко удалённые (FR-05.4 "исключить" через soft_delete) остаются в
    // `sessions` ниже, чтобы UI мог их показать (приглушённо) и предложить
    // "Восстановить" (FR-05.4) — именно поэтому они не отфильтрованы перед
    // этим вызовом, а пропускаются только внутри `compute_day_totals`. Если
    // бы мы отдавали только неудалённые, кнопка "Восстановить" была бы
    // недостижима — ровно такой баг был найден и исправлен при ручной
    // проверке Итерации 5 (см. docs/iterations/05.md).
    let (total, unassigned, excluded, _count) = compute_day_totals(&sessions);
    DaySessionsView { local_date, sessions, total_active_seconds: total, unassigned_seconds: unassigned, excluded_seconds: excluded }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DaySummary {
    pub local_date: String,
    pub total_active_seconds: i64,
    pub unassigned_seconds: i64,
    pub excluded_seconds: i64,
    pub session_count: i64,
}

/// Все команды-мутаторы в этом файле эмитят `sessions:changed` при успехе —
/// main-window и overlay (когда появится) подписываются на одно и то же
/// событие вместо поллинга (ТЗ, раздел 6).
fn notify_sessions_changed(app: &AppHandle) {
    let _ = app.emit("sessions:changed", ());
}

/// `local_date` — `YYYY-MM-DD` в локальной таймзоне пользователя (не UTC!).
/// Пустая строка или `"today"` — сегодня.
#[tauri::command]
pub fn get_sessions_for_day(db: State<AppDatabase>, local_date: String) -> Result<DaySessionsView, String> {
    let (start, end, resolved_date) = session_engine::resolve_local_day_range(&local_date).map_err(|e| e.to_string())?;
    let conn = db.lock();
    let sessions = work_sessions::list_by_range(&conn, start, end, true).map_err(|e| e.to_string())?;
    Ok(build_day_view(resolved_date, sessions))
}

/// Сводки по последним `days` календарным дням (от сегодня назад) — для
/// списка-аккордеона в Timeline: сам список сессий конкретного дня
/// (который может быть длинным — сессии по 1-5 минут друг от друга)
/// подгружается лениво через `get_sessions_for_day`, только когда
/// пользователь разворачивает день, а не все дни сразу. Дни без единой
/// сессии не включаются в результат — незачем занимать строчку в списке
/// пустым днём (выходные и т.п.).
#[tauri::command]
pub fn list_day_summaries(db: State<AppDatabase>, days: u32) -> Result<Vec<DaySummary>, String> {
    let conn = db.lock();
    let today = chrono::Local::now().date_naive();
    let mut result = Vec::new();
    for offset in 0..days {
        let Some(date) = today.checked_sub_signed(chrono::Duration::days(i64::from(offset))) else { break };
        let Some((start, end)) = session_engine::local_date_range_utc(date) else { continue };
        let sessions = work_sessions::list_by_range(&conn, start, end, true).map_err(|e| e.to_string())?;
        let (total, unassigned, excluded, count) = compute_day_totals(&sessions);
        if count == 0 {
            continue;
        }
        result.push(DaySummary {
            local_date: date.format("%Y-%m-%d").to_string(),
            total_active_seconds: total,
            unassigned_seconds: unassigned,
            excluded_seconds: excluded,
            session_count: count,
        });
    }
    Ok(result)
}

#[tauri::command]
pub fn update_session(app: AppHandle, db: State<AppDatabase>, id: String, patch: UpdateWorkSessionPatch) -> Result<WorkSession, String> {
    let mut conn = db.lock();
    let result = work_sessions::update(&mut conn, &id, &patch).map_err(|e| e.to_string())?;
    notify_sessions_changed(&app);
    Ok(result)
}

#[tauri::command]
pub fn split_session(app: AppHandle, db: State<AppDatabase>, id: String, at_utc: i64) -> Result<(WorkSession, WorkSession), String> {
    let mut conn = db.lock();
    let result = work_sessions::split(&mut conn, &id, at_utc).map_err(|e| e.to_string())?;
    notify_sessions_changed(&app);
    Ok(result)
}

#[tauri::command]
pub fn merge_sessions(app: AppHandle, db: State<AppDatabase>, first_id: String, second_id: String) -> Result<WorkSession, String> {
    let mut conn = db.lock();
    let result = work_sessions::merge(&mut conn, &first_id, &second_id).map_err(|e| e.to_string())?;
    notify_sessions_changed(&app);
    Ok(result)
}

#[tauri::command]
pub fn exclude_session(app: AppHandle, db: State<AppDatabase>, id: String) -> Result<WorkSession, String> {
    let mut conn = db.lock();
    let result = work_sessions::soft_delete(&mut conn, &id).map_err(|e| e.to_string())?;
    notify_sessions_changed(&app);
    Ok(result)
}

#[tauri::command]
pub fn restore_session(app: AppHandle, db: State<AppDatabase>, id: String) -> Result<WorkSession, String> {
    let mut conn = db.lock();
    let result = work_sessions::restore(&mut conn, &id).map_err(|e| e.to_string())?;
    notify_sessions_changed(&app);
    Ok(result)
}

#[tauri::command]
pub fn undo_session_edit(app: AppHandle, db: State<AppDatabase>, id: String) -> Result<Option<WorkSession>, String> {
    let mut conn = db.lock();
    let result = work_sessions::undo_last_edit(&mut conn, &id).map_err(|e| e.to_string())?;
    notify_sessions_changed(&app);
    Ok(result)
}

/// FR-04.5: ручные сессии (встречи, изучение документации и т.п.) —
/// `source` всегда принудительно `Manual` независимо от того, что прислал
/// frontend (не доверяем клиенту этот выбор).
#[tauri::command]
pub fn create_manual_session(app: AppHandle, db: State<AppDatabase>, mut input: CreateWorkSessionInput) -> Result<WorkSession, String> {
    input.source = WorkSessionSource::Manual;
    let conn = db.lock();
    let result = work_sessions::create(&conn, &input).map_err(|e| e.to_string())?;
    notify_sessions_changed(&app);
    Ok(result)
}
