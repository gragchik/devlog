use rusqlite::{Connection, Result};

/// Таблицы для Jira-интеграции (ADR-0004), отложенные с Итерации 1 до
/// появления реальной Jira-интеграции (Итерация 7) — раздел 5 ТЗ.
pub fn up(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "
        CREATE TABLE issues_cache (
            issueKey TEXT PRIMARY KEY,
            title TEXT NOT NULL,
            fetchedAtUtc INTEGER NOT NULL,
            sourceProvider TEXT NOT NULL
        );

        CREATE TABLE jira_submissions (
            id TEXT PRIMARY KEY,
            localDraftId TEXT NOT NULL,
            issueKey TEXT NOT NULL,
            payloadHash TEXT NOT NULL,
            remoteWorklogId TEXT,
            state TEXT NOT NULL,
            attemptedAtUtc INTEGER NOT NULL,
            resolvedAtUtc INTEGER
        );
        CREATE INDEX idx_jira_submissions_draft ON jira_submissions(localDraftId);
        CREATE INDEX idx_jira_submissions_state ON jira_submissions(state);
        ",
    )
}
