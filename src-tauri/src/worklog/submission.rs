//! Отправка черновиков в Jira (FR-07.5–7.7) и reconciliation после
//! неизвестного результата (FR-07.6).
//!
//! Порядок одной отправки:
//! 1. локальные проверки (статус черновика, незакрытые попытки, дубль
//!    payload-хэша) — под блокировкой БД;
//! 2. remote-проверка дублей: нет ли уже в Jira нашего worklog'а с тем же
//!    `started`/временем — сеть, БД не заблокирована;
//! 3. повтор локальных проверок + запись `pending` ДО запроса — если
//!    процесс упадёт посреди POST, после рестарта попытка станет `unknown`
//!    (`mark_stale_pending_as_unknown`), а не будет тихо повторена;
//! 4. POST — сеть;
//! 5. итог: `posted` (+ черновик `submitted`) / `failed` (Jira точно
//!    отказала — можно исправить и отправить снова) / `unknown` (таймаут —
//!    только reconciliation, никакого автоматического повтора).
//!
//! Блокировка `Mutex<Connection>` никогда не держится через `await` — иначе
//! медленная Jira заморозила бы трекер и overlay.

use std::sync::Mutex;

use rusqlite::Connection;
use serde::Serialize;

use crate::db::repositories::{jira_submissions, worklog_drafts};
use crate::domain::jira::{JiraError, JiraSubmissionState, JiraUser, RemoteWorklog, WorklogEntry};
use crate::domain::worklog_draft::{UpdateWorklogDraftPatch, WorklogDraftStatus};
use crate::integrations::jira::{compute_payload_hash, JiraProvider};
use crate::tracking::issue_key::is_valid_issue_key;

use super::drafts::MIN_WORKLOG_SECONDS;

/// Допуск сравнения `started` при поиске своего worklog'а в Jira.
const STARTED_TOLERANCE_SECONDS: i64 = 60;

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum SubmitOutcome {
    #[serde(rename_all = "camelCase")]
    Posted { remote_worklog_id: Option<String> },
    /// Jira ответила ошибкой — запись точно не создана, черновик остаётся
    /// черновиком, повтор разрешён после исправления.
    Failed { message: String },
    /// Результат POST неизвестен — нужна проверка (reconciliation).
    Unknown { message: String },
    /// Отправка даже не начиналась (дубль, уже отправлено, ошибка
    /// валидации, Jira недоступна на этапе проверок).
    Blocked { message: String },
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DraftSubmitResult {
    pub draft_id: String,
    pub outcome: SubmitOutcome,
}

fn lock(db: &Mutex<Connection>) -> Result<std::sync::MutexGuard<'_, Connection>, String> {
    db.lock().map_err(|_| "database lock poisoned".to_string())
}

/// Свой worklog в Jira, совпадающий с локальной записью (FR-07.6/7.7).
fn find_own_match<'a>(worklogs: &'a [RemoteWorklog], me: &JiraUser, entry: &WorklogEntry) -> Option<&'a RemoteWorklog> {
    worklogs.iter().find(|w| {
        w.author_account_id.as_deref() == Some(me.account_id.as_str())
            && (w.started_at_utc - entry.started_at_utc).abs() <= STARTED_TOLERANCE_SECONDS
            && w.time_spent_seconds == entry.time_spent_seconds
    })
}

