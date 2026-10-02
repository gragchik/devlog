use rusqlite::Connection;
use tempfile::TempDir;

use super::database::create_database;
use crate::domain::activity_event::CreateActivityEventInput;
use crate::domain::confidence::Confidence;
use crate::domain::tracking_status::TrackingStatus;
use crate::domain::work_session::{CreateWorkSessionInput, WorkSessionReviewStatus, WorkSessionSource};

/// Реальный файл на диске (не `:memory:`) — нужен тестам на "перезапуск не
/// теряет записи" (закрыть одно соединение, открыть новое на том же пути).
/// `TempDir` нужно держать в скоупе теста — при его Drop директория
/// удаляется.
pub fn temp_database() -> (Connection, TempDir) {
    let dir = TempDir::new().expect("create temp dir");
    let path = dir.path().join("test.sqlite3");
    let conn = create_database(&path).expect("create test database");
    (conn, dir)
}

const HOUR: i64 = 3600;
const POLL_STEP_SECONDS: i64 = 300; // 5 минут — целевой интервал опроса (FR-02.1)

pub struct SampleDayFixture {
    pub events: Vec<CreateActivityEventInput>,
    // Пока используется только `events` (Итерация 1 тестирует только
    // activity_events-слой) — `sessions` предназначен для Session Engine
    // тестов в Итерации 4, где фикстура и будет реально проверяться на
    // равенство с результатом группировки событий.
    #[allow(dead_code)]
    pub sessions: Vec<CreateWorkSessionInput>,
}

struct RangeSpec {
    from_offset: i64,
    to_offset: i64,
    state: TrackingStatus,
    issue_key: Option<&'static str>,
    branch: Option<&'static str>,
}

fn build_events_for_range(start_of_day_utc: i64, spec: &RangeSpec) -> Vec<CreateActivityEventInput> {
    let mut events = Vec::new();
    let mut offset = spec.from_offset;
    while offset < spec.to_offset {
        events.push(CreateActivityEventInput {
            timestamp_utc: start_of_day_utc + offset,
            process_name_sanitized: "webstorm64.exe".to_string(),
            app_category: "ide".to_string(),
            project_id: None,
            branch: spec.branch.map(str::to_string),
            detected_issue_key: spec.issue_key.map(str::to_string),
            idle_seconds: if spec.state == TrackingStatus::Idle { Some(POLL_STEP_SECONDS) } else { Some(0) },
            state: spec.state,
            confidence: if spec.issue_key.is_some() { Confidence::High } else { Confidence::Low },
            reason: if let Some(key) = spec.issue_key {
                format!("branch regex match: {key}")
            } else {
                "idle timeout exceeded".to_string()
            },
        });
        offset += POLL_STEP_SECONDS;
    }
    events
}

/// Сценарий 1 из ТЗ (раздел 9, E2E): "Работа 09:00–10:30 OB-448, 10:30–11:00
/// idle, 11:00–12:00 OB-419 → 1:30 + 1:00, idle не входит". Переиспользуется
/// здесь (Итерация 1) и будет переиспользован Session Engine тестами в
/// Итерации 4.
pub fn build_sample_day_fixture(start_of_day_utc: i64, timezone_id: &str) -> SampleDayFixture {
    let ranges = [
        RangeSpec { from_offset: 9 * HOUR, to_offset: (10.5 * HOUR as f64) as i64, state: TrackingStatus::Tracking, issue_key: Some("OB-448"), branch: Some("feature/OB-448-mass-payments") },
        RangeSpec { from_offset: (10.5 * HOUR as f64) as i64, to_offset: 11 * HOUR, state: TrackingStatus::Idle, issue_key: None, branch: None },
        RangeSpec { from_offset: 11 * HOUR, to_offset: 12 * HOUR, state: TrackingStatus::Tracking, issue_key: Some("OB-419"), branch: Some("feature/OB-419-refunds") },
    ];

    let events = ranges.iter().flat_map(|r| build_events_for_range(start_of_day_utc, r)).collect();

    let sessions = vec![
        CreateWorkSessionInput {
            started_at_utc: start_of_day_utc + 9 * HOUR,
            ended_at_utc: Some(start_of_day_utc + (10.5 * HOUR as f64) as i64),
            timezone_id: timezone_id.to_string(),
            active_seconds: (1.5 * HOUR as f64) as i64,
            source: WorkSessionSource::Detected,
            project_id: None,
            issue_key: Some("OB-448".to_string()),
            manual_seconds_override: None,
            description: None,
            confidence: Some(Confidence::High),
            review_status: Some(WorkSessionReviewStatus::Detected),
            is_manually_edited: None,
        },
        CreateWorkSessionInput {
            started_at_utc: start_of_day_utc + 11 * HOUR,
            ended_at_utc: Some(start_of_day_utc + 12 * HOUR),
            timezone_id: timezone_id.to_string(),
            active_seconds: HOUR,
            source: WorkSessionSource::Detected,
            project_id: None,
            issue_key: Some("OB-419".to_string()),
            manual_seconds_override: None,
            description: None,
            confidence: Some(Confidence::High),
            review_status: Some(WorkSessionReviewStatus::Detected),
            is_manually_edited: None,
        },
    ];

    SampleDayFixture { events, sessions }
}
