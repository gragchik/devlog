use rusqlite::{params, Connection, OptionalExtension, Row};

use super::session_edits;
use crate::db::error::{RepoError, RepoResult};
use crate::db::ids::{generate_id, now_utc_seconds};
use crate::domain::confidence::Confidence;
use crate::domain::session_edit::CreateSessionEditInput;
use crate::domain::work_session::{
    CreateWorkSessionInput, UpdateWorkSessionPatch, WorkSession, WorkSessionReviewStatus, WorkSessionSource,
};

fn row_to_session(row: &Row) -> rusqlite::Result<WorkSession> {
    let source_str: String = row.get("source")?;
    let confidence_str: String = row.get("confidence")?;
    let review_status_str: String = row.get("reviewStatus")?;
    Ok(WorkSession {
        id: row.get("id")?,
        started_at_utc: row.get("startedAtUtc")?,
        ended_at_utc: row.get("endedAtUtc")?,
        timezone_id: row.get("timezoneId")?,
        project_id: row.get("projectId")?,
        issue_key: row.get("issueKey")?,
        active_seconds: row.get("activeSeconds")?,
        manual_seconds_override: row.get("manualSecondsOverride")?,
        description: row.get("description")?,
        source: WorkSessionSource::from_db_str(&source_str).expect("invalid source stored in work_sessions"),
        confidence: Confidence::from_db_str(&confidence_str).expect("invalid confidence stored in work_sessions"),
        review_status: WorkSessionReviewStatus::from_db_str(&review_status_str)
            .expect("invalid reviewStatus stored in work_sessions"),
        is_manually_edited: row.get::<_, i64>("isManuallyEdited")? == 1,
        deleted_at: row.get("deletedAt")?,
        created_at_utc: row.get("createdAtUtc")?,
        updated_at_utc: row.get("updatedAtUtc")?,
    })
}

fn assert_valid_interval(started_at_utc: i64, ended_at_utc: Option<i64>) -> RepoResult<()> {
    if let Some(ended) = ended_at_utc {
        if ended <= started_at_utc {
            return Err(RepoError::InvalidInterval { started_at_utc, ended_at_utc: ended });
        }
    }
    Ok(())
}

/// CRUD + журналируемые правки для `work_sessions` (FR-04). Каждая мутация
/// существующей сессии (`update`/`soft_delete`/`restore`/`undo_last_edit`)
/// атомарно пишет и саму строку, и запись в `session_edits` — либо обе
/// операции применяются, либо ни одна (транзакция).
///
/// Разбиение/объединение интервалов (FR-04.6, конфликты перекрытия) —
/// ответственность Session Engine (Итерация 4), здесь не реализовано.
pub fn create(conn: &Connection, input: &CreateWorkSessionInput) -> RepoResult<WorkSession> {
    assert_valid_interval(input.started_at_utc, input.ended_at_utc)?;
    let id = generate_id();
    let now = now_utc_seconds();
    let is_manual = input.source == WorkSessionSource::Manual;
    let confidence = input.confidence.unwrap_or(Confidence::High);
    let review_status = input
        .review_status
        .unwrap_or(if is_manual { WorkSessionReviewStatus::Reviewed } else { WorkSessionReviewStatus::Detected });
    let is_manually_edited = input.is_manually_edited.unwrap_or(is_manual);

    conn.execute(
        "INSERT INTO work_sessions
           (id, startedAtUtc, endedAtUtc, timezoneId, projectId, issueKey, activeSeconds,
            manualSecondsOverride, description, source, confidence, reviewStatus,
            isManuallyEdited, deletedAt, createdAtUtc, updatedAtUtc)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, NULL, ?14, ?14)",
        params![
            id,
            input.started_at_utc,
            input.ended_at_utc,
            input.timezone_id,
            input.project_id,
            input.issue_key,
            input.active_seconds,
            input.manual_seconds_override,
            input.description,
            input.source.as_db_str(),
            confidence.as_db_str(),
            review_status.as_db_str(),
            is_manually_edited as i64,
            now,
        ],
    )?;
    Ok(get_by_id(conn, &id)?.expect("just inserted"))
}

