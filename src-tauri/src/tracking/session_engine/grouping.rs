//! Чистая логика группировки сырых `ActivityEvent` в кандидаты рабочих
//! сессий (ТЗ FR-04.1, раздел 4). Никаких БД/системных вызовов времени —
//! принимает уже выбранные события и уже вычисленные границы полуночи,
//! поэтому полностью unit-тестируема без возни с часовыми поясами внутри
//! теста.

use crate::domain::activity_event::ActivityEvent;
use crate::domain::confidence::Confidence;
use crate::domain::tracking_status::TrackingStatus;
use crate::domain::work_session::WorkSessionReviewStatus;

/// Ключ группировки: "исключено" (не-whitelisted приложение, FR-02.3) —
/// отдельная группа независимо от project/issueKey; иначе — пара
/// (project, issueKey), где `None` для issueKey означает "Нераспределено"
/// в рамках того же проекта (или вовсе без проекта, если git context не
/// резолвился).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum RunKey {
    Excluded,
    Task { project_id: Option<String>, issue_key: Option<String> },
}

fn run_key(event: &ActivityEvent) -> RunKey {
    if event.app_category == "excluded" {
        RunKey::Excluded
    } else {
        RunKey::Task { project_id: event.project_id.clone(), issue_key: event.detected_issue_key.clone() }
    }
}

fn review_status_for(key: &RunKey) -> WorkSessionReviewStatus {
    match key {
        RunKey::Excluded => WorkSessionReviewStatus::Excluded,
        RunKey::Task { issue_key: Some(_), .. } => WorkSessionReviewStatus::Detected,
        RunKey::Task { issue_key: None, .. } => WorkSessionReviewStatus::Unassigned,
    }
}

fn weakest(confidences: &[Confidence]) -> Confidence {
    fn rank(c: Confidence) -> u8 {
        match c {
            Confidence::High => 2,
            Confidence::Medium => 1,
            Confidence::Low => 0,
        }
    }
    confidences.iter().copied().min_by_key(|c| rank(*c)).unwrap_or(Confidence::Low)
}

/// Кандидат рабочей сессии, построенный из одного "run" смежных событий.
#[derive(Debug, Clone, PartialEq)]
pub struct DetectedRun {
    pub started_at_utc: i64,
    pub ended_at_utc: i64,
    pub active_seconds: i64,
    pub project_id: Option<String>,
    pub issue_key: Option<String>,
    pub review_status: WorkSessionReviewStatus,
    pub confidence: Confidence,
}

fn finalize_run(events: &[&ActivityEvent], poll_interval_seconds: i64) -> DetectedRun {
    let key = run_key(events[0]);
    let started_at_utc = events[0].timestamp_utc;
    let ended_at_utc = events.last().unwrap().timestamp_utc + poll_interval_seconds;
    let (project_id, issue_key) = match &key {
        RunKey::Excluded => (None, None),
        RunKey::Task { project_id, issue_key } => (project_id.clone(), issue_key.clone()),
    };
    DetectedRun {
        started_at_utc,
        ended_at_utc,
        active_seconds: events.len() as i64 * poll_interval_seconds,
        project_id,
        issue_key,
        review_status: review_status_for(&key),
        confidence: weakest(&events.iter().map(|e| e.confidence).collect::<Vec<_>>()),
    }
}

/// `events` — уже отсортированы по времени по возрастанию (так их отдаёт
/// `activity_events::list_by_range`). `protected_ranges` — интервалы
/// `[start, end)`, которые уже заняты ручными/отредактированными сессиями
/// и должны быть исключены из группировки целиком (FR-04.6: не допускать
/// двойного учёта — если время уже занято, детектор туда не лезет).
/// `midnight_boundaries_utc` — отсортированные моменты локальной полуночи
/// в диапазоне (FR-04.4: разделять сессии на границе суток).
pub fn group_events_into_runs(
    events: &[ActivityEvent],
    poll_interval_seconds: i64,
    max_gap_seconds: i64,
    protected_ranges: &[(i64, i64)],
    midnight_boundaries_utc: &[i64],
) -> Vec<DetectedRun> {
    let is_protected = |ts: i64| protected_ranges.iter().any(|(s, e)| ts >= *s && ts < *e);
    let crosses_boundary = |from: i64, to: i64| midnight_boundaries_utc.iter().any(|&b| from < b && to >= b);

    let mut runs = Vec::new();
    let mut current: Vec<&ActivityEvent> = Vec::new();

    for event in events {
        if event.state != TrackingStatus::Tracking || is_protected(event.timestamp_utc) {
            if !current.is_empty() {
                runs.push(finalize_run(&current, poll_interval_seconds));
                current.clear();
            }
            continue;
        }

        if let Some(last) = current.last() {
            let gap = event.timestamp_utc - last.timestamp_utc;
            let same_key = run_key(last) == run_key(event);
            if !same_key || gap > max_gap_seconds || crosses_boundary(last.timestamp_utc, event.timestamp_utc) {
                runs.push(finalize_run(&current, poll_interval_seconds));
                current.clear();
            }
        }
        current.push(event);
    }
    if !current.is_empty() {
        runs.push(finalize_run(&current, poll_interval_seconds));
    }

    runs
}