/// Шаги 1 и 3: всё, что можно проверить локально. Возвращает то, что
/// будет отправлено, и его хэш.
fn check_locally(conn: &Connection, draft_id: &str) -> Result<(WorklogEntry, String), String> {
    let draft = worklog_drafts::get_by_id(conn, draft_id).map_err(|e| e.to_string())?.ok_or("черновик не найден")?;
    if draft.status == WorklogDraftStatus::Submitted {
        return Err("уже отправлено в Jira".into());
    }
    let previous = jira_submissions::list_by_local_draft(conn, draft_id).map_err(|e| e.to_string())?;
    if previous.iter().any(|s| matches!(s.state, JiraSubmissionState::Pending | JiraSubmissionState::Unknown)) {
        return Err("предыдущая отправка завершилась с неизвестным результатом — сначала проверьте её в Jira".into());
    }
    if previous.iter().any(|s| s.state == JiraSubmissionState::Posted) {
        return Err("уже отправлено в Jira".into());
    }
    if !is_valid_issue_key(&draft.issue_key) {
        return Err(format!("некорректный ключ задачи «{}»", draft.issue_key));
    }
    if draft.time_spent_seconds < MIN_WORKLOG_SECONDS {
        return Err("время меньше минуты — Jira такую запись не примет".into());
    }

    let entry = WorklogEntry {
        issue_key: draft.issue_key,
        started_at_utc: draft.started_at_utc,
        time_spent_seconds: draft.time_spent_seconds,
        comment: draft.comment.unwrap_or_default(),
        local_draft_id: draft.id,
    };
    let hash = compute_payload_hash(&entry.issue_key, entry.started_at_utc, entry.time_spent_seconds, &entry.comment);
    if let Some(dup) = jira_submissions::find_non_failed_by_payload_hash(conn, &hash).map_err(|e| e.to_string())? {
        return Err(format!("точно такая же запись уже отправлялась ({}, {})", dup.issue_key, dup.state.as_db_str()));
    }
    Ok((entry, hash))
}

/// Шаг 5: фиксирует итог POST'а атомарно (попытка + статус черновика).
fn record_outcome(conn: &mut Connection, submission_id: &str, draft_id: &str, outcome: &SubmitOutcome) -> rusqlite::Result<()> {
    let tx = conn.transaction()?;
    match outcome {
        SubmitOutcome::Posted { remote_worklog_id } => {
            jira_submissions::resolve(&tx, submission_id, JiraSubmissionState::Posted, remote_worklog_id.as_deref())?;
            worklog_drafts::update(&tx, draft_id, &UpdateWorklogDraftPatch { status: Some(WorklogDraftStatus::Submitted), ..Default::default() })?;
        }
        SubmitOutcome::Failed { .. } => {
            jira_submissions::resolve(&tx, submission_id, JiraSubmissionState::Failed, None)?;
        }
        SubmitOutcome::Unknown { .. } => {
            jira_submissions::resolve(&tx, submission_id, JiraSubmissionState::Unknown, None)?;
        }
        SubmitOutcome::Blocked { .. } => {}
    }
    tx.commit()
}

async fn submit_one(db: &Mutex<Connection>, provider: &dyn JiraProvider, me: &JiraUser, draft_id: &str) -> SubmitOutcome {
    let blocked = |message: String| SubmitOutcome::Blocked { message };

    // 1. Локальные проверки.
    let entry = match lock(db).and_then(|conn| check_locally(&conn, draft_id)) {
        Ok((entry, _)) => entry,
        Err(message) => return blocked(message),
    };

    // 2. Remote-проверка дублей (FR-07.7).
    match provider.list_worklogs_near(&entry.issue_key, entry.started_at_utc).await {
        Ok(worklogs) => {
            if let Some(existing) = find_own_match(&worklogs, me, &entry) {
                return blocked(format!("в Jira уже есть ваш worklog на это время (id {}) — повторно не отправляю", existing.id));
            }
        }
        Err(err) => return blocked(format!("не удалось проверить дубли в Jira, ничего не отправлено: {err}")),
    }

    // 3. Повторная локальная проверка (могли нажать «Отправить» дважды) +
    //    `pending` до запроса — в одной блокировке.
    let submission_id = {
        let conn = match lock(db) {
            Ok(conn) => conn,
            Err(message) => return blocked(message),
        };
        let (entry_now, hash) = match check_locally(&conn, draft_id) {
            Ok(v) => v,
            Err(message) => return blocked(message),
        };
        if entry_now.started_at_utc != entry.started_at_utc || entry_now.time_spent_seconds != entry.time_spent_seconds || entry_now.issue_key != entry.issue_key {
            return blocked("черновик изменился во время проверки — откройте preview заново".into());
        }
        match jira_submissions::create_pending(&conn, draft_id, &entry.issue_key, &hash) {
            Ok(sub) => sub.id,
            Err(e) => return blocked(e.to_string()),
        }
    };

    // 4. POST.
    let outcome = match provider.post_worklog(&entry).await {
        Ok(result) => SubmitOutcome::Posted { remote_worklog_id: Some(result.remote_worklog_id) },
        Err(err @ JiraError::NetworkUnknown(_)) => SubmitOutcome::Unknown { message: err.to_string() },
        Err(err) => SubmitOutcome::Failed { message: err.to_string() },
    };

    // 5. Итог. Если даже записать его не удалось, попытка останется
    //    `pending` и при следующем старте станет `unknown` — безопасная
    //    сторона (без слепого повтора).
    if let Err(message) = lock(db).and_then(|mut conn| record_outcome(&mut conn, &submission_id, draft_id, &outcome).map_err(|e| e.to_string())) {
        return SubmitOutcome::Unknown { message: format!("результат отправки не сохранён локально ({message}) — проверьте запись в Jira") };
    }
    outcome
}

