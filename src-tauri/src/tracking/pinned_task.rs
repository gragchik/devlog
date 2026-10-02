//! Явное закрепление текущей задачи из overlay (FR-03.4 приоритет 1,
//! FR-06.10). Хранится в `settings` — overlay вызывает
//! `commands::tracking::set_pinned_issue`, трекер читает **каждый poll**
//! (не кэширует при старте потока, в отличие от whitelist/порогов — пин
//! должен подействовать немедленно, как того требует FR-06.10
//! "немедленно завершить прежнюю сессию и начать новую").

use rusqlite::Connection;

use crate::db::repositories::settings;

pub const PINNED_ISSUE_KEY_SETTINGS_KEY: &str = "tracker.pinnedIssueKey";

pub fn get_pinned_issue_key(conn: &Connection) -> Option<String> {
    settings::get(conn, PINNED_ISSUE_KEY_SETTINGS_KEY).ok().flatten().filter(|s| !s.trim().is_empty())
}

pub fn set_pinned_issue_key(conn: &Connection, issue_key: Option<&str>) -> rusqlite::Result<()> {
    let value = issue_key.map(str::trim).filter(|s| !s.is_empty()).unwrap_or("");
    settings::set(conn, PINNED_ISSUE_KEY_SETTINGS_KEY, value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::test_support::temp_database;

    #[test]
    fn returns_none_when_not_set() {
        let (conn, _dir) = temp_database();
        assert_eq!(get_pinned_issue_key(&conn), None);
    }

    #[test]
    fn round_trips_a_value() {
        let (conn, _dir) = temp_database();
        set_pinned_issue_key(&conn, Some("OB-448")).unwrap();
        assert_eq!(get_pinned_issue_key(&conn), Some("OB-448".to_string()));
    }

    #[test]
    fn clearing_with_none_makes_it_absent_again() {
        let (conn, _dir) = temp_database();
        set_pinned_issue_key(&conn, Some("OB-448")).unwrap();
        set_pinned_issue_key(&conn, None).unwrap();
        assert_eq!(get_pinned_issue_key(&conn), None);
    }

    #[test]
    fn whitespace_only_value_is_treated_as_absent() {
        let (conn, _dir) = temp_database();
        set_pinned_issue_key(&conn, Some("   ")).unwrap();
        assert_eq!(get_pinned_issue_key(&conn), None);
    }

    #[test]
    fn trims_surrounding_whitespace() {
        let (conn, _dir) = temp_database();
        set_pinned_issue_key(&conn, Some("  OB-448  ")).unwrap();
        assert_eq!(get_pinned_issue_key(&conn), Some("OB-448".to_string()));
    }
}
