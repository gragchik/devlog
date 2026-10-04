//! Хранение Jira-токена (FR-07.4: "пароли в plaintext не хранить" —
//! через `keyring` крейт, обёртку над Windows Credential Manager, а НЕ в
//! SQLite; ADR-0008). `base_url`/`email` не секретны — живут в обычной
//! таблице `settings`.

use keyring::Entry;

use crate::db::repositories::settings;

const KEYRING_SERVICE: &str = "DevLog";
const KEYRING_TOKEN_USERNAME: &str = "jira-api-token";

pub const SETTINGS_KEY_BASE_URL: &str = "jira.baseUrl";
pub const SETTINGS_KEY_EMAIL: &str = "jira.email";

fn entry(username: &str) -> Result<Entry, String> {
    Entry::new(KEYRING_SERVICE, username).map_err(|e| format!("keyring unavailable: {e}"))
}

fn store_secret(username: &str, secret: &str) -> Result<(), String> {
    entry(username)?.set_password(secret).map_err(|e| e.to_string())
}

fn load_secret(username: &str) -> Option<String> {
    entry(username).ok()?.get_password().ok()
}

fn delete_secret(username: &str) -> Result<(), String> {
    match entry(username)?.delete_credential() {
        Ok(()) => Ok(()),
        // Не настроено — уже отсутствует, не ошибка с точки зрения вызывающего.
        Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(e.to_string()),
    }
}

pub fn store_api_token(token: &str) -> Result<(), String> {
    store_secret(KEYRING_TOKEN_USERNAME, token)
}

pub fn load_api_token() -> Option<String> {
    load_secret(KEYRING_TOKEN_USERNAME)
}

pub fn delete_api_token() -> Result<(), String> {
    delete_secret(KEYRING_TOKEN_USERNAME)
}

pub fn store_connection_details(conn: &rusqlite::Connection, base_url: &str, email: &str) -> rusqlite::Result<()> {
    settings::set(conn, SETTINGS_KEY_BASE_URL, base_url)?;
    settings::set(conn, SETTINGS_KEY_EMAIL, email)
}

pub fn load_connection_details(conn: &rusqlite::Connection) -> (Option<String>, Option<String>) {
    (settings::get(conn, SETTINGS_KEY_BASE_URL).ok().flatten(), settings::get(conn, SETTINGS_KEY_EMAIL).ok().flatten())
}

pub fn clear_connection_details(conn: &rusqlite::Connection) -> rusqlite::Result<()> {
    settings::remove(conn, SETTINGS_KEY_BASE_URL)?;
    settings::remove(conn, SETTINGS_KEY_EMAIL)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Реальный Windows Credential Manager (не мок) — отдельное имя записи,
    /// чтобы тест не трогал настоящий токен пользователя.
    #[test]
    fn secret_round_trips_through_credential_manager() {
        let username = format!("test-{}", crate::db::ids::generate_id());
        store_secret(&username, "secret-token").unwrap();
        assert_eq!(load_secret(&username).as_deref(), Some("secret-token"));
        delete_secret(&username).unwrap();
        assert_eq!(load_secret(&username), None);
        // Повторное удаление — не ошибка.
        delete_secret(&username).unwrap();
    }

    /// Критерий Итерации 9: "секреты не попадают в SQLite plain fields".
    /// Полный путь сохранения подключения — и поиск токена во всём файле
    /// БД (включая WAL), а не только в ожидаемой таблице.
    #[test]
    fn saving_connection_never_puts_token_into_sqlite() {
        let (conn, dir) = crate::db::test_support::temp_database();
        let username = format!("test-{}", crate::db::ids::generate_id());
        let token = format!("ATATT-test-token-{}", crate::db::ids::generate_id());

        store_connection_details(&conn, "https://acme.atlassian.net", "dev@example.com").unwrap();
        store_secret(&username, &token).unwrap();
        drop(conn);

        for entry in std::fs::read_dir(dir.path()).unwrap() {
            let bytes = std::fs::read(entry.unwrap().path()).unwrap();
            assert!(!bytes.windows(token.len()).any(|w| w == token.as_bytes()), "токен найден в файле БД");
        }
        delete_secret(&username).unwrap();
    }
}
