//! Составление черновиков worklog из сессий дня (FR-05.5, FR-08.1).
//! Чистая логика над `&Connection` — без Tauri и без сети.

use std::collections::{BTreeMap, HashSet};

use rusqlite::Connection;

use crate::db::error::RepoError;
use crate::db::repositories::{issues_cache, jira_submissions, work_sessions, worklog_drafts};
use crate::domain::jira::JiraSubmissionState;
use crate::domain::work_session::{WorkSession, WorkSessionReviewStatus};
use crate::domain::worklog_draft::{CreateWorklogDraftInput, WorklogDraft, WorklogDraftStatus};

/// Jira не принимает worklog короче минуты и хранит время с точностью до
/// минуты — черновики сразу округляются, чтобы preview совпадал с тем, что
/// окажется в Jira (и чтобы remote-сверка по `timeSpentSeconds` работала).
pub const MIN_WORKLOG_SECONDS: i64 = 60;

/// Учитываемое время сессии: ручная корректировка важнее обнаруженного
/// (FR-04.3).
fn effective_seconds(session: &WorkSession) -> i64 {
    session.manual_seconds_override.unwrap_or(session.active_seconds)
}

pub fn round_to_minutes(seconds: i64) -> i64 {
    ((seconds + 30) / 60) * 60
}

/// FR-08.1/8.2: нейтральное описание только из подтверждённых данных —
/// без утверждений о том, что именно сделано.
pub fn default_comment(issue_key: &str) -> String {
    format!("Работа над задачей {issue_key}")
}

struct IssueGroup {
    session_ids: Vec<String>,
    started_at_utc: i64,
    total_seconds: i64,
}

/// Создаёт черновики за `local_day` (`[day_start_utc, day_end_utc)`) по
/// одному на issue key. Существующие черновики не трогает — правки
/// пользователя важнее пересчёта: группа пропускается, если на её issue
/// key уже есть черновик или хоть одна её сессия уже вошла в другой
/// черновик (пользователь мог сменить в нём ключ). Чтобы пересчитать —
/// удалить черновик и сгенерировать снова.
///
/// Не попадают: исключённые/удалённые сессии, нераспределённые (без issue
/// key — их сначала назначают в Timeline, NFR "не приписывать
/// нераспознанное время") и ещё открытая текущая сессия.
pub fn generate_for_day(conn: &Connection, local_day: &str, day_start_utc: i64, day_end_utc: i64) -> Result<Vec<WorklogDraft>, RepoError> {
    let sessions = work_sessions::list_by_range(conn, day_start_utc, day_end_utc, false)?;

    let mut groups: BTreeMap<String, IssueGroup> = BTreeMap::new();
    for s in sessions.iter().filter(|s| s.review_status != WorkSessionReviewStatus::Excluded && s.ended_at_utc.is_some()) {
        let Some(issue_key) = &s.issue_key else { continue };
        let seconds = effective_seconds(s);
        if seconds <= 0 {
            continue;
        }
        let group = groups.entry(issue_key.clone()).or_insert(IssueGroup { session_ids: Vec::new(), started_at_utc: s.started_at_utc, total_seconds: 0 });
        group.session_ids.push(s.id.clone());
        group.started_at_utc = group.started_at_utc.min(s.started_at_utc);
        group.total_seconds += seconds;
    }

    let existing = worklog_drafts::list_by_local_day(conn, local_day)?;
    let existing_keys: HashSet<&str> = existing.iter().map(|d| d.issue_key.as_str()).collect();
    let covered_sessions: HashSet<&str> = existing.iter().flat_map(|d| d.selected_session_ids.iter().map(String::as_str)).collect();

    for (issue_key, group) in &groups {
        if existing_keys.contains(issue_key.as_str()) || group.session_ids.iter().any(|id| covered_sessions.contains(id.as_str())) {
            continue;
        }
        let time_spent_seconds = round_to_minutes(group.total_seconds);
        if time_spent_seconds < MIN_WORKLOG_SECONDS {
            continue;
        }
        worklog_drafts::create(
            conn,
            &CreateWorklogDraftInput {
                local_day: local_day.to_string(),
                issue_key: issue_key.clone(),
                time_spent_seconds,
                started_at_utc: group.started_at_utc,
                comment: Some(default_comment(issue_key)),
                selected_session_ids: group.session_ids.clone(),
                status: None,
            },
        )?;
    }

    Ok(worklog_drafts::list_by_local_day(conn, local_day)?)
}

/// Черновик можно править/удалять, только пока он не отправлен и по нему
/// нет неразрешённой попытки (FR-07.6: правка после таймаута изменила бы
/// payload, и reconciliation сверял бы уже не то, что ушло в Jira).
pub fn ensure_editable(conn: &Connection, draft: &WorklogDraft) -> Result<(), String> {
    if draft.status == WorklogDraftStatus::Submitted {
        return Err("черновик уже отправлен в Jira — редактирование опубликованных записей пока не поддерживается (FR-07.9)".into());
    }
    let submissions = jira_submissions::list_by_local_draft(conn, &draft.id).map_err(|e| e.to_string())?;
    if submissions.iter().any(|s| matches!(s.state, JiraSubmissionState::Pending | JiraSubmissionState::Unknown)) {
        return Err("по черновику есть отправка с неизвестным результатом — сначала проверьте её в Jira".into());
    }
    Ok(())
}

