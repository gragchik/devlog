use rusqlite::{params, Connection, OptionalExtension, Result, Row};

use crate::db::ids::{generate_id, now_utc_seconds};
use crate::domain::worklog_draft::{CreateWorklogDraftInput, UpdateWorklogDraftPatch, WorklogDraft, WorklogDraftStatus};

fn row_to_draft(row: &Row) -> rusqlite::Result<WorklogDraft> {
    let status_str: String = row.get("status")?;
    let selected_ids_json: String = row.get("selectedSessionIdsJson")?;
    Ok(WorklogDraft {
        id: row.get("id")?,
        local_day: row.get("localDay")?,
        issue_key: row.get("issueKey")?,
        time_spent_seconds: row.get("timeSpentSeconds")?,
        started_at_utc: row.get("startedAtUtc")?,
        comment: row.get("comment")?,
        selected_session_ids: serde_json::from_str(&selected_ids_json).unwrap_or_default(),
        status: WorklogDraftStatus::from_db_str(&status_str).expect("invalid status stored in worklog_drafts"),
        updated_at_utc: row.get("updatedAtUtc")?,
    })
}

/// CRUD-слой для `worklog_drafts` (FR-08, раздел 5). UI составления
/// отчёта (Worklog Review) — Итерация 5/8, здесь только хранение.
pub fn create(conn: &Connection, input: &CreateWorklogDraftInput) -> Result<WorklogDraft> {
    let id = generate_id();
    let updated_at_utc = now_utc_seconds();
    let status = input.status.unwrap_or(WorklogDraftStatus::Draft);
    conn.execute(
        "INSERT INTO worklog_drafts
           (id, localDay, issueKey, timeSpentSeconds, startedAtUtc, comment, selectedSessionIdsJson, status, updatedAtUtc)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        params![
            id,
            input.local_day,
            input.issue_key,
            input.time_spent_seconds,
            input.started_at_utc,
            input.comment,
            serde_json::to_string(&input.selected_session_ids).expect("Vec<String> must serialize"),
            status.as_db_str(),
            updated_at_utc,
        ],
    )?;
    Ok(get_by_id(conn, &id)?.expect("just inserted"))
}

pub fn get_by_id(conn: &Connection, id: &str) -> Result<Option<WorklogDraft>> {
    conn.query_row("SELECT * FROM worklog_drafts WHERE id = ?1", params![id], row_to_draft)
        .optional()
}

pub fn list_by_local_day(conn: &Connection, local_day: &str) -> Result<Vec<WorklogDraft>> {
    let mut stmt = conn.prepare("SELECT * FROM worklog_drafts WHERE localDay = ?1 ORDER BY startedAtUtc")?;
    let rows = stmt.query_map(params![local_day], row_to_draft)?;
    rows.collect()
}

pub fn update(conn: &Connection, id: &str, patch: &UpdateWorklogDraftPatch) -> Result<WorklogDraft> {
    let existing = get_by_id(conn, id)?.ok_or(rusqlite::Error::QueryReturnedNoRows)?;
    let issue_key = patch.issue_key.clone().unwrap_or(existing.issue_key);
    let time_spent_seconds = patch.time_spent_seconds.unwrap_or(existing.time_spent_seconds);
    let started_at_utc = patch.started_at_utc.unwrap_or(existing.started_at_utc);
    let comment = patch.comment.clone().unwrap_or(existing.comment);
    let selected_session_ids = patch.selected_session_ids.clone().unwrap_or(existing.selected_session_ids);
    let status = patch.status.unwrap_or(existing.status);
    let updated_at_utc = now_utc_seconds();

    conn.execute(
        "UPDATE worklog_drafts SET
           issueKey = ?1, timeSpentSeconds = ?2, startedAtUtc = ?3, comment = ?4,
           selectedSessionIdsJson = ?5, status = ?6, updatedAtUtc = ?7
         WHERE id = ?8",
        params![
            issue_key,
            time_spent_seconds,
            started_at_utc,
            comment,
            serde_json::to_string(&selected_session_ids).expect("Vec<String> must serialize"),
            status.as_db_str(),
            updated_at_utc,
            id,
        ],
    )?;
    Ok(get_by_id(conn, id)?.expect("just updated"))
}

pub fn remove(conn: &Connection, id: &str) -> Result<()> {
    conn.execute("DELETE FROM worklog_drafts WHERE id = ?1", params![id])?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::test_support::temp_database;

    fn minimal_input(local_day: &str, issue_key: &str, started_at_utc: i64) -> CreateWorklogDraftInput {
        CreateWorklogDraftInput {
            local_day: local_day.into(),
            issue_key: issue_key.into(),
            time_spent_seconds: 100,
            started_at_utc,
            comment: None,
            selected_session_ids: vec!["s1".into()],
            status: None,
        }
    }

    #[test]
    fn creates_draft_with_default_status() {
        let (conn, _dir) = temp_database();
        let draft = create(&conn, &minimal_input("2026-10-01", "OB-448", 1_700_000_000)).unwrap();
        assert_eq!(draft.status, WorklogDraftStatus::Draft);
    }

    #[test]
    fn list_by_local_day_filters_correctly() {
        let (conn, _dir) = temp_database();
        create(&conn, &minimal_input("2026-10-01", "OB-448", 1)).unwrap();
        create(&conn, &minimal_input("2026-10-02", "OB-419", 2)).unwrap();
        let day1 = list_by_local_day(&conn, "2026-10-01").unwrap();
        assert_eq!(day1.len(), 1);
        assert_eq!(day1[0].issue_key, "OB-448");
    }

    #[test]
    fn update_changes_comment_and_sessions_keeping_the_rest() {
        let (conn, _dir) = temp_database();
        let draft = create(&conn, &minimal_input("2026-10-01", "OB-448", 1)).unwrap();
        let updated = update(
            &conn,
            &draft.id,
            &UpdateWorklogDraftPatch { comment: Some(Some("готово".into())), selected_session_ids: Some(vec!["s1".into(), "s2".into()]), ..Default::default() },
        )
        .unwrap();
        assert_eq!(updated.comment.as_deref(), Some("готово"));
        assert_eq!(updated.selected_session_ids, vec!["s1".to_string(), "s2".to_string()]);
        assert_eq!(updated.issue_key, "OB-448");
    }

    #[test]
    fn remove_deletes_draft() {
        let (conn, _dir) = temp_database();
        let draft = create(&conn, &minimal_input("2026-10-01", "OB-448", 1)).unwrap();
        remove(&conn, &draft.id).unwrap();
        assert_eq!(get_by_id(&conn, &draft.id).unwrap(), None);
    }
}
