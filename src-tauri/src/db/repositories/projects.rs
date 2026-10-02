use rusqlite::{params, Connection, OptionalExtension, Result, Row};
use serde_json::Value;

use crate::db::ids::{generate_id, now_utc_seconds};
use crate::domain::project::{CreateProjectInput, Project, UpdateProjectInput, DEFAULT_ISSUE_REGEX};

fn row_to_project(row: &Row) -> rusqlite::Result<Project> {
    let preferences_json: String = row.get("preferencesJson")?;
    Ok(Project {
        id: row.get("id")?,
        name: row.get("name")?,
        repo_path: row.get("repoPath")?,
        enabled: row.get::<_, i64>("enabled")? == 1,
        issue_regex: row.get("issueRegex")?,
        preferences: serde_json::from_str(&preferences_json).unwrap_or(Value::Null),
        created_at_utc: row.get("createdAtUtc")?,
        updated_at_utc: row.get("updatedAtUtc")?,
    })
}

pub fn create(conn: &Connection, input: CreateProjectInput) -> Result<Project> {
    let id = generate_id();
    let now = now_utc_seconds();
    let preferences = input.preferences.unwrap_or(Value::Object(Default::default()));
    conn.execute(
        "INSERT INTO projects (id, name, repoPath, enabled, issueRegex, preferencesJson, createdAtUtc, updatedAtUtc)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?7)",
        params![
            id,
            input.name,
            input.repo_path,
            input.enabled.unwrap_or(true) as i64,
            input.issue_regex.unwrap_or_else(|| DEFAULT_ISSUE_REGEX.to_string()),
            preferences.to_string(),
            now,
        ],
    )?;
    Ok(get_by_id(conn, &id)?.expect("just inserted"))
}

pub fn get_by_id(conn: &Connection, id: &str) -> Result<Option<Project>> {
    conn.query_row("SELECT * FROM projects WHERE id = ?1", params![id], row_to_project)
        .optional()
}

pub fn get_by_repo_path(conn: &Connection, repo_path: &str) -> Result<Option<Project>> {
    conn.query_row("SELECT * FROM projects WHERE repoPath = ?1", params![repo_path], row_to_project)
        .optional()
}

pub fn list(conn: &Connection) -> Result<Vec<Project>> {
    let mut stmt = conn.prepare("SELECT * FROM projects ORDER BY name")?;
    let rows = stmt.query_map([], row_to_project)?;
    rows.collect()
}

pub fn update(conn: &Connection, id: &str, patch: UpdateProjectInput) -> Result<Project> {
    let existing = get_by_id(conn, id)?.ok_or_else(|| {
        rusqlite::Error::QueryReturnedNoRows
    })?;
    let name = patch.name.unwrap_or(existing.name);
    let enabled = patch.enabled.unwrap_or(existing.enabled);
    let issue_regex = patch.issue_regex.unwrap_or(existing.issue_regex);
    let preferences = patch.preferences.unwrap_or(existing.preferences);
    let now = now_utc_seconds();

    conn.execute(
        "UPDATE projects SET name = ?1, enabled = ?2, issueRegex = ?3, preferencesJson = ?4, updatedAtUtc = ?5 WHERE id = ?6",
        params![name, enabled as i64, issue_regex, preferences.to_string(), now, id],
    )?;
    Ok(get_by_id(conn, id)?.expect("just updated"))
}

pub fn remove(conn: &Connection, id: &str) -> Result<()> {
    conn.execute("DELETE FROM projects WHERE id = ?1", params![id])?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::test_support::temp_database;

    #[test]
    fn creates_project_with_default_issue_regex_and_enabled() {
        let (conn, _dir) = temp_database();
        let project = create(
            &conn,
            CreateProjectInput { name: "DevLog".into(), repo_path: "D:/soft/worklog".into(), enabled: None, issue_regex: None, preferences: None },
        )
        .unwrap();
        assert!(project.enabled);
        assert_eq!(project.issue_regex, DEFAULT_ISSUE_REGEX);
        assert_eq!(project.preferences, Value::Object(Default::default()));
    }

    #[test]
    fn repo_path_is_unique() {
        let (conn, _dir) = temp_database();
        create(&conn, CreateProjectInput { name: "DevLog".into(), repo_path: "D:/soft/worklog".into(), enabled: None, issue_regex: None, preferences: None }).unwrap();
        let second = create(&conn, CreateProjectInput { name: "DevLog copy".into(), repo_path: "D:/soft/worklog".into(), enabled: None, issue_regex: None, preferences: None });
        assert!(second.is_err());
    }

    #[test]
    fn update_changes_only_provided_fields() {
        let (conn, _dir) = temp_database();
        let project = create(&conn, CreateProjectInput { name: "DevLog".into(), repo_path: "D:/soft/worklog".into(), enabled: None, issue_regex: None, preferences: None }).unwrap();
        let updated = update(&conn, &project.id, UpdateProjectInput { enabled: Some(false), ..Default::default() }).unwrap();
        assert!(!updated.enabled);
        assert_eq!(updated.name, "DevLog");
        assert_eq!(updated.repo_path, "D:/soft/worklog");
    }

    #[test]
    fn list_returns_all_projects_ordered_by_name() {
        let (conn, _dir) = temp_database();
        create(&conn, CreateProjectInput { name: "B".into(), repo_path: "/b".into(), enabled: None, issue_regex: None, preferences: None }).unwrap();
        create(&conn, CreateProjectInput { name: "A".into(), repo_path: "/a".into(), enabled: None, issue_regex: None, preferences: None }).unwrap();
        let names: Vec<String> = list(&conn).unwrap().into_iter().map(|p| p.name).collect();
        assert_eq!(names, vec!["A".to_string(), "B".to_string()]);
    }

    #[test]
    fn get_by_repo_path_finds_project() {
        let (conn, _dir) = temp_database();
        let project = create(&conn, CreateProjectInput { name: "DevLog".into(), repo_path: "D:/soft/worklog".into(), enabled: None, issue_regex: None, preferences: None }).unwrap();
        assert_eq!(get_by_repo_path(&conn, "D:/soft/worklog").unwrap(), Some(project));
        assert_eq!(get_by_repo_path(&conn, "D:/does-not-exist").unwrap(), None);
    }
}
