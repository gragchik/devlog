use rusqlite::{params, Connection, OptionalExtension, Result, Row};

use crate::db::ids::{generate_id, now_utc_seconds};
use crate::domain::session_edit::{CreateSessionEditInput, SessionEdit};
use crate::domain::work_session::{SessionEditOperation, WorkSession};

fn row_to_edit(row: &Row) -> rusqlite::Result<SessionEdit> {
    let operation_str: String = row.get("operation")?;
    let previous_json: String = row.get("previousStateJson")?;
    let next_json: String = row.get("nextStateJson")?;
    Ok(SessionEdit {
        id: row.get("id")?,
        session_id: row.get("sessionId")?,
        edited_at_utc: row.get("editedAtUtc")?,
        operation: SessionEditOperation::from_db_str(&operation_str).expect("invalid operation stored in session_edits"),
        previous_state: serde_json::from_str::<WorkSession>(&previous_json).expect("corrupt previousStateJson"),
        next_state: serde_json::from_str::<WorkSession>(&next_json).expect("corrupt nextStateJson"),
    })
}

/// Insert-only журнал правок (FR-04.2/FR-04.8). Вызывается изнутри
/// `work_sessions::update`/`soft_delete`/`restore`/`undo_last_edit` — в
/// одной транзакции с изменением самой строки `work_sessions`.
pub fn append(conn: &Connection, input: &CreateSessionEditInput) -> Result<SessionEdit> {
    let id = generate_id();
    let edited_at_utc = now_utc_seconds();
    conn.execute(
        "INSERT INTO session_edits (id, sessionId, editedAtUtc, operation, previousStateJson, nextStateJson)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![
            id,
            input.session_id,
            edited_at_utc,
            input.operation.as_db_str(),
            serde_json::to_string(&input.previous_state).expect("WorkSession must serialize"),
            serde_json::to_string(&input.next_state).expect("WorkSession must serialize"),
        ],
    )?;
    Ok(SessionEdit {
        id,
        session_id: input.session_id.clone(),
        edited_at_utc,
        operation: input.operation,
        previous_state: input.previous_state.clone(),
        next_state: input.next_state.clone(),
    })
}

/// Пока не выведено ни в одну IPC-команду (нет UI "история правок этой
/// сессии" в Итерации 5) — используется напрямую тестами.
#[allow(dead_code)]
pub fn list_by_session(conn: &Connection, session_id: &str) -> Result<Vec<SessionEdit>> {
    let mut stmt = conn.prepare("SELECT * FROM session_edits WHERE sessionId = ?1 ORDER BY editedAtUtc, rowid")?;
    let rows = stmt.query_map(params![session_id], row_to_edit)?;
    rows.collect()
}

pub fn get_latest_for_session(conn: &Connection, session_id: &str) -> Result<Option<SessionEdit>> {
    conn.query_row(
        "SELECT * FROM session_edits WHERE sessionId = ?1 ORDER BY editedAtUtc DESC, rowid DESC LIMIT 1",
        params![session_id],
        row_to_edit,
    )
    .optional()
}

/// FR-04.8: "журнал изменений, минимум для последних 30 дней" — удаляет
/// записи старше `older_than_utc`, но вызывающий обязан сам выбрать порог
/// так, чтобы оставались как минимум 30 дней истории (см. вызов в
/// `app_database.rs`).
pub fn prune_older_than(conn: &Connection, older_than_utc: i64) -> Result<usize> {
    conn.execute("DELETE FROM session_edits WHERE editedAtUtc < ?1", params![older_than_utc])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::ids::now_utc_seconds;
    use crate::db::repositories::work_sessions;
    use crate::db::test_support::temp_database;
    use crate::domain::work_session::{CreateWorkSessionInput, UpdateWorkSessionPatch, WorkSessionSource};

    const DAY_START: i64 = 1_700_000_000;

    fn seed_session_with_edits(conn: &mut Connection, edits: usize) -> String {
        let session = work_sessions::create(
            conn,
            &CreateWorkSessionInput {
                started_at_utc: DAY_START,
                ended_at_utc: Some(DAY_START + 3600),
                timezone_id: "UTC".into(),
                active_seconds: 3600,
                source: WorkSessionSource::Detected,
                project_id: None,
                issue_key: None,
                manual_seconds_override: None,
                description: None,
                confidence: None,
                review_status: None,
                is_manually_edited: None,
            },
        )
        .unwrap();
        for i in 0..edits {
            work_sessions::update(
                conn,
                &session.id,
                &UpdateWorkSessionPatch { description: Some(Some(format!("правка {i}"))), ..Default::default() },
            )
            .unwrap();
        }
        session.id
    }

    #[test]
    fn removes_only_entries_older_than_threshold() {
        let (mut conn, _dir) = temp_database();
        let session_id = seed_session_with_edits(&mut conn, 2);
        assert_eq!(list_by_session(&conn, &session_id).unwrap().len(), 2);

        // Порог в будущем — удалит обе (имитирует "старше 30 дней" без
        // необходимости ждать реального времени в тесте).
        let removed = prune_older_than(&conn, now_utc_seconds() + 3600).unwrap();
        assert_eq!(removed, 2);
        assert_eq!(list_by_session(&conn, &session_id).unwrap().len(), 0);
    }

    #[test]
    fn threshold_in_the_past_removes_nothing() {
        let (mut conn, _dir) = temp_database();
        let session_id = seed_session_with_edits(&mut conn, 1);
        let removed = prune_older_than(&conn, 1).unwrap();
        assert_eq!(removed, 0);
        assert_eq!(list_by_session(&conn, &session_id).unwrap().len(), 1);
    }
}
