//! Напоминание в приложении о неотчитанном прошлом дне (Итерация 8: "о
//! незаполненных вчерашних сессиях").
//!
//! "Вчера" = последний день до сегодняшнего, в который вообще была работа
//! (в пределах недели): в понедельник напоминание про пятницу полезнее,
//! чем пустое воскресенье. Напоминание показывается, если за этот день есть
//! нераспределённое время или время с задачей, не ушедшее в Jira.

use std::collections::HashSet;

use chrono::{Days, NaiveDate};
use rusqlite::Connection;
use serde::Serialize;

use crate::db::repositories::{settings, work_sessions, worklog_drafts};
use crate::domain::work_session::{WorkSession, WorkSessionReviewStatus};
use crate::domain::worklog_draft::WorklogDraftStatus;

use super::drafts::{effective_seconds, MIN_WORKLOG_SECONDS};

pub const SETTINGS_KEY_DISMISSED_FOR: &str = "worklog.reminderDismissedFor";
const LOOKBACK_DAYS: u64 = 7;

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorklogReminder {
    /// `YYYY-MM-DD` дня, о котором напоминание.
    pub local_date: String,
    /// Время без задачи — его сначала назначают в Timeline.
    pub unassigned_seconds: i64,
    /// Время с задачей, которое ещё не отправлено в Jira.
    pub unreported_seconds: i64,
    pub unreported_issue_count: usize,
}

fn counts(s: &WorkSession) -> bool {
    s.deleted_at.is_none() && s.review_status != WorkSessionReviewStatus::Excluded && s.ended_at_utc.is_some() && effective_seconds(s) > 0
}

/// `day_range` — границы локального дня в UTC (в приложении — по системной
/// таймзоне, в тестах — любая фиксированная).
pub fn compute(conn: &Connection, today: NaiveDate, day_range: impl Fn(NaiveDate) -> Option<(i64, i64)>) -> Result<Option<WorklogReminder>, String> {
    for offset in 1..=LOOKBACK_DAYS {
        let Some(date) = today.checked_sub_days(Days::new(offset)) else { break };
        let Some((start, end)) = day_range(date) else { continue };
        let sessions: Vec<WorkSession> = work_sessions::list_by_range(conn, start, end, false).map_err(|e| e.to_string())?.into_iter().filter(counts).collect();
        if sessions.is_empty() {
            continue;
        }

        let local_date = date.format("%Y-%m-%d").to_string();
        if settings::get(conn, SETTINGS_KEY_DISMISSED_FOR).map_err(|e| e.to_string())?.as_deref() == Some(local_date.as_str()) {
            return Ok(None);
        }

        let drafts = worklog_drafts::list_by_local_day(conn, &local_date).map_err(|e| e.to_string())?;
        let reported: HashSet<&str> =
            drafts.iter().filter(|d| d.status == WorklogDraftStatus::Submitted).flat_map(|d| d.selected_session_ids.iter().map(String::as_str)).collect();

        let unassigned_seconds = sessions.iter().filter(|s| s.issue_key.is_none()).map(effective_seconds).sum();
        let unreported: Vec<&WorkSession> = sessions.iter().filter(|s| s.issue_key.is_some() && !reported.contains(s.id.as_str())).collect();
        let unreported_seconds = unreported.iter().map(|s| effective_seconds(s)).sum();
        let unreported_issue_count = unreported.iter().filter_map(|s| s.issue_key.as_deref()).collect::<HashSet<_>>().len();

        // Секундные огрызки (меньше минуты) не повод дёргать пользователя.
        if unassigned_seconds < MIN_WORKLOG_SECONDS && unreported_seconds < MIN_WORKLOG_SECONDS {
            return Ok(None);
        }
        return Ok(Some(WorklogReminder { local_date, unassigned_seconds, unreported_seconds, unreported_issue_count }));
    }
    Ok(None)
}

