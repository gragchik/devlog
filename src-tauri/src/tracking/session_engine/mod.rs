//! Session Engine (ТЗ, Итерация 4): строит `work_sessions` из сырых
//! `activity_events`. Идемпотентно и безопасно для ручных правок —
//! пересчитывает **только** нетронутые detected-сессии (без истории
//! правок, `source == Detected && !is_manually_edited`), никогда не
//! трогает ручные/отредактированные. Защита от двойного учёта (FR-04.6):
//! время, уже занятое ручной/отредактированной сессией, целиком
//! исключается из повторной группировки.

mod grouping;

use chrono::{Datelike, Local, TimeZone};
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

/// Текущий год/месяц/день локально — используется вызывающим кодом
/// (`tracking::tracker`) для определения границ "сегодня" без утечки
/// `chrono`-типов наружу этого модуля.
pub fn today_local_range_utc() -> (i64, i64) {
    let now = Local::now();
    let start_of_day =
        Local.with_ymd_and_hms(now.year(), now.month(), now.day(), 0, 0, 0).single().unwrap_or(now);
    (start_of_day.timestamp(), now.timestamp() + 1)
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
    let existing = work_sessions::list_by_range(conn, range_start_utc, range_end_utc, false)?;

    let protected: Vec<(i64, i64)> = existing
        .iter()
        .filter(|s| s.is_manually_edited || s.source == WorkSessionSource::Manual)
        .map(|s| (s.started_at_utc, s.ended_at_utc.unwrap_or(range_end_utc)))
        .collect();

    let regeneratable_ids: Vec<String> = existing
        .iter()
        .filter(|s| !s.is_manually_edited && s.source == WorkSessionSource::Detected)
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
}
