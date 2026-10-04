use rusqlite::{params, Connection, OptionalExtension, Result, Row};

use crate::db::ids::now_utc_seconds;
use crate::domain::jira::CachedIssue;

fn row_to_issue(row: &Row) -> rusqlite::Result<CachedIssue> {
    Ok(CachedIssue { issue_key: row.get("issueKey")?, title: row.get("title")?, fetched_at_utc: row.get("fetchedAtUtc")?, source_provider: row.get("sourceProvider")? })
}

/// Локальный кэш названий задач (ТЗ раздел 5 "issues_cache") — чтобы в
/// Worklog Review показывать человекочитаемое название issue без похода в
/// сеть на каждый рендер, и чтобы preview оставался рабочим офлайн (FR-07.5
/// подразумевает возможность посмотреть черновики без сети).
pub fn upsert(conn: &Connection, issue_key: &str, title: &str, source_provider: &str) -> Result<CachedIssue> {
    conn.execute(
        "INSERT INTO issues_cache (issueKey, title, fetchedAtUtc, sourceProvider) VALUES (?1, ?2, ?3, ?4)
         ON CONFLICT(issueKey) DO UPDATE SET title = excluded.title, fetchedAtUtc = excluded.fetchedAtUtc, sourceProvider = excluded.sourceProvider",
        params![issue_key, title, now_utc_seconds(), source_provider],
    )?;
    Ok(get(conn, issue_key)?.expect("just upserted"))
}

pub fn get(conn: &Connection, issue_key: &str) -> Result<Option<CachedIssue>> {
    conn.query_row("SELECT * FROM issues_cache WHERE issueKey = ?1", params![issue_key], row_to_issue).optional()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::test_support::temp_database;

    #[test]
    fn upsert_then_get_round_trips() {
        let (conn, _dir) = temp_database();
        upsert(&conn, "OB-448", "Массовые платежи", "jira-cloud").unwrap();
        let cached = get(&conn, "OB-448").unwrap().unwrap();
        assert_eq!(cached.title, "Массовые платежи");
    }

    #[test]
    fn upsert_overwrites_title_on_conflict() {
        let (conn, _dir) = temp_database();
        upsert(&conn, "OB-448", "Старое название", "jira-cloud").unwrap();
        upsert(&conn, "OB-448", "Новое название", "jira-cloud").unwrap();
        let cached = get(&conn, "OB-448").unwrap().unwrap();
        assert_eq!(cached.title, "Новое название");
    }

    #[test]
    fn get_returns_none_for_unknown_key() {
        let (conn, _dir) = temp_database();
        assert_eq!(get(&conn, "MISSING-1").unwrap(), None);
    }
}