/// FR-07.5: отправка только явно выбранных пользователем черновиков, по
/// одному, последовательно — каждый со своим итогом.
pub async fn submit_drafts(db: &Mutex<Connection>, provider: &dyn JiraProvider, draft_ids: &[String]) -> Vec<DraftSubmitResult> {
    // Без account id не сделать remote-проверку дублей — значит не отправляем.
    let me = match provider.verify_connection().await {
        Ok(me) => me,
        Err(err) => {
            let message = format!("нет соединения с Jira, ничего не отправлено: {err}");
            return draft_ids.iter().map(|id| DraftSubmitResult { draft_id: id.clone(), outcome: SubmitOutcome::Blocked { message: message.clone() } }).collect();
        }
    };

    let mut results = Vec::with_capacity(draft_ids.len());
    for draft_id in draft_ids {
        let outcome = submit_one(db, provider, &me, draft_id).await;
        results.push(DraftSubmitResult { draft_id: draft_id.clone(), outcome });
    }
    results
}

/// FR-07.6: разбор попытки `unknown`/`pending` — ищем свой worklog в Jira.
/// Нашёлся → `posted` с его id; точно нет → `failed` (можно отправить
/// заново). Jira недоступна → ошибка, попытка остаётся `unknown`.
pub async fn reconcile(db: &Mutex<Connection>, provider: &dyn JiraProvider, submission_id: &str) -> Result<SubmitOutcome, String> {
    let (submission, entry) = {
        let conn = lock(db)?;
        let submission = jira_submissions::get_by_id(&conn, submission_id).map_err(|e| e.to_string())?.ok_or("попытка отправки не найдена")?;
        if !matches!(submission.state, JiraSubmissionState::Pending | JiraSubmissionState::Unknown) {
            return Err(format!("попытка уже разрешена ({})", submission.state.as_db_str()));
        }
        let draft = worklog_drafts::get_by_id(&conn, &submission.local_draft_id).map_err(|e| e.to_string())?.ok_or("черновик попытки не найден")?;
        let entry = WorklogEntry {
            issue_key: draft.issue_key,
            started_at_utc: draft.started_at_utc,
            time_spent_seconds: draft.time_spent_seconds,
            comment: draft.comment.unwrap_or_default(),
            local_draft_id: draft.id,
        };
        (submission, entry)
    };

    let me = provider.verify_connection().await.map_err(|e| e.to_string())?;
    let worklogs = provider.list_worklogs_near(&entry.issue_key, entry.started_at_utc).await.map_err(|e| e.to_string())?;

    let outcome = match find_own_match(&worklogs, &me, &entry) {
        Some(found) => SubmitOutcome::Posted { remote_worklog_id: Some(found.id.clone()) },
        None => SubmitOutcome::Failed { message: "в Jira такой записи нет — черновик можно отправить заново".into() },
    };
    let mut conn = lock(db)?;
    record_outcome(&mut conn, &submission.id, &submission.local_draft_id, &outcome).map_err(|e| e.to_string())?;
    Ok(outcome)
}