#[cfg(test)]
mod tests {
    use super::*;

    fn event(ts: i64, state: TrackingStatus, project: Option<&str>, issue: Option<&str>, category: &str) -> ActivityEvent {
        ActivityEvent {
            id: format!("e{ts}"),
            timestamp_utc: ts,
            process_name_sanitized: "webstorm64.exe".to_string(),
            app_category: category.to_string(),
            project_id: project.map(str::to_string),
            branch: None,
            detected_issue_key: issue.map(str::to_string),
            idle_seconds: Some(0),
            state,
            confidence: Confidence::High,
            reason: "test".to_string(),
            created_at_utc: ts,
        }
    }

    const POLL: i64 = 5;
    const MAX_GAP: i64 = POLL * 3;
    const DAY_START: i64 = 1_700_000_000;

    #[test]
    fn merges_consecutive_events_with_the_same_task_into_one_run() {
        let events = vec![
            event(DAY_START, TrackingStatus::Tracking, Some("p1"), Some("OB-448"), "ide"),
            event(DAY_START + 5, TrackingStatus::Tracking, Some("p1"), Some("OB-448"), "ide"),
            event(DAY_START + 10, TrackingStatus::Tracking, Some("p1"), Some("OB-448"), "ide"),
        ];
        let runs = group_events_into_runs(&events, POLL, MAX_GAP, &[], &[]);
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].started_at_utc, DAY_START);
        assert_eq!(runs[0].ended_at_utc, DAY_START + 10 + POLL);
        assert_eq!(runs[0].active_seconds, 15);
        assert_eq!(runs[0].issue_key.as_deref(), Some("OB-448"));
        assert_eq!(runs[0].review_status, WorkSessionReviewStatus::Detected);
    }

    #[test]
    fn idle_gap_between_splits_into_two_runs_scenario_work_lunch_work() {
        // Работа 09:00–09:10, idle 09:10-11:00 (не в events вовсе — не Tracking,
        // отфильтровывается), снова работа 11:00-11:10.
        let events = vec![
            event(DAY_START, TrackingStatus::Tracking, Some("p1"), Some("OB-448"), "ide"),
            event(DAY_START + 5, TrackingStatus::Tracking, Some("p1"), Some("OB-448"), "ide"),
            event(DAY_START + 3600, TrackingStatus::Idle, None, None, "excluded"),
            event(DAY_START + 7200, TrackingStatus::Tracking, Some("p1"), Some("OB-448"), "ide"),
        ];
        let runs = group_events_into_runs(&events, POLL, MAX_GAP, &[], &[]);
        assert_eq!(runs.len(), 2);
        assert_eq!(runs[0].active_seconds, 10);
        assert_eq!(runs[1].started_at_utc, DAY_START + 7200);
    }

    #[test]
    fn task_switch_without_gap_still_splits_into_two_runs() {
        let events = vec![
            event(DAY_START, TrackingStatus::Tracking, Some("p1"), Some("OB-448"), "ide"),
            event(DAY_START + 5, TrackingStatus::Tracking, Some("p1"), Some("OB-419"), "ide"),
        ];
        let runs = group_events_into_runs(&events, POLL, MAX_GAP, &[], &[]);
        assert_eq!(runs.len(), 2);
        assert_eq!(runs[0].issue_key.as_deref(), Some("OB-448"));
        assert_eq!(runs[1].issue_key.as_deref(), Some("OB-419"));
    }

    #[test]
    fn two_projects_produce_separate_runs_even_with_same_issue_key_missing() {
        let events = vec![
            event(DAY_START, TrackingStatus::Tracking, Some("p1"), None, "ide"),
            event(DAY_START + 5, TrackingStatus::Tracking, Some("p2"), None, "ide"),
        ];
        let runs = group_events_into_runs(&events, POLL, MAX_GAP, &[], &[]);
        assert_eq!(runs.len(), 2);
        assert_eq!(runs[0].project_id.as_deref(), Some("p1"));
        assert_eq!(runs[1].project_id.as_deref(), Some("p2"));
        assert!(runs.iter().all(|r| r.review_status == WorkSessionReviewStatus::Unassigned));
    }

    #[test]
    fn excluded_category_never_gets_an_issue_key_even_if_one_was_detected() {
        // Невероятный, но возможный случай: git-контекст резолвился, но
        // категория всё равно excluded (см. tracker.rs) — не должно
        // попасть в Detected.
        let events = vec![event(DAY_START, TrackingStatus::Tracking, Some("p1"), Some("OB-448"), "excluded")];
        let runs = group_events_into_runs(&events, POLL, MAX_GAP, &[], &[]);
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].issue_key, None);
        assert_eq!(runs[0].review_status, WorkSessionReviewStatus::Excluded);
    }

    #[test]
    fn gap_larger_than_tolerance_splits_even_with_same_key_simulated_sleep() {
        let events = vec![
            event(DAY_START, TrackingStatus::Tracking, Some("p1"), Some("OB-448"), "ide"),
            // Разрыв намного больше max_gap — симулирует сон между событиями
            // (сам SUSPENDED-эвент тоже был бы не-Tracking и отфильтровался
            // бы, но даже будь он таким же Tracking, с таким разрывом это не
            // одна непрерывная сессия).
            event(DAY_START + MAX_GAP + 100, TrackingStatus::Tracking, Some("p1"), Some("OB-448"), "ide"),
        ];
        let runs = group_events_into_runs(&events, POLL, MAX_GAP, &[], &[]);
        assert_eq!(runs.len(), 2);
    }

    #[test]
    fn protected_ranges_are_excluded_from_grouping_entirely() {
        let events = vec![
            event(DAY_START, TrackingStatus::Tracking, Some("p1"), Some("OB-448"), "ide"),
            event(DAY_START + 5, TrackingStatus::Tracking, Some("p1"), Some("OB-448"), "ide"), // protected
            event(DAY_START + 10, TrackingStatus::Tracking, Some("p1"), Some("OB-448"), "ide"),
        ];
        let protected = [(DAY_START + 5, DAY_START + 6)];
        let runs = group_events_into_runs(&events, POLL, MAX_GAP, &protected, &[]);
        // Защищённое событие выбыло из середины — разрыв разбивает run на два.
        assert_eq!(runs.len(), 2);
        assert_eq!(runs[0].active_seconds, 5);
        assert_eq!(runs[1].started_at_utc, DAY_START + 10);
    }

    #[test]
    fn midnight_boundary_splits_run_even_with_no_gap_and_same_key() {
        let midnight = DAY_START + 7;
        let events = vec![
            event(DAY_START, TrackingStatus::Tracking, Some("p1"), Some("OB-448"), "ide"),
            event(DAY_START + 5, TrackingStatus::Tracking, Some("p1"), Some("OB-448"), "ide"),
            event(DAY_START + 10, TrackingStatus::Tracking, Some("p1"), Some("OB-448"), "ide"), // после полуночи
        ];
        let runs = group_events_into_runs(&events, POLL, MAX_GAP, &[], &[midnight]);
        assert_eq!(runs.len(), 2);
        assert_eq!(runs[0].ended_at_utc, DAY_START + 5 + POLL);
        assert_eq!(runs[1].started_at_utc, DAY_START + 10);
    }

    #[test]
    fn empty_events_produce_no_runs() {
        assert_eq!(group_events_into_runs(&[], POLL, MAX_GAP, &[], &[]), vec![]);
    }

    #[test]
    fn confidence_of_run_is_the_weakest_among_its_events() {
        let mut e1 = event(DAY_START, TrackingStatus::Tracking, Some("p1"), Some("OB-448"), "ide");
        e1.confidence = Confidence::High;
        let mut e2 = event(DAY_START + 5, TrackingStatus::Tracking, Some("p1"), Some("OB-448"), "ide");
        e2.confidence = Confidence::Low;
        let runs = group_events_into_runs(&[e1, e2], POLL, MAX_GAP, &[], &[]);
        assert_eq!(runs[0].confidence, Confidence::Low);
    }
}