pub fn get_by_id(conn: &Connection, id: &str) -> RepoResult<Option<WorkSession>> {
    Ok(conn
        .query_row("SELECT * FROM work_sessions WHERE id = ?1", params![id], row_to_session)
        .optional()?)
}

/// По умолчанию без мягко удалённых — передайте `include_deleted: true`,
/// чтобы увидеть и их.
pub fn list_by_range(
    conn: &Connection,
    start_utc: i64,
    end_utc_exclusive: i64,
    include_deleted: bool,
) -> RepoResult<Vec<WorkSession>> {
    let sql = if include_deleted {
        "SELECT * FROM work_sessions WHERE startedAtUtc >= ?1 AND startedAtUtc < ?2 ORDER BY startedAtUtc"
    } else {
        "SELECT * FROM work_sessions WHERE startedAtUtc >= ?1 AND startedAtUtc < ?2 AND deletedAt IS NULL ORDER BY startedAtUtc"
    };
    let mut stmt = conn.prepare(sql)?;
    let rows = stmt.query_map(params![start_utc, end_utc_exclusive], row_to_session)?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

fn write_session_row(conn: &Connection, s: &WorkSession) -> rusqlite::Result<()> {
    conn.execute(
        "UPDATE work_sessions SET
           startedAtUtc = ?1, endedAtUtc = ?2, projectId = ?3, issueKey = ?4, activeSeconds = ?5,
           manualSecondsOverride = ?6, description = ?7, confidence = ?8, reviewStatus = ?9,
           isManuallyEdited = ?10, deletedAt = ?11, updatedAtUtc = ?12
         WHERE id = ?13",
        params![
            s.started_at_utc,
            s.ended_at_utc,
            s.project_id,
            s.issue_key,
            s.active_seconds,
            s.manual_seconds_override,
            s.description,
            s.confidence.as_db_str(),
            s.review_status.as_db_str(),
            s.is_manually_edited as i64,
            s.deleted_at,
            s.updated_at_utc,
            s.id,
        ],
    )?;
    Ok(())
}

/// Применяет патч к сессии и атомарно пишет snapshot до/после в
/// `session_edits`. Возвращает `RepoError::InvalidInterval`, если патч
/// нарушает `end > start`.
pub fn update(conn: &mut Connection, id: &str, patch: &UpdateWorkSessionPatch) -> RepoResult<WorkSession> {
    let tx = conn.transaction()?;

    let previous = get_by_id(&tx, id)?.ok_or_else(|| RepoError::NotFound(format!("WorkSession {id}")))?;

    let mut next = previous.clone();
    if let Some(v) = patch.started_at_utc {
        next.started_at_utc = v;
    }
    if let Some(v) = patch.ended_at_utc {
        next.ended_at_utc = v;
    }
    if let Some(v) = patch.project_id.clone() {
        next.project_id = v;
    }
    if let Some(v) = patch.issue_key.clone() {
        next.issue_key = v;
    }
    if let Some(v) = patch.active_seconds {
        next.active_seconds = v;
    }
    if let Some(v) = patch.manual_seconds_override {
        next.manual_seconds_override = v;
    }
    if let Some(v) = patch.description.clone() {
        next.description = v;
    }
    if let Some(v) = patch.confidence {
        next.confidence = v;
    }
    if let Some(v) = patch.review_status {
        next.review_status = v;
    }
    next.is_manually_edited = true;
    next.updated_at_utc = now_utc_seconds();

    assert_valid_interval(next.started_at_utc, next.ended_at_utc)?;

    write_session_row(&tx, &next)?;
    session_edits::append(
        &tx,
        &CreateSessionEditInput {
            session_id: id.to_string(),
            operation: crate::domain::work_session::SessionEditOperation::Update,
            previous_state: previous,
            next_state: next.clone(),
        },
    )?;

    tx.commit()?;
    Ok(next)
}

fn set_deleted_at(
    conn: &mut Connection,
    id: &str,
    deleted_at: Option<i64>,
    operation: crate::domain::work_session::SessionEditOperation,
) -> RepoResult<WorkSession> {
    let tx = conn.transaction()?;
    let previous = get_by_id(&tx, id)?.ok_or_else(|| RepoError::NotFound(format!("WorkSession {id}")))?;
    let mut next = previous.clone();
    next.deleted_at = deleted_at;
    next.updated_at_utc = now_utc_seconds();

    write_session_row(&tx, &next)?;
    session_edits::append(
        &tx,
        &CreateSessionEditInput { session_id: id.to_string(), operation, previous_state: previous, next_state: next.clone() },
    )?;
    tx.commit()?;
    Ok(next)
}

pub fn soft_delete(conn: &mut Connection, id: &str) -> RepoResult<WorkSession> {
    set_deleted_at(conn, id, Some(now_utc_seconds()), crate::domain::work_session::SessionEditOperation::Delete)
}

pub fn restore(conn: &mut Connection, id: &str) -> RepoResult<WorkSession> {
    set_deleted_at(conn, id, None, crate::domain::work_session::SessionEditOperation::Restore)
}

/// Откатывает сессию к состоянию перед последней правкой в журнале. Сама
/// запись последней правки не удаляется (журнал append-only) — вместо
/// этого добавляется новая запись с `operation: Undo`, что позволяет
/// повторным undo идти дальше в историю, не теряя ни одной записи.
pub fn undo_last_edit(conn: &mut Connection, session_id: &str) -> RepoResult<Option<WorkSession>> {
    let tx = conn.transaction()?;

    let Some(last_edit) = session_edits::get_latest_for_session(&tx, session_id)? else {
        return Ok(None);
    };
    let current = get_by_id(&tx, session_id)?.ok_or_else(|| RepoError::NotFound(format!("WorkSession {session_id}")))?;

    let mut restored = last_edit.previous_state;
    restored.updated_at_utc = now_utc_seconds();
    write_session_row(&tx, &restored)?;

    session_edits::append(
        &tx,
        &CreateSessionEditInput {
            session_id: session_id.to_string(),
            operation: crate::domain::work_session::SessionEditOperation::Undo,
            previous_state: current,
            next_state: restored.clone(),
        },
    )?;

    tx.commit()?;
    Ok(Some(restored))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::repositories::session_edits;
    use crate::db::test_support::temp_database;
    use std::sync::{Arc, Mutex};
    use std::thread;

    const DAY_START: i64 = 1_700_000_000;

    fn minimal_input(started_at_utc: i64, ended_at_utc: Option<i64>) -> CreateWorkSessionInput {
        CreateWorkSessionInput {
            started_at_utc,
            ended_at_utc,
            timezone_id: "UTC".into(),
            active_seconds: ended_at_utc.map(|e| e - started_at_utc).unwrap_or(0),
            source: WorkSessionSource::Detected,
            project_id: None,
            issue_key: None,
            manual_seconds_override: None,
            description: None,
            confidence: None,
            review_status: None,
            is_manually_edited: None,
        }
    }

    #[test]
    fn creates_session_and_reads_it_back_unchanged() {
        let (conn, _dir) = temp_database();
        let mut input = minimal_input(DAY_START, Some(DAY_START + 3600));
        input.issue_key = Some("OB-448".into());
        let session = create(&conn, &input).unwrap();

        assert_eq!(get_by_id(&conn, &session.id).unwrap(), Some(session.clone()));
        assert_eq!(session.review_status, WorkSessionReviewStatus::Detected);
        assert!(!session.is_manually_edited);
    }

    #[test]
    fn manual_session_defaults_to_reviewed_and_manually_edited() {
        let (conn, _dir) = temp_database();
        let mut input = minimal_input(DAY_START, Some(DAY_START + 1800));
        input.source = WorkSessionSource::Manual;
        input.description = Some("Созвон по архитектуре".into());
        let session = create(&conn, &input).unwrap();
        assert_eq!(session.review_status, WorkSessionReviewStatus::Reviewed);
        assert!(session.is_manually_edited);
    }

    #[test]
    fn rejects_ended_at_not_after_started_at_on_create() {
        let (conn, _dir) = temp_database();
        let err = create(&conn, &minimal_input(DAY_START, Some(DAY_START))).unwrap_err();
        assert!(matches!(err, RepoError::InvalidInterval { .. }));
    }

    #[test]
    fn list_by_range_filters_window_and_excludes_deleted_by_default() {
        let (mut conn, _dir) = temp_database();
        let in_range = create(&conn, &minimal_input(DAY_START + 100, Some(DAY_START + 200))).unwrap();
        create(&conn, &minimal_input(DAY_START + 10_000, Some(DAY_START + 10_100))).unwrap();
        let deleted = create(&conn, &minimal_input(DAY_START + 300, Some(DAY_START + 400))).unwrap();
        soft_delete(&mut conn, &deleted.id).unwrap();

        let result = list_by_range(&conn, DAY_START, DAY_START + 1000, false).unwrap();
        assert_eq!(result.iter().map(|s| &s.id).collect::<Vec<_>>(), vec![&in_range.id]);

        let mut with_deleted: Vec<_> = list_by_range(&conn, DAY_START, DAY_START + 1000, true).unwrap().into_iter().map(|s| s.id).collect();
        with_deleted.sort();
        let mut expected = vec![deleted.id.clone(), in_range.id.clone()];
        expected.sort();
        assert_eq!(with_deleted, expected);
    }

    #[test]
    fn update_atomically_changes_session_and_writes_journal_snapshot() {
        let (mut conn, _dir) = temp_database();
        let mut input = minimal_input(DAY_START, Some(DAY_START + 3600));
        input.issue_key = Some("OB-448".into());
        let session = create(&conn, &input).unwrap();

        let patch = UpdateWorkSessionPatch { issue_key: Some(Some("OB-419".into())), description: Some(Some("переназначено".into())), ..Default::default() };
        let updated = update(&mut conn, &session.id, &patch).unwrap();

        assert_eq!(updated.issue_key.as_deref(), Some("OB-419"));
        assert!(updated.is_manually_edited);

        let log = session_edits::list_by_session(&conn, &session.id).unwrap();
        assert_eq!(log.len(), 1);
        assert_eq!(log[0].operation, crate::domain::work_session::SessionEditOperation::Update);
        assert_eq!(log[0].previous_state.issue_key.as_deref(), Some("OB-448"));
        assert_eq!(log[0].next_state.issue_key.as_deref(), Some("OB-419"));
    }

    #[test]
    fn update_rejects_patch_violating_interval_and_does_not_touch_journal() {
        let (mut conn, _dir) = temp_database();
        let session = create(&conn, &minimal_input(DAY_START, Some(DAY_START + 3600))).unwrap();

        let patch = UpdateWorkSessionPatch { ended_at_utc: Some(Some(DAY_START - 1)), ..Default::default() };
        let err = update(&mut conn, &session.id, &patch).unwrap_err();
        assert!(matches!(err, RepoError::InvalidInterval { .. }));

        assert_eq!(session_edits::list_by_session(&conn, &session.id).unwrap().len(), 0);
        assert_eq!(get_by_id(&conn, &session.id).unwrap(), Some(session)); // не изменилась
    }

    #[test]
    fn soft_delete_and_restore_toggle_deleted_at_and_are_journaled() {
        let (mut conn, _dir) = temp_database();
        let session = create(&conn, &minimal_input(DAY_START, Some(DAY_START + 3600))).unwrap();

        let deleted = soft_delete(&mut conn, &session.id).unwrap();
        assert!(deleted.deleted_at.is_some());

        let restored = restore(&mut conn, &session.id).unwrap();
        assert!(restored.deleted_at.is_none());

        let ops: Vec<_> = session_edits::list_by_session(&conn, &session.id).unwrap().into_iter().map(|e| e.operation).collect();
        assert_eq!(
            ops,
            vec![crate::domain::work_session::SessionEditOperation::Delete, crate::domain::work_session::SessionEditOperation::Restore]
        );
    }

    #[test]
    fn undo_last_edit_reverts_without_deleting_journal_entry() {
        let (mut conn, _dir) = temp_database();
        let mut input = minimal_input(DAY_START, Some(DAY_START + 3600));
        input.issue_key = Some("OB-448".into());
        let session = create(&conn, &input).unwrap();
        update(&mut conn, &session.id, &UpdateWorkSessionPatch { issue_key: Some(Some("OB-419".into())), ..Default::default() }).unwrap();

        let after_undo = undo_last_edit(&mut conn, &session.id).unwrap();

        assert_eq!(after_undo.as_ref().and_then(|s| s.issue_key.clone()), Some("OB-448".to_string()));
        assert_eq!(get_by_id(&conn, &session.id).unwrap().and_then(|s| s.issue_key), Some("OB-448".to_string()));

        let ops: Vec<_> = session_edits::list_by_session(&conn, &session.id).unwrap().into_iter().map(|e| e.operation).collect();
        assert_eq!(ops, vec![crate::domain::work_session::SessionEditOperation::Update, crate::domain::work_session::SessionEditOperation::Undo]);
    }

    #[test]
    fn undo_last_edit_returns_none_when_no_edits_exist() {
        let (mut conn, _dir) = temp_database();
        let session = create(&conn, &minimal_input(DAY_START, Some(DAY_START + 3600))).unwrap();
        assert_eq!(undo_last_edit(&mut conn, &session.id).unwrap(), None);
    }

    /// Регрессия: в первой версии `undo_last_edit` в UPDATE отсутствовал
    /// `WHERE id = ?` — обновил бы все строки таблицы. Создаём две сессии,
    /// откатываем правку у одной, проверяем, что вторая не задета.
    #[test]
    fn undo_touches_only_its_own_session_not_the_whole_table() {
        let (mut conn, _dir) = temp_database();
        let mut input_a = minimal_input(DAY_START, Some(DAY_START + 100));
        input_a.issue_key = Some("OB-448".into());
        let a = create(&conn, &input_a).unwrap();

        let mut input_b = minimal_input(DAY_START + 1000, Some(DAY_START + 1100));
        input_b.issue_key = Some("OB-419".into());
        let b = create(&conn, &input_b).unwrap();

        update(&mut conn, &a.id, &UpdateWorkSessionPatch { issue_key: Some(Some("OB-448-renamed".into())), ..Default::default() }).unwrap();
        undo_last_edit(&mut conn, &a.id).unwrap();

        assert_eq!(get_by_id(&conn, &a.id).unwrap().and_then(|s| s.issue_key), Some("OB-448".to_string()));
        assert_eq!(get_by_id(&conn, &b.id).unwrap().and_then(|s| s.issue_key), Some("OB-419".to_string())); // не затронута
    }

    /// В отличие от прежней (TS/Electron) версии, где "параллельность"
    /// была лишь иллюзией однопоточного JS, здесь — настоящие ОС-потоки,
    /// синхронизированные через тот же `Mutex<Connection>`, что и реальное
    /// Tauri `AppDatabase`. Тест проверяет, что это действительно не даёт
    /// повредить БД/потерять правку при истинном параллелизме.
    #[test]
    fn two_real_threads_updating_the_same_session_do_not_corrupt_the_db() {
        let (conn, _dir) = temp_database();
        let session = create(&conn, &minimal_input(DAY_START, Some(DAY_START + 3600))).unwrap();
        let session_id = session.id.clone();

        let shared = Arc::new(Mutex::new(conn));

        let make_updater = |label: &'static str| {
            let shared = Arc::clone(&shared);
            let session_id = session_id.clone();
            thread::spawn(move || {
                let mut guard = shared.lock().unwrap();
                update(&mut guard, &session_id, &UpdateWorkSessionPatch { description: Some(Some(label.to_string())), ..Default::default() }).unwrap()
            })
        };

        let thread_a = make_updater("правка A");
        let thread_b = make_updater("правка B");
        let result_a = thread_a.join().unwrap();
        let result_b = thread_b.join().unwrap();

        let conn = shared.lock().unwrap();
        let final_session = get_by_id(&conn, &session_id).unwrap().unwrap();

        // Детерминированный результат: одна из правок победила целиком (не смесь полей).
        assert!([Some("правка A".to_string()), Some("правка B".to_string())].contains(&final_session.description));
        assert!([result_a.description, result_b.description].contains(&final_session.description));

        // Обе правки зафиксированы в журнале — ни одна не потерялась бесследно.
        let log = session_edits::list_by_session(&conn, &session_id).unwrap();
        assert_eq!(log.len(), 2);
        let mut descriptions: Vec<_> = log.iter().map(|e| e.next_state.description.clone()).collect();
        descriptions.sort();
        assert_eq!(descriptions, vec![Some("правка A".to_string()), Some("правка B".to_string())]);
    }
}
