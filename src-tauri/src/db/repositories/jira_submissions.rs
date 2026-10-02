use rusqlite::{params, Connection, OptionalExtension, Result, Row};

use crate::db::ids::{generate_id, now_utc_seconds};
use crate::domain::jira::{JiraSubmission, JiraSubmissionState};

fn row_to_submission(row: &Row) -> rusqlite::Result<JiraSubmission> {
    let state_str: String = row.get("state")?;
    Ok(JiraSubmission {
        id: row.get("id")?,
        local_draft_id: row.get("localDraftId")?,
        issue_key: row.get("issueKey")?,
        payload_hash: row.get("payloadHash")?,
        remote_worklog_id: row.get("remoteWorklogId")?,
        state: JiraSubmissionState::from_db_str(&state_str).expect("invalid state stored in jira_submissions"),
        attempted_at_utc: row.get("attemptedAtUtc")?,
        resolved_at_utc: row.get("resolvedAtUtc")?,
    })
}

/// Журнал попыток отправки worklog в Jira (ТЗ раздел 5 "jira_submissions",
/// FR-07.6/7.7). Пишется ДО сетевого запроса (state=pending), чтобы
/// `payload_hash` уже был виден для проверки дублей даже если процесс упадёт
/// посреди запроса — после рестарта `pending`-записи reconciliation-логика
/// должна разобрать явно, а не тихо повторить.
pub fn create_pending(conn: &Connection, local_draft_id: &str, issue_key: &str, payload_hash: &str) -> Result<JiraSubmission> {
    let id = generate_id();
    conn.execute(
        "INSERT INTO jira_submissions (id, localDraftId, issueKey, payloadHash, remoteWorklogId, state, attemptedAtUtc, resolvedAtUtc)
         VALUES (?1, ?2, ?3, ?4, NULL, ?5, ?6, NULL)",
        params![id, local_draft_id, issue_key, payload_hash, JiraSubmissionState::Pending.as_db_str(), now_utc_seconds()],
    )?;
    Ok(get_by_id(conn, &id)?.expect("just inserted"))
}

pub fn get_by_id(conn: &Connection, id: &str) -> Result<Option<JiraSubmission>> {
    conn.query_row("SELECT * FROM jira_submissions WHERE id = ?1", params![id], row_to_submission).optional()
}

/// Переводит попытку в финальное/промежуточное состояние по итогу сетевого
/// запроса. `remote_worklog_id` заполняется только при `Posted`.
pub fn resolve(conn: &Connection, id: &str, state: JiraSubmissionState, remote_worklog_id: Option<&str>) -> Result<JiraSubmission> {
    conn.execute(
        "UPDATE jira_submissions SET state = ?1, remoteWorklogId = ?2, resolvedAtUtc = ?3 WHERE id = ?4",
        params![state.as_db_str(), remote_worklog_id, now_utc_seconds(), id],
    )?;
    Ok(get_by_id(conn, id)?.expect("just updated"))
}

/// FR-07.6/7.7: уже есть успешно отправленный (или ещё не разрешённый —
/// `Unknown` после таймаута считается "возможно уже отправлено", см.
/// reconciliation) payload с таким же хэшем — не даём слепо повторить.
pub fn find_non_failed_by_payload_hash(conn: &Connection, payload_hash: &str) -> Result<Option<JiraSubmission>> {
    conn.query_row(
        "SELECT * FROM jira_submissions WHERE payloadHash = ?1 AND state != ?2 ORDER BY attemptedAtUtc DESC LIMIT 1",
        params![payload_hash, JiraSubmissionState::Failed.as_db_str()],
        row_to_submission,
    )
    .optional()
}

/// Попытки, оставшиеся в `Unknown` (сеть оборвалась, POST мог уйти) —
/// кандидаты для ручного reconciliation-UI (FR-07.6: "не повторять
/// автоматически").
pub fn list_unknown(conn: &Connection) -> Result<Vec<JiraSubmission>> {
    let mut stmt = conn.prepare("SELECT * FROM jira_submissions WHERE state = ?1 ORDER BY attemptedAtUtc DESC")?;
    let rows = stmt.query_map(params![JiraSubmissionState::Unknown.as_db_str()], row_to_submission)?;
    rows.collect()
}

pub fn list_by_local_draft(conn: &Connection, local_draft_id: &str) -> Result<Vec<JiraSubmission>> {
    let mut stmt = conn.prepare("SELECT * FROM jira_submissions WHERE localDraftId = ?1 ORDER BY attemptedAtUtc DESC")?;
    let rows = stmt.query_map(params![local_draft_id], row_to_submission)?;
    rows.collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::test_support::temp_database;

    #[test]
    fn create_pending_then_get_round_trips() {
        let (conn, _dir) = temp_database();
        let sub = create_pending(&conn, "draft-1", "OB-448", "hash-1").unwrap();
        assert_eq!(sub.state, JiraSubmissionState::Pending);
        assert_eq!(sub.remote_worklog_id, None);
    }

    #[test]
    fn resolve_to_posted_sets_remote_id_and_resolved_time() {
        let (conn, _dir) = temp_database();
        let sub = create_pending(&conn, "draft-1", "OB-448", "hash-1").unwrap();
        let resolved = resolve(&conn, &sub.id, JiraSubmissionState::Posted, Some("10042")).unwrap();
        assert_eq!(resolved.state, JiraSubmissionState::Posted);
        assert_eq!(resolved.remote_worklog_id.as_deref(), Some("10042"));
        assert!(resolved.resolved_at_utc.is_some());
    }

    #[test]
    fn find_non_failed_by_payload_hash_ignores_failed_attempts() {
        let (conn, _dir) = temp_database();
        let sub = create_pending(&conn, "draft-1", "OB-448", "hash-1").unwrap();
        resolve(&conn, &sub.id, JiraSubmissionState::Failed, None).unwrap();
        assert_eq!(find_non_failed_by_payload_hash(&conn, "hash-1").unwrap(), None);
    }

    #[test]
    fn find_non_failed_by_payload_hash_finds_posted_attempt() {
        let (conn, _dir) = temp_database();
        let sub = create_pending(&conn, "draft-1", "OB-448", "hash-1").unwrap();
        resolve(&conn, &sub.id, JiraSubmissionState::Posted, Some("10042")).unwrap();
        let found = find_non_failed_by_payload_hash(&conn, "hash-1").unwrap().unwrap();
        assert_eq!(found.state, JiraSubmissionState::Posted);
    }

    #[test]
    fn list_unknown_returns_only_unknown_state() {
        let (conn, _dir) = temp_database();
        let a = create_pending(&conn, "draft-1", "OB-448", "hash-1").unwrap();
        let b = create_pending(&conn, "draft-2", "OB-419", "hash-2").unwrap();
        resolve(&conn, &a.id, JiraSubmissionState::Unknown, None).unwrap();
        resolve(&conn, &b.id, JiraSubmissionState::Posted, Some("1")).unwrap();
        let unknowns = list_unknown(&conn).unwrap();
        assert_eq!(unknowns.len(), 1);
        assert_eq!(unknowns[0].id, a.id);
    }

    #[test]
    fn list_by_local_draft_filters_correctly() {
        let (conn, _dir) = temp_database();
        create_pending(&conn, "draft-1", "OB-448", "hash-1").unwrap();
        create_pending(&conn, "draft-2", "OB-419", "hash-2").unwrap();
        let for_draft1 = list_by_local_draft(&conn, "draft-1").unwrap();
        assert_eq!(for_draft1.len(), 1);
        assert_eq!(for_draft1[0].issue_key, "OB-448");
    }
}