/// "Скрыть" — до следующего дня с работой (храним дату, а не флаг).
pub fn dismiss(conn: &Connection, local_date: &str) -> Result<(), String> {
    NaiveDate::parse_from_str(local_date, "%Y-%m-%d").map_err(|e| format!("invalid date '{local_date}': {e}"))?;
    settings::set(conn, SETTINGS_KEY_DISMISSED_FOR, local_date).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::test_support::temp_database;
    use crate::domain::work_session::{CreateWorkSessionInput, WorkSessionSource};
    use crate::domain::worklog_draft::{CreateWorklogDraftInput, UpdateWorklogDraftPatch};

    // 2026-10-05 — понедельник.
    fn today() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 10, 5).unwrap()
    }

    fn utc_range(date: NaiveDate) -> Option<(i64, i64)> {
        let start = date.and_hms_opt(0, 0, 0)?.and_utc().timestamp();
        Some((start, start + 86_400))
    }

    fn add_session(conn: &Connection, date: NaiveDate, hour: i64, seconds: i64, issue_key: Option<&str>) -> String {
        let start = utc_range(date).unwrap().0 + hour * 3600;
        work_sessions::create(
            conn,
            &CreateWorkSessionInput {
                started_at_utc: start,
                ended_at_utc: Some(start + seconds),
                timezone_id: "UTC".into(),
                active_seconds: seconds,
                source: WorkSessionSource::Manual,
                project_id: None,
                issue_key: issue_key.map(str::to_string),
                manual_seconds_override: None,
                description: None,
                confidence: None,
                review_status: None,
                is_manually_edited: None,
            },
        )
        .unwrap()
        .id
    }

    fn friday() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 10, 2).unwrap()
    }

    #[test]
    fn monday_reminds_about_friday_not_empty_weekend() {
        let (conn, _dir) = temp_database();
        add_session(&conn, friday(), 9, 5400, Some("OB-448"));
        add_session(&conn, friday(), 11, 900, None);

        let r = compute(&conn, today(), utc_range).unwrap().unwrap();
        assert_eq!(r, WorklogReminder { local_date: "2026-10-02".into(), unassigned_seconds: 900, unreported_seconds: 5400, unreported_issue_count: 1 });
    }

    #[test]
    fn fully_submitted_day_gives_no_reminder() {
        let (conn, _dir) = temp_database();
        let id = add_session(&conn, friday(), 9, 5400, Some("OB-448"));
        let draft = worklog_drafts::create(
            &conn,
            &CreateWorklogDraftInput { local_day: "2026-10-02".into(), issue_key: "OB-448".into(), time_spent_seconds: 5400, started_at_utc: 0, comment: None, selected_session_ids: vec![id], status: None },
        )
        .unwrap();
        assert!(compute(&conn, today(), utc_range).unwrap().is_some(), "черновик ещё не отправлен");

        worklog_drafts::update(&conn, &draft.id, &UpdateWorklogDraftPatch { status: Some(WorklogDraftStatus::Submitted), ..Default::default() }).unwrap();
        assert_eq!(compute(&conn, today(), utc_range).unwrap(), None);
    }

    #[test]
    fn dismissed_day_is_silent_until_a_newer_day_has_work() {
        let (conn, _dir) = temp_database();
        add_session(&conn, friday(), 9, 5400, Some("OB-448"));
        dismiss(&conn, "2026-10-02").unwrap();
        assert_eq!(compute(&conn, today(), utc_range).unwrap(), None);

        let sunday = NaiveDate::from_ymd_opt(2026, 10, 4).unwrap();
        add_session(&conn, sunday, 10, 600, Some("OB-1"));
        assert_eq!(compute(&conn, today(), utc_range).unwrap().unwrap().local_date, "2026-10-04");
    }

    #[test]
    fn nothing_worked_in_a_week_gives_no_reminder() {
        let (conn, _dir) = temp_database();
        add_session(&conn, NaiveDate::from_ymd_opt(2026, 9, 20).unwrap(), 9, 5400, Some("OB-448"));
        assert_eq!(compute(&conn, today(), utc_range).unwrap(), None);
    }

    #[test]
    fn dismiss_rejects_garbage_date() {
        let (conn, _dir) = temp_database();
        assert!(dismiss(&conn, "вчера").is_err());
    }
}
