//! Session Engine (ТЗ, Итерация 4): строит `work_sessions` из сырых
//! `activity_events`. Идемпотентно и безопасно для ручных правок —
//! пересчитывает **только** нетронутые detected-сессии (без истории
//! правок, `source == Detected && !is_manually_edited`), никогда не
//! трогает ручные/отредактированные. Защита от двойного учёта (FR-04.6):
//! время, уже занятое ручной/отредактированной сессией, целиком
//! исключается из повторной группировки.

mod grouping;

use chrono::{Local, TimeZone};
use rusqlite::Connection;

use crate::db::error::RepoResult;
use crate::db::repositories::{activity_events, work_sessions};
use crate::domain::work_session::{CreateWorkSessionInput, WorkSessionSource};

/// Множитель над интервалом опроса — после такого разрыва между
/// событиями run считается прерванным (то же обоснование, что у
/// `tracking::engine::SUSPEND_GAP_MULTIPLIER`: такой разрыв уже
/// подразумевает non-Tracking событие между ними, но дублируем константу
/// здесь намеренно — это отдельная, концептуально самостоятельная
/// настройка группировки, не обязанная меняться вместе с порогом
/// обнаружения suspend).
pub const MAX_GROUPING_GAP_MULTIPLIER: i64 = 3;

/// Моменты локальной полуночи (как UTC-секунды) внутри `(range_start_utc,
/// range_end_utc)` — FR-04.4 "разделять сессии на границе локальной
/// полуночи... не предполагать, что день всегда 86400 секунд" (учитывает
/// переход на летнее/зимнее время, т.к. использует реальный локальный
/// часовой пояс системы через `chrono::Local`, а не фиксированный сдвиг).
fn local_midnight_boundaries_utc(range_start_utc: i64, range_end_utc: i64) -> Vec<i64> {
    let Some(start_dt) = Local.timestamp_opt(range_start_utc, 0).single() else {
        return Vec::new();
    };
    let Some(end_dt) = Local.timestamp_opt(range_end_utc, 0).single() else {
        return Vec::new();
    };

    let mut boundaries = Vec::new();
    let mut date = start_dt.date_naive();
    let end_date = end_dt.date_naive();

    while date <= end_date {
        if let Some(next_date) = date.succ_opt() {
            if let Some(midnight_naive) = next_date.and_hms_opt(0, 0, 0) {
                if let Some(midnight_local) = Local.from_local_datetime(&midnight_naive).single() {
                    let ts = midnight_local.timestamp();
                    if ts > range_start_utc && ts < range_end_utc {
                        boundaries.push(ts);
                    }
                }
            }
            date = next_date;
        } else {
            break;
        }
    }

    boundaries
}

/// Полный локальный календарный день `date` как `[start, end)` в UTC-секундах.
fn local_date_range_utc(date: chrono::NaiveDate) -> Option<(i64, i64)> {
    let start = Local.from_local_datetime(&date.and_hms_opt(0, 0, 0)?).single()?;
    let next_date = date.succ_opt()?;
    let end = Local.from_local_datetime(&next_date.and_hms_opt(0, 0, 0)?).single()?;
    Some((start.timestamp(), end.timestamp()))
}

/// Текущий локальный день целиком — используется `tracking::tracker` для
/// периодической регенерации (конец диапазона в будущем безвреден: там
/// просто ещё нет событий).
pub fn today_local_range_utc() -> (i64, i64) {
    let today = Local::now().date_naive();
    local_date_range_utc(today).unwrap_or_else(|| {
        let now = Local::now().timestamp();
        (now, now + 1)
    })
}

/// Разбирает `"today"` / `"yesterday"` / `""` (= сегодня) / `"YYYY-MM-DD"`
/// в границы локального дня + нормализованную строку даты — для IPC-команд
/// (`get_sessions_for_day`). Возвращает текстовую ошибку вместо
/// `RepoError`, т.к. это ошибка пользовательского ввода, а не БД.
pub fn resolve_local_day_range(input: &str) -> Result<(i64, i64, String), String> {
    let today = Local::now().date_naive();
    let date = match input.trim() {
        "" | "today" => today,
        "yesterday" => today.pred_opt().ok_or("date underflow")?,
        other => chrono::NaiveDate::parse_from_str(other, "%Y-%m-%d")
            .map_err(|e| format!("invalid date '{other}' (expected YYYY-MM-DD): {e}"))?,
    };
    let (start, end) = local_date_range_utc(date).ok_or_else(|| format!("cannot resolve local range for {date}"))?;
    Ok((start, end, date.format("%Y-%m-%d").to_string()))
}