/// Ручное разрешение, когда автоматическая проверка невозможна (нет прав
/// читать worklog'и и т.п.): пользователь сам посмотрел в Jira и явно
/// говорит, есть там запись или нет ("retry после явного выбора").
pub fn resolve_manually(conn: &mut Connection, submission_id: &str, posted: bool) -> Result<SubmitOutcome, String> {
    let submission = jira_submissions::get_by_id(conn, submission_id).map_err(|e| e.to_string())?.ok_or("попытка отправки не найдена")?;
    if !matches!(submission.state, JiraSubmissionState::Pending | JiraSubmissionState::Unknown) {
        return Err(format!("попытка уже разрешена ({})", submission.state.as_db_str()));
    }
    let outcome = if posted {
        SubmitOutcome::Posted { remote_worklog_id: None }
    } else {
        SubmitOutcome::Failed { message: "отмечено вручную: в Jira записи нет".into() }
    };
    record_outcome(conn, &submission.id, &submission.local_draft_id, &outcome).map_err(|e| e.to_string())?;
    Ok(outcome)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::test_support::temp_database;
    use crate::domain::jira::{JiraIssueSummary, JiraWorklogResult};
    use crate::domain::worklog_draft::{CreateWorklogDraftInput, WorklogDraft};
    use async_trait::async_trait;
    use std::sync::atomic::{AtomicUsize, Ordering};

    const STARTED: i64 = 1_700_000_000;

    /// Поддельная Jira: запоминает принятые POST'ы как настоящие worklog'и
    /// (чтобы reconciliation находил их), умеет "потерять ответ".
    struct FakeJira {
        worklogs: Mutex<Vec<RemoteWorklog>>,
        post_behaviour: Mutex<PostBehaviour>,
        posts: AtomicUsize,
        online: bool,
    }

    #[derive(Clone, Copy)]
    enum PostBehaviour {
        Accept,
        /// Сервер создал запись, но ответ до нас не дошёл (таймаут).
        AcceptButTimeout,
        Reject(u16),
    }

    impl FakeJira {
        fn new(behaviour: PostBehaviour) -> Self {
            Self { worklogs: Mutex::new(Vec::new()), post_behaviour: Mutex::new(behaviour), posts: AtomicUsize::new(0), online: true }
        }
        fn set_behaviour(&self, b: PostBehaviour) {
            *self.post_behaviour.lock().unwrap() = b;
        }
        fn me() -> JiraUser {
            JiraUser { account_id: "me".into(), display_name: "Me".into() }
        }
    }

    #[async_trait]
    impl JiraProvider for FakeJira {
        async fn verify_connection(&self) -> Result<JiraUser, JiraError> {
            if self.online {
                Ok(Self::me())
            } else {
                Err(JiraError::NetworkUnknown("offline".into()))
            }
        }
        async fn get_issue(&self, issue_key: &str) -> Result<JiraIssueSummary, JiraError> {
            Ok(JiraIssueSummary { issue_key: issue_key.into(), title: "t".into() })
        }
        async fn list_worklogs_near(&self, _issue_key: &str, _started: i64) -> Result<Vec<RemoteWorklog>, JiraError> {
            Ok(self.worklogs.lock().unwrap().clone())
        }
        async fn post_worklog(&self, entry: &WorklogEntry) -> Result<JiraWorklogResult, JiraError> {
            self.posts.fetch_add(1, Ordering::SeqCst);
            let behaviour = *self.post_behaviour.lock().unwrap();
            let store = || {
                let mut worklogs = self.worklogs.lock().unwrap();
                let id = (10_000 + worklogs.len()).to_string();
                worklogs.push(RemoteWorklog { id: id.clone(), author_account_id: Some("me".into()), started_at_utc: entry.started_at_utc, time_spent_seconds: entry.time_spent_seconds });
                id
            };
            match behaviour {
                PostBehaviour::Accept => Ok(JiraWorklogResult { local_draft_id: entry.local_draft_id.clone(), remote_worklog_id: store() }),
                PostBehaviour::AcceptButTimeout => {
                    store();
                    Err(JiraError::NetworkUnknown("timed out".into()))
                }
                PostBehaviour::Reject(status) => Err(JiraError::ApiError { status, message: String::new() }),
            }
        }
    }

    fn setup() -> (Mutex<Connection>, tempfile::TempDir, WorklogDraft) {
        let (conn, dir) = temp_database();
        let draft = worklog_drafts::create(
            &conn,
            &CreateWorklogDraftInput {
                local_day: "2023-11-14".into(),
                issue_key: "OB-448".into(),
                time_spent_seconds: 5400,
                started_at_utc: STARTED,
                comment: Some("Работа над задачей OB-448".into()),
                selected_session_ids: vec![],
                status: None,
            },
        )
        .unwrap();
        (Mutex::new(conn), dir, draft)
    }

    fn draft_status(db: &Mutex<Connection>, id: &str) -> WorklogDraftStatus {
        worklog_drafts::get_by_id(&db.lock().unwrap(), id).unwrap().unwrap().status
    }

    #[tokio::test]
    async fn successful_post_marks_draft_submitted_with_remote_id() {
        let (db, _dir, draft) = setup();
        let jira = FakeJira::new(PostBehaviour::Accept);

        let results = submit_drafts(&db, &jira, std::slice::from_ref(&draft.id)).await;
        assert_eq!(results[0].outcome, SubmitOutcome::Posted { remote_worklog_id: Some("10000".into()) });
        assert_eq!(draft_status(&db, &draft.id), WorklogDraftStatus::Submitted);
        // E2E-сценарий 8: после restart запись хранит remote id и статус posted.
        let subs = jira_submissions::list_by_local_draft(&db.lock().unwrap(), &draft.id).unwrap();
        assert_eq!(subs[0].state, JiraSubmissionState::Posted);
        assert_eq!(subs[0].remote_worklog_id.as_deref(), Some("10000"));
    }

    #[tokio::test]
    async fn second_submit_of_posted_draft_is_blocked_without_network_post() {
        let (db, _dir, draft) = setup();
        let jira = FakeJira::new(PostBehaviour::Accept);
        submit_drafts(&db, &jira, std::slice::from_ref(&draft.id)).await;

        let again = submit_drafts(&db, &jira, std::slice::from_ref(&draft.id)).await;
        assert!(matches!(again[0].outcome, SubmitOutcome::Blocked { .. }));
        assert_eq!(jira.posts.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn timeout_leaves_unknown_and_never_reposts_blindly() {
        // E2E-сценарий 7: timeout POST → черновик цел, дубль не публикуется.
        let (db, _dir, draft) = setup();
        let jira = FakeJira::new(PostBehaviour::AcceptButTimeout);

        let first = submit_drafts(&db, &jira, std::slice::from_ref(&draft.id)).await;
        assert!(matches!(first[0].outcome, SubmitOutcome::Unknown { .. }));
        assert_eq!(draft_status(&db, &draft.id), WorklogDraftStatus::Draft, "черновик не потерян");

        jira.set_behaviour(PostBehaviour::Accept);
        let retry = submit_drafts(&db, &jira, std::slice::from_ref(&draft.id)).await;
        assert!(matches!(retry[0].outcome, SubmitOutcome::Blocked { .. }));
        assert_eq!(jira.posts.load(Ordering::SeqCst), 1, "повторного POST не было");
    }

    #[tokio::test]
    async fn reconcile_finds_worklog_accepted_before_timeout() {
        let (db, _dir, draft) = setup();
        let jira = FakeJira::new(PostBehaviour::AcceptButTimeout);
        submit_drafts(&db, &jira, std::slice::from_ref(&draft.id)).await;
        let unknown = jira_submissions::list_unknown(&db.lock().unwrap()).unwrap().remove(0);

        let outcome = reconcile(&db, &jira, &unknown.id).await.unwrap();
        assert_eq!(outcome, SubmitOutcome::Posted { remote_worklog_id: Some("10000".into()) });
        assert_eq!(draft_status(&db, &draft.id), WorklogDraftStatus::Submitted);
    }

    #[tokio::test]
    async fn reconcile_without_remote_worklog_allows_resubmit() {
        let (db, _dir, draft) = setup();
        let jira = FakeJira::new(PostBehaviour::AcceptButTimeout);
        submit_drafts(&db, &jira, std::slice::from_ref(&draft.id)).await;
        // Сервер на самом деле запись НЕ сохранил.
        jira.worklogs.lock().unwrap().clear();
        let unknown = jira_submissions::list_unknown(&db.lock().unwrap()).unwrap().remove(0);

        assert!(matches!(reconcile(&db, &jira, &unknown.id).await.unwrap(), SubmitOutcome::Failed { .. }));

        jira.set_behaviour(PostBehaviour::Accept);
        let retry = submit_drafts(&db, &jira, std::slice::from_ref(&draft.id)).await;
        assert!(matches!(retry[0].outcome, SubmitOutcome::Posted { .. }));
    }

    #[tokio::test]
    async fn api_error_keeps_draft_and_allows_retry() {
        let (db, _dir, draft) = setup();
        let jira = FakeJira::new(PostBehaviour::Reject(401));

        let first = submit_drafts(&db, &jira, std::slice::from_ref(&draft.id)).await;
        assert!(matches!(first[0].outcome, SubmitOutcome::Failed { .. }));
        assert_eq!(draft_status(&db, &draft.id), WorklogDraftStatus::Draft);

        jira.set_behaviour(PostBehaviour::Accept);
        assert!(matches!(submit_drafts(&db, &jira, std::slice::from_ref(&draft.id)).await[0].outcome, SubmitOutcome::Posted { .. }));
    }

    #[tokio::test]
    async fn existing_own_remote_worklog_blocks_post() {
        let (db, _dir, draft) = setup();
        let jira = FakeJira::new(PostBehaviour::Accept);
        jira.worklogs.lock().unwrap().push(RemoteWorklog { id: "777".into(), author_account_id: Some("me".into()), started_at_utc: STARTED + 30, time_spent_seconds: 5400 });

        let results = submit_drafts(&db, &jira, std::slice::from_ref(&draft.id)).await;
        assert!(matches!(&results[0].outcome, SubmitOutcome::Blocked { message } if message.contains("777")));
        assert_eq!(jira.posts.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn colleagues_worklog_at_same_time_does_not_block() {
        let (db, _dir, draft) = setup();
        let jira = FakeJira::new(PostBehaviour::Accept);
        jira.worklogs.lock().unwrap().push(RemoteWorklog { id: "1".into(), author_account_id: Some("colleague".into()), started_at_utc: STARTED, time_spent_seconds: 5400 });

        assert!(matches!(submit_drafts(&db, &jira, std::slice::from_ref(&draft.id)).await[0].outcome, SubmitOutcome::Posted { .. }));
    }

    #[tokio::test]
    async fn offline_jira_blocks_everything_without_touching_drafts() {
        let (db, _dir, draft) = setup();
        let mut jira = FakeJira::new(PostBehaviour::Accept);
        jira.online = false;

        let results = submit_drafts(&db, &jira, std::slice::from_ref(&draft.id)).await;
        assert!(matches!(results[0].outcome, SubmitOutcome::Blocked { .. }));
        assert_eq!(draft_status(&db, &draft.id), WorklogDraftStatus::Draft);
        assert!(jira_submissions::list_by_local_draft(&db.lock().unwrap(), &draft.id).unwrap().is_empty());
    }

    #[tokio::test]
    async fn identical_payload_in_another_draft_is_a_local_duplicate() {
        let (db, _dir, draft) = setup();
        let jira = FakeJira::new(PostBehaviour::Accept);
        submit_drafts(&db, &jira, std::slice::from_ref(&draft.id)).await;
        jira.worklogs.lock().unwrap().clear(); // чтобы сработала именно локальная проверка
        let copy = worklog_drafts::create(
            &db.lock().unwrap(),
            &CreateWorklogDraftInput {
                local_day: draft.local_day.clone(),
                issue_key: draft.issue_key.clone(),
                time_spent_seconds: draft.time_spent_seconds,
                started_at_utc: draft.started_at_utc,
                comment: draft.comment.clone(),
                selected_session_ids: vec![],
                status: None,
            },
        )
        .unwrap();

        assert!(matches!(submit_drafts(&db, &jira, &[copy.id]).await[0].outcome, SubmitOutcome::Blocked { .. }));
        assert_eq!(jira.posts.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn manual_resolution_marks_posted_or_failed() {
        let (db, _dir, draft) = setup();
        let mut conn = db.into_inner().unwrap();
        let sub = jira_submissions::create_pending(&conn, &draft.id, &draft.issue_key, "h").unwrap();
        jira_submissions::resolve(&conn, &sub.id, JiraSubmissionState::Unknown, None).unwrap();

        assert_eq!(resolve_manually(&mut conn, &sub.id, true).unwrap(), SubmitOutcome::Posted { remote_worklog_id: None });
        assert_eq!(worklog_drafts::get_by_id(&conn, &draft.id).unwrap().unwrap().status, WorklogDraftStatus::Submitted);
        assert!(resolve_manually(&mut conn, &sub.id, false).is_err(), "повторно разрешить нельзя");
    }
}
