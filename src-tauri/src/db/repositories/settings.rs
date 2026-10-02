use rusqlite::{params, Connection, OptionalExtension, Result};
use std::collections::HashMap;

/// Простое key-value хранилище настроек (раздел 5: "settings: key,
/// value"). Чувствительные значения (Jira-токены и т.п.) здесь НЕ
/// хранятся — для них предусмотрено отдельное зашифрованное хранилище
/// (Итерация 7/9), а не эта таблица.
pub fn get(conn: &Connection, key: &str) -> Result<Option<String>> {
    conn.query_row("SELECT value FROM settings WHERE key = ?1", params![key], |row| row.get(0))
        .optional()
}

pub fn set(conn: &Connection, key: &str, value: &str) -> Result<()> {
    conn.execute(
        "INSERT INTO settings (key, value) VALUES (?1, ?2)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        params![key, value],
    )?;
    Ok(())
}

/// Не выведено ни в одну IPC-команду в Итерации 5 (нет "экспорт всех
/// настроек" в UI) — оставлено как готовый инструмент для Итерации 9
/// (экспорт/удаление данных).
#[allow(dead_code)]
pub fn get_all(conn: &Connection) -> Result<HashMap<String, String>> {
    let mut stmt = conn.prepare("SELECT key, value FROM settings")?;
    let rows = stmt.query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)))?;
    rows.collect()
}

/// Не используется напрямую командами — настройки в этой итерации только
/// перезаписываются (`set`), явного "сбросить к дефолту" в UI нет.
#[allow(dead_code)]
pub fn remove(conn: &Connection, key: &str) -> Result<()> {
    conn.execute("DELETE FROM settings WHERE key = ?1", params![key])?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::test_support::temp_database;

    #[test]
    fn get_returns_none_for_unknown_key() {
        let (conn, _dir) = temp_database();
        assert_eq!(get(&conn, "missing").unwrap(), None);
    }

    #[test]
    fn set_then_get_round_trips_and_overwrites() {
        let (conn, _dir) = temp_database();
        set(&conn, "idleThresholdSeconds", "180").unwrap();
        assert_eq!(get(&conn, "idleThresholdSeconds").unwrap(), Some("180".to_string()));
        set(&conn, "idleThresholdSeconds", "240").unwrap();
        assert_eq!(get(&conn, "idleThresholdSeconds").unwrap(), Some("240".to_string()));
    }

    #[test]
    fn get_all_returns_every_key() {
        let (conn, _dir) = temp_database();
        set(&conn, "a", "1").unwrap();
        set(&conn, "b", "2").unwrap();
        let all = get_all(&conn).unwrap();
        assert_eq!(all.get("a"), Some(&"1".to_string()));
        assert_eq!(all.get("b"), Some(&"2".to_string()));
    }

    #[test]
    fn remove_deletes_key() {
        let (conn, _dir) = temp_database();
        set(&conn, "a", "1").unwrap();
        remove(&conn, "a").unwrap();
        assert_eq!(get(&conn, "a").unwrap(), None);
    }
}