/// Название задачи из локального кэша — офлайн-preview без сети.
pub fn cached_title(conn: &Connection, issue_key: &str) -> Option<String> {
    issues_cache::get(conn, issue_key).ok().flatten().map(|i| i.title)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::test_support::{build_sample_day_fixture, temp_database};
    use crate::domain::work_session::{CreateWorkSessionInput, WorkSessionSource};

    const DAY_START: i64 = 1_700_000_000;
    const DAY_END: i64 = DAY_START + 86_400;

    fn seed_sample_day(conn: &Connection) -> Vec<WorkSession> {
        build_sample_day_fixture(DAY_START, "UTC").sessions.iter().map(|s| work_sessions::create(conn, s).unwrap()).collect()
    }

    fn session(start_offset: i64, seconds: i64, issue_key: Option<&str>) -> CreateWorkSessionInput {
        CreateWorkSessionInput {
            started_at_utc: DAY_START + start_offset,
            ended_at_utc: Some(DAY_START + start_offset + seconds),
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
        }
    }

    #[test]
    fn round_to_minutes_rounds_half_up() {
        assert_eq!(round_to_minutes(29), 0);
        assert_eq!(round_to_minutes(30), 60);
        assert_eq!(round_to_minutes(5399), 5400);
    }

    #[test]
    fn sample_day_gives_one_draft_per_issue() {
        // E2E-сценарий 1 из ТЗ: 1:30 на OB-448 + 1:00 на OB-419, idle не входит.
        let (conn, _dir) = temp_database();
        seed_sample_day(&conn);

        let drafts = generate_for_day(&conn, "2023-11-14", DAY_START, DAY_END).unwrap();
        let by_key: BTreeMap<_, _> = drafts.iter().map(|d| (d.issue_key.as_str(), d)).collect();
        assert_eq!(by_key.len(), 2);
        assert_eq!(by_key["OB-448"].time_spent_seconds, 5400);
        assert_eq!(by_key["OB-419"].time_spent_seconds, 3600);
        assert_eq!(by_key["OB-448"].comment.as_deref(), Some("Работа над задачей OB-448"));
        assert_eq!(by_key["OB-448"].status, WorklogDraftStatus::Draft);
    }

    #[test]
    fn sessions_of_same_issue_are_summed_and_start_at_earliest() {
        let (conn, _dir) = temp_database();
        work_sessions::create(&conn, &session(3600, 1200, Some("OB-1"))).unwrap();
        work_sessions::create(&conn, &session(7200, 600, Some("OB-1"))).unwrap();

        let drafts = generate_for_day(&conn, "d", DAY_START, DAY_END).unwrap();
        assert_eq!(drafts.len(), 1);
        assert_eq!(drafts[0].time_spent_seconds, 1800);
        assert_eq!(drafts[0].started_at_utc, DAY_START + 3600);
        assert_eq!(drafts[0].selected_session_ids.len(), 2);
    }

    #[test]
    fn unassigned_excluded_open_and_too_short_sessions_are_skipped() {
        let (conn, _dir) = temp_database();
        work_sessions::create(&conn, &session(0, 3600, None)).unwrap();
        work_sessions::create(&conn, &session(3600, 20, Some("OB-2"))).unwrap();
        let mut open = session(7200, 600, Some("OB-3"));
        open.ended_at_utc = None;
        work_sessions::create(&conn, &open).unwrap();
        let mut excluded = session(9000, 600, Some("OB-4"));
        excluded.review_status = Some(WorkSessionReviewStatus::Excluded);
        work_sessions::create(&conn, &excluded).unwrap();

        assert!(generate_for_day(&conn, "d", DAY_START, DAY_END).unwrap().is_empty());
    }

    #[test]
    fn manual_seconds_override_wins_over_detected_time() {
        let (conn, _dir) = temp_database();
        let mut s = session(0, 3600, Some("OB-1"));
        s.manual_seconds_override = Some(1800);
        work_sessions::create(&conn, &s).unwrap();

        assert_eq!(generate_for_day(&conn, "d", DAY_START, DAY_END).unwrap()[0].time_spent_seconds, 1800);
    }

    #[test]
    fn regenerating_keeps_user_edits_and_does_not_duplicate() {
        let (conn, _dir) = temp_database();
        seed_sample_day(&conn);
        let first = generate_for_day(&conn, "d", DAY_START, DAY_END).unwrap();
        let ob448 = first.iter().find(|d| d.issue_key == "OB-448").unwrap();
        // Пользователь переназначил черновик на другую задачу и поправил время.
        worklog_drafts::update(
            &conn,
            &ob448.id,
            &crate::domain::worklog_draft::UpdateWorklogDraftPatch { issue_key: Some("OB-500".into()), time_spent_seconds: Some(600), ..Default::default() },
        )
        .unwrap();

        let second = generate_for_day(&conn, "d", DAY_START, DAY_END).unwrap();
        assert_eq!(second.len(), 2, "сессии OB-448 уже покрыты переназначенным черновиком");
        let edited = second.iter().find(|d| d.id == ob448.id).unwrap();
        assert_eq!(edited.issue_key, "OB-500");
        assert_eq!(edited.time_spent_seconds, 600);
    }

    #[test]
    fn ensure_editable_rejects_unknown_submission() {
        let (conn, _dir) = temp_database();
        seed_sample_day(&conn);
        let draft = generate_for_day(&conn, "d", DAY_START, DAY_END).unwrap().remove(0);
        assert!(ensure_editable(&conn, &draft).is_ok());

        let sub = jira_submissions::create_pending(&conn, &draft.id, &draft.issue_key, "h").unwrap();
        jira_submissions::resolve(&conn, &sub.id, JiraSubmissionState::Unknown, None).unwrap();
        assert!(ensure_editable(&conn, &draft).is_err());
    }
}
