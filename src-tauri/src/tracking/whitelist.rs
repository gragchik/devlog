use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use crate::db::repositories::settings;

/// Ключ в таблице `settings`, под которым хранится пользовательский
/// whitelist (JSON-массив `WhitelistEntry`). UI редактирования — Итерация 5
/// (Settings page); само хранение/чтение готово уже здесь.
pub const WHITELIST_SETTINGS_KEY: &str = "activityTracker.whitelist";

/// Одна запись белого списка приложений, учитываемых трекером (FR-02.3).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WhitelistEntry {
    /// Имя исполняемого файла, без пути, регистронезависимое сравнение
    /// (например `webstorm64.exe`).
    pub process_name: String,
    /// Свободная категория — используется как `appCategory` в
    /// `activity_events` и позже в UI. Не enum (см. комментарий в
    /// `domain::activity_event`).
    pub category: String,
}

/// Дефолтный список: WebStorm, VS Code, основные терминалы Windows
/// (ТЗ FR-02.3: "WebStorm, VS Code, терминал и вручную выбранные рабочие
/// программы"). Расширяется пользователем через `settings` (ключ
/// `activityTracker.whitelist`, JSON-массив той же формы) — хранение уже
/// готово (Итерация 1), UI для редактирования — Итерация 5.
pub fn default_whitelist() -> Vec<WhitelistEntry> {
    let ide = [
        "webstorm64.exe",
        "idea64.exe",
        "pycharm64.exe",
        "clion64.exe",
        "rider64.exe",
        "Code.exe",
    ];
    let terminal = ["WindowsTerminal.exe", "cmd.exe", "powershell.exe", "pwsh.exe"];

    ide.iter()
        .map(|&name| WhitelistEntry { process_name: name.to_string(), category: "ide".to_string() })
        .chain(
            terminal
                .iter()
                .map(|&name| WhitelistEntry { process_name: name.to_string(), category: "terminal".to_string() }),
        )
        .collect()
}

/// Пользовательский whitelist из `settings`, если он там есть и валиден;
/// иначе — `default_whitelist()`. Не падает на отсутствующем/битом JSON —
/// это настройка, а не критичные данные, молчаливый fallback безопаснее
/// падения всего трекера.
pub fn load_whitelist(conn: &Connection) -> Vec<WhitelistEntry> {
    match settings::get(conn, WHITELIST_SETTINGS_KEY) {
        Ok(Some(json)) => serde_json::from_str(&json).unwrap_or_else(|_| default_whitelist()),
        _ => default_whitelist(),
    }
}

/// Регистронезависимый поиск по имени процесса.
pub fn find_category<'a>(process_name: &str, whitelist: &'a [WhitelistEntry]) -> Option<&'a WhitelistEntry> {
    whitelist.iter().find(|entry| entry.process_name.eq_ignore_ascii_case(process_name))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_whitelisted_process_case_insensitively() {
        let list = default_whitelist();
        let found = find_category("WEBSTORM64.EXE", &list).unwrap();
        assert_eq!(found.category, "ide");
    }

    #[test]
    fn returns_none_for_unknown_process() {
        let list = default_whitelist();
        assert!(find_category("totally-unknown-app.exe", &list).is_none());
    }

    #[test]
    fn default_whitelist_is_not_empty_and_has_both_categories() {
        let list = default_whitelist();
        assert!(list.iter().any(|e| e.category == "ide"));
        assert!(list.iter().any(|e| e.category == "terminal"));
    }

    #[test]
    fn load_whitelist_falls_back_to_default_when_setting_missing() {
        let (conn, _dir) = crate::db::test_support::temp_database();
        let loaded = load_whitelist(&conn);
        assert_eq!(loaded, default_whitelist());
    }

    #[test]
    fn load_whitelist_uses_stored_override_when_present() {
        let (conn, _dir) = crate::db::test_support::temp_database();
        let custom = vec![WhitelistEntry { process_name: "myapp.exe".into(), category: "custom".into() }];
        settings::set(&conn, WHITELIST_SETTINGS_KEY, &serde_json::to_string(&custom).unwrap()).unwrap();
        assert_eq!(load_whitelist(&conn), custom);
    }

    #[test]
    fn load_whitelist_falls_back_on_corrupt_json() {
        let (conn, _dir) = crate::db::test_support::temp_database();
        settings::set(&conn, WHITELIST_SETTINGS_KEY, "not valid json").unwrap();
        assert_eq!(load_whitelist(&conn), default_whitelist());
    }
}
