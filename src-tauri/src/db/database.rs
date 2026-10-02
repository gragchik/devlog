use rusqlite::{Connection, Result};
use std::path::Path;

use super::migrations;

/// Открывает (или создаёт) файл SQLite, включает WAL и внешние ключи,
/// применяет миграции. Единая точка создания соединения — используется и
/// настоящим приложением (`app_database.rs`, путь внутри app data dir), и
/// тестами (временный файл на диске).
///
/// WAL даёт устойчивость к падению процесса посреди записи (НФТ "atomic
/// writes, recovery при crash") — SQLite восстанавливает consistent-состояние
/// из WAL-журнала при следующем открытии файла, без ручного кода.
pub fn create_database<P: AsRef<Path>>(path: P) -> Result<Connection> {
    let mut conn = Connection::open(path)?;
    conn.pragma_update(None, "journal_mode", "WAL")?;
    conn.pragma_update(None, "foreign_keys", "ON")?;
    migrations::run(&mut conn)?;
    Ok(conn)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::repositories::projects;
    use crate::domain::project::CreateProjectInput;
    use tempfile::TempDir;

    #[test]
    fn creates_all_expected_tables() {
        let dir = TempDir::new().unwrap();
        let conn = create_database(dir.path().join("db.sqlite3")).unwrap();

        let mut stmt = conn.prepare("SELECT name FROM sqlite_master WHERE type = 'table' ORDER BY name").unwrap();
        let tables: Vec<String> = stmt.query_map([], |row| row.get(0)).unwrap().collect::<rusqlite::Result<Vec<_>>>().unwrap();

        assert_eq!(
            tables,
            vec![
                "_migrations".to_string(),
                "activity_events".to_string(),
                "issues_cache".to_string(),
                "jira_submissions".to_string(),
                "projects".to_string(),
                "session_edits".to_string(),
                "settings".to_string(),
                "work_sessions".to_string(),
                "worklog_drafts".to_string(),
            ]
        );
    }

    #[test]
    fn enables_wal_mode() {
        let dir = TempDir::new().unwrap();
        let conn = create_database(dir.path().join("db.sqlite3")).unwrap();
        let mode: String = conn.pragma_query_value(None, "journal_mode", |row| row.get(0)).unwrap();
        assert_eq!(mode.to_lowercase(), "wal");
    }

    #[test]
    fn reopening_does_not_reapply_migrations_or_lose_data_restart_scenario() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("db.sqlite3");

        let conn1 = create_database(&path).unwrap();
        let created = projects::create(&conn1, CreateProjectInput { name: "DevLog".into(), repo_path: "D:/soft/worklog".into(), enabled: None, issue_regex: None, preferences: None }).unwrap();
        drop(conn1); // закрыли соединение — имитация остановки процесса

        // "Перезапуск" — новое соединение на том же файле, как после рестарта приложения.
        let conn2 = create_database(&path).unwrap();
        let applied: i64 = conn2.query_row("SELECT COUNT(*) FROM _migrations", [], |row| row.get(0)).unwrap();
        assert_eq!(applied, crate::db::migrations::all().len() as i64); // миграции не применились повторно

        let reloaded = projects::get_by_id(&conn2, &created.id).unwrap();
        assert_eq!(reloaded, Some(created));
    }

    #[test]
    fn check_constraint_rejects_ended_at_not_after_started_at_at_the_sqlite_level() {
        let dir = TempDir::new().unwrap();
        let conn = create_database(dir.path().join("db.sqlite3")).unwrap();
        let result = conn.execute(
            "INSERT INTO work_sessions
               (id, startedAtUtc, endedAtUtc, timezoneId, activeSeconds, source, confidence, reviewStatus, isManuallyEdited, createdAtUtc, updatedAtUtc)
             VALUES ('x', 1000, 1000, 'UTC', 0, 'manual', 'high', 'reviewed', 0, 1000, 1000)",
            [],
        );
        assert!(result.is_err());
    }
}
