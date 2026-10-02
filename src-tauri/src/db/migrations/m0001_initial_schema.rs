use rusqlite::{Connection, Result};

/// Базовая схема (раздел 5 ТЗ). Намеренно не включает `issues_cache` и
/// `jira_submissions` — появятся в миграции Итерации 7 вместе с Jira-
/// интеграцией (ADR-0004), чтобы не фиксировать сейчас структуру, которая
/// может уточниться на практике.
///
/// Все временные метки — INTEGER, эпоха в секундах UTC ("секунды —
/// основной формат хранения", ТЗ раздел 4). `timezoneId` хранится отдельно
/// (IANA, например `Europe/Bishkek`) для корректного деления по локальным
/// суткам — сама логика деления по полуночи появится в Session Engine
/// (Итерация 4).
pub fn up(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "
        CREATE TABLE projects (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            repoPath TEXT NOT NULL UNIQUE,
            enabled INTEGER NOT NULL DEFAULT 1,
            issueRegex TEXT NOT NULL,
            preferencesJson TEXT NOT NULL DEFAULT '{}',
            createdAtUtc INTEGER NOT NULL,
            updatedAtUtc INTEGER NOT NULL
        );

        CREATE TABLE activity_events (
            id TEXT PRIMARY KEY,
            timestampUtc INTEGER NOT NULL,
            processNameSanitized TEXT NOT NULL,
            appCategory TEXT NOT NULL,
            projectId TEXT REFERENCES projects(id),
            branch TEXT,
            detectedIssueKey TEXT,
            idleSeconds INTEGER,
            state TEXT NOT NULL,
            confidence TEXT NOT NULL,
            reason TEXT NOT NULL,
            createdAtUtc INTEGER NOT NULL
        );
        CREATE INDEX idx_activity_events_timestamp ON activity_events(timestampUtc);
        CREATE INDEX idx_activity_events_project ON activity_events(projectId);

        CREATE TABLE work_sessions (
            id TEXT PRIMARY KEY,
            startedAtUtc INTEGER NOT NULL,
            endedAtUtc INTEGER,
            timezoneId TEXT NOT NULL,
            projectId TEXT REFERENCES projects(id),
            issueKey TEXT,
            activeSeconds INTEGER NOT NULL,
            manualSecondsOverride INTEGER,
            description TEXT,
            source TEXT NOT NULL,
            confidence TEXT NOT NULL,
            reviewStatus TEXT NOT NULL,
            isManuallyEdited INTEGER NOT NULL DEFAULT 0,
            deletedAt INTEGER,
            createdAtUtc INTEGER NOT NULL,
            updatedAtUtc INTEGER NOT NULL,
            CHECK (endedAtUtc IS NULL OR endedAtUtc > startedAtUtc)
        );
        CREATE INDEX idx_work_sessions_started ON work_sessions(startedAtUtc);
        CREATE INDEX idx_work_sessions_project ON work_sessions(projectId);

        CREATE TABLE session_edits (
            id TEXT PRIMARY KEY,
            sessionId TEXT NOT NULL REFERENCES work_sessions(id),
            editedAtUtc INTEGER NOT NULL,
            operation TEXT NOT NULL,
            previousStateJson TEXT NOT NULL,
            nextStateJson TEXT NOT NULL
        );
        CREATE INDEX idx_session_edits_session ON session_edits(sessionId);
        CREATE INDEX idx_session_edits_edited_at ON session_edits(editedAtUtc);

        CREATE TABLE worklog_drafts (
            id TEXT PRIMARY KEY,
            localDay TEXT NOT NULL,
            issueKey TEXT NOT NULL,
            timeSpentSeconds INTEGER NOT NULL,
            startedAtUtc INTEGER NOT NULL,
            comment TEXT,
            selectedSessionIdsJson TEXT NOT NULL DEFAULT '[]',
            status TEXT NOT NULL,
            updatedAtUtc INTEGER NOT NULL
        );
        CREATE INDEX idx_worklog_drafts_local_day ON worklog_drafts(localDay);

        CREATE TABLE settings (
            key TEXT PRIMARY KEY,
            value TEXT NOT NULL
        );
        ",
    )
}
