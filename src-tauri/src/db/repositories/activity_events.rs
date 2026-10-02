use rusqlite::{params, Connection, Result, Row};

use crate::db::ids::{generate_id, now_utc_seconds};
use crate::domain::activity_event::{ActivityEvent, CreateActivityEventInput};
use crate::domain::confidence::Confidence;
use crate::domain::tracking_status::TrackingStatus;

fn row_to_event(row: &Row) -> rusqlite::Result<ActivityEvent> {
    let state_str: String = row.get("state")?;
    let confidence_str: String = row.get("confidence")?;
    Ok(ActivityEvent {
        id: row.get("id")?,
        timestamp_utc: row.get("timestampUtc")?,
        process_name_sanitized: row.get("processNameSanitized")?,
        app_category: row.get("appCategory")?,
        project_id: row.get("projectId")?,
        branch: row.get("branch")?,
        detected_issue_key: row.get("detectedIssueKey")?,
        idle_seconds: row.get("idleSeconds")?,
        state: TrackingStatus::from_db_str(&state_str).expect("invalid state stored in activity_events"),
        confidence: Confidence::from_db_str(&confidence_str).expect("invalid confidence stored in activity_events"),
        reason: row.get("reason")?,
        created_at_utc: row.get("createdAtUtc")?,
    })
}

/// Сырой, неизменяемый слой наблюдений (FR-02, FR-04.2). Только `insert` +
/// чтение — здесь намеренно нет `update`/`delete`: правки происходят
/// только на уровне `work_sessions`, первичная история событий не трогается.
pub fn insert(conn: &Connection, input: &CreateActivityEventInput) -> Result<ActivityEvent> {
    let id = generate_id();
    let created_at_utc = now_utc_seconds();
    conn.execute(
        "INSERT INTO activity_events
           (id, timestampUtc, processNameSanitized, appCategory, projectId, branch,
            detectedIssueKey, idleSeconds, state, confidence, reason, createdAtUtc)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
        params![
            id,
            input.timestamp_utc,
            input.process_name_sanitized,
            input.app_category,
            input.project_id,
            input.branch,
            input.detected_issue_key,
            input.idle_seconds,
            input.state.as_db_str(),
            input.confidence.as_db_str(),
            input.reason,
            created_at_utc,
        ],
    )?;
    Ok(ActivityEvent {
        id,
        timestamp_utc: input.timestamp_utc,
        process_name_sanitized: input.process_name_sanitized.clone(),
        app_category: input.app_category.clone(),
        project_id: input.project_id.clone(),
        branch: input.branch.clone(),
        detected_issue_key: input.detected_issue_key.clone(),
        idle_seconds: input.idle_seconds,
        state: input.state,
        confidence: input.confidence,
        reason: input.reason.clone(),
        created_at_utc,
    })
}

/// Для сидирования фикстур в тестах — одна транзакция на весь набор.
pub fn insert_many(conn: &mut Connection, inputs: &[CreateActivityEventInput]) -> Result<Vec<ActivityEvent>> {
    let tx = conn.transaction()?;
    let mut result = Vec::with_capacity(inputs.len());
    for input in inputs {
        result.push(insert(&tx, input)?);
    }
    tx.commit()?;
    Ok(result)
}

pub fn list_by_range(conn: &Connection, start_utc: i64, end_utc_exclusive: i64) -> Result<Vec<ActivityEvent>> {
    let mut stmt =
        conn.prepare("SELECT * FROM activity_events WHERE timestampUtc >= ?1 AND timestampUtc < ?2 ORDER BY timestampUtc")?;
    let rows = stmt.query_map(params![start_utc, end_utc_exclusive], row_to_event)?;
    rows.collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::test_support::{build_sample_day_fixture, temp_database};

    const DAY_START: i64 = 1_700_000_000;

    #[test]
    fn insert_returns_event_with_id_and_created_at() {
        let (conn, _dir) = temp_database();
        let event = insert(
            &conn,
            &CreateActivityEventInput {
                timestamp_utc: DAY_START,
                process_name_sanitized: "webstorm64.exe".into(),
                app_category: "ide".into(),
                project_id: None,
                branch: Some("main".into()),
                detected_issue_key: None,
                idle_seconds: Some(0),
                state: TrackingStatus::Tracking,
                confidence: Confidence::Low,
                reason: "no issue key in branch".into(),
            },
        )
        .unwrap();
        assert!(!event.id.is_empty());
        assert!(event.created_at_utc > 0);
    }

    #[test]
    fn fixture_seeds_in_one_transaction_and_reads_back_by_range() {
        let (mut conn, _dir) = temp_database();
        let fixture = build_sample_day_fixture(DAY_START, "UTC");
        let inserted = insert_many(&mut conn, &fixture.events).unwrap();
        assert_eq!(inserted.len(), fixture.events.len());

        let all = list_by_range(&conn, DAY_START, DAY_START + 24 * 3600).unwrap();
        assert_eq!(all.len(), fixture.events.len());

        // idle-период 10:30–11:00 должен быть представлен как отдельные
        // IDLE-события, не как TRACKING (иначе бездействие молча
        // засчиталось бы работой).
        let idle_events: Vec<_> = all.iter().filter(|e| e.state == TrackingStatus::Idle).collect();
        assert!(!idle_events.is_empty());
        assert!(idle_events.iter().all(|e| e.detected_issue_key.is_none()));
    }

    #[test]
    fn list_by_range_excludes_right_boundary() {
        let (conn, _dir) = temp_database();
        insert(
            &conn,
            &CreateActivityEventInput {
                timestamp_utc: DAY_START + 100,
                process_name_sanitized: "x".into(),
                app_category: "ide".into(),
                project_id: None,
                branch: None,
                detected_issue_key: None,
                idle_seconds: None,
                state: TrackingStatus::Tracking,
                confidence: Confidence::High,
                reason: "test".into(),
            },
        )
        .unwrap();
        assert_eq!(list_by_range(&conn, DAY_START, DAY_START + 100).unwrap().len(), 0);
        assert_eq!(list_by_range(&conn, DAY_START, DAY_START + 101).unwrap().len(), 1);
    }
}