fn system_timezone_id() -> String {
    iana_time_zone::get_timezone().unwrap_or_else(|_| "UTC".to_string())
}

/// Пересобирает detected-сессии в диапазоне `[range_start_utc,
/// range_end_utc)`. Возвращает количество созданных сессий. Один вызов —
/// одна транзакция (атомарно: либо вся регенерация применилась, либо
/// откатилась).
pub fn rebuild_detected_sessions_in_range(
    conn: &mut Connection,
    range_start_utc: i64,
    range_end_utc: i64,
    poll_interval_seconds: i64,
) -> RepoResult<usize> {
    // `include_deleted: true` — критично для protected-диапазонов: мягко
    // удалённая (= "исключённая") сессия всё ещё должна "занимать" своё
    // время, иначе Session Engine создаст новую detected-сессию на том же
    // месте при следующем цикле (кнопка "Исключить" была бы недолговечной —
    // баг, найденный при ручной проверке, см. docs/iterations/05.md).
    let existing = work_sessions::list_by_range(conn, range_start_utc, range_end_utc, true)?;

    let protected: Vec<(i64, i64)> = existing
        .iter()
        .filter(|s| s.is_manually_edited || s.source == WorkSessionSource::Manual)
        .map(|s| (s.started_at_utc, s.ended_at_utc.unwrap_or(range_end_utc)))
        .collect();

    // Регенерируемые — только нетронутые detected-сессии, которые при этом
    // ещё не удалены (удалённая пристинная detected-сессия не должна в
    // принципе существовать после фикса `soft_delete`, выставляющего
    // `is_manually_edited = true`, но проверка оставлена как защита от
    // будущих регрессий).
    let regeneratable_ids: Vec<String> = existing
        .iter()
        .filter(|s| !s.is_manually_edited && s.source == WorkSessionSource::Detected && s.deleted_at.is_none())
        .map(|s| s.id.clone())
        .collect();

    let events = activity_events::list_by_range(conn, range_start_utc, range_end_utc)?;
    let midnight_boundaries = local_midnight_boundaries_utc(range_start_utc, range_end_utc);
    let max_gap = poll_interval_seconds * MAX_GROUPING_GAP_MULTIPLIER;
    let runs = grouping::group_events_into_runs(&events, poll_interval_seconds, max_gap, &protected, &midnight_boundaries);
    let timezone_id = system_timezone_id();

    let tx = conn.transaction()?;
    for id in &regeneratable_ids {
        work_sessions::hard_delete(&tx, id)?;
    }
    let created_count = runs.len();
    for run in runs {
        work_sessions::create(
            &tx,
            &CreateWorkSessionInput {
                started_at_utc: run.started_at_utc,
                ended_at_utc: Some(run.ended_at_utc),
                timezone_id: timezone_id.clone(),
                active_seconds: run.active_seconds,
                source: WorkSessionSource::Detected,
                project_id: run.project_id,
                issue_key: run.issue_key,
                manual_seconds_override: None,
                description: None,
                confidence: Some(run.confidence),
                review_status: Some(run.review_status),
                is_manually_edited: Some(false),
            },
        )?;
    }
    tx.commit()?;

    Ok(created_count)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_midnight_boundaries_are_strictly_inside_the_range() {
        // Диапазон ровно на 2 дня вперёд от текущего локального момента —
        // должно быть минимум 2 полуночи внутри, ни одна не равна границам.
        let now = Local::now().timestamp();
        let two_days_later = now + 2 * 86_400;
        let boundaries = local_midnight_boundaries_utc(now, two_days_later);
        assert!(!boundaries.is_empty());
        for b in &boundaries {
            assert!(*b > now && *b < two_days_later);
        }
    }

    #[test]
    fn no_boundaries_within_the_same_day() {
        // Фиксированный "локальный полдень" вместо `Local::now()` — иначе
        // тест был бы флапающим в узком окне около полуночи.
        let today = Local::now().date_naive();
        let noon = Local.from_local_datetime(&today.and_hms_opt(12, 0, 0).unwrap()).single().unwrap();
        let boundaries = local_midnight_boundaries_utc(noon.timestamp(), noon.timestamp() + 10);
        assert_eq!(boundaries, Vec::<i64>::new());
    }

    #[test]
    fn resolve_empty_and_today_give_the_same_range_as_today_local_range_utc() {
        let expected = today_local_range_utc();
        let (start_empty, end_empty, _) = resolve_local_day_range("").unwrap();
        let (start_today, end_today, date_str) = resolve_local_day_range("today").unwrap();
        assert_eq!((start_empty, end_empty), expected);
        assert_eq!((start_today, end_today), expected);
        assert_eq!(date_str, Local::now().date_naive().format("%Y-%m-%d").to_string());
    }

    #[test]
    fn resolve_yesterday_is_exactly_one_day_before_today() {
        let (today_start, _, _) = resolve_local_day_range("today").unwrap();
        let (yesterday_start, yesterday_end, _) = resolve_local_day_range("yesterday").unwrap();
        assert_eq!(yesterday_end, today_start);
        // Обычно ровно 86400с; 82800/90000 — переход на летнее/зимнее время.
        let day_len = today_start - yesterday_start;
        assert!([82_800, 86_400, 90_000].contains(&day_len), "unexpected day length: {day_len}");
    }

    #[test]
    fn resolve_explicit_date_parses_correctly() {
        let (start, end, date_str) = resolve_local_day_range("2026-01-15").unwrap();
        assert!(start < end);
        assert_eq!(date_str, "2026-01-15");
    }

    #[test]
    fn resolve_rejects_malformed_date() {
        assert!(resolve_local_day_range("not-a-date").is_err());
        assert!(resolve_local_day_range("2026-13-99").is_err());
    }

    /// Интеграционный регрессионный тест (нашёлся при ручной проверке
    /// Итерации 5 со скриншотами UI, см. docs/iterations/05.md): исключение
    /// (`soft_delete`) detected-сессии должно быть долговечным — повторная
    /// регенерация не должна создавать новую detected-сессию на том же
    /// месте.
    #[test]
    fn excluded_session_is_not_resurrected_by_next_regeneration() {
        use crate::db::repositories::activity_events as events_repo;
        use crate::db::test_support::temp_database;
        use crate::domain::activity_event::CreateActivityEventInput;
        use crate::domain::confidence::Confidence;
        use crate::domain::tracking_status::TrackingStatus;

        let (mut conn, _dir) = temp_database();
        let day_start = Local::now().date_naive().and_hms_opt(9, 0, 0).unwrap();
        let start_utc = Local.from_local_datetime(&day_start).single().unwrap().timestamp();

        for i in 0..3 {
            events_repo::insert(
                &conn,
                &CreateActivityEventInput {
                    timestamp_utc: start_utc + i * 5,
                    process_name_sanitized: "webstorm64.exe".into(),
                    app_category: "ide".into(),
                    project_id: None,
                    branch: None,
                    detected_issue_key: None,
                    idle_seconds: Some(0),
                    state: TrackingStatus::Tracking,
                    confidence: Confidence::High,
                    reason: "test".into(),
                },
            )
            .unwrap();
        }

        let (day_s, day_e) = (start_utc - 3600, start_utc + 3600);
        rebuild_detected_sessions_in_range(&mut conn, day_s, day_e, 5).unwrap();

        let before = work_sessions::list_by_range(&conn, day_s, day_e, false).unwrap();
        assert_eq!(before.len(), 1, "ожидалась ровно одна обнаруженная сессия");
        work_sessions::soft_delete(&mut conn, &before[0].id).unwrap();

        // Повторная регенерация — как будто прошёл ещё один цикл трекера.
        rebuild_detected_sessions_in_range(&mut conn, day_s, day_e, 5).unwrap();

        let after_visible = work_sessions::list_by_range(&conn, day_s, day_e, false).unwrap();
        assert_eq!(after_visible.len(), 0, "исключённая сессия не должна воскреснуть как новая detected");

        let after_all = work_sessions::list_by_range(&conn, day_s, day_e, true).unwrap();
        assert_eq!(after_all.len(), 1, "сама исключённая сессия должна остаться (мягко удалённой), не продублироваться");
    }
}
