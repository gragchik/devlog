//! Хранение Jira-токена (FR-07.4: "пароли в plaintext не хранить" —
//! через `keyring` крейт, обёртку над Windows Credential Manager, а НЕ в
//! SQLite). `base_url`/`email` не секретны — живут в обычной таблице
//! `settings`.

use keyring::Entry;

use crate::db::repositories::settings;

const KEYRING_SERVICE: &str = "DevLog";
const KEYRING_TOKEN_USERNAME: &str = "jira-api-token";

pub const SETTINGS_KEY_BASE_URL: &str = "jira.baseUrl";
pub const SETTINGS_KEY_EMAIL: &str = "jira.email";

fn token_entry() -> Result<Entry, String> {
    Entry::new(KEYRING_SERVICE, KEYRING_TOKEN_USERNAME).map_err(|e| format!("keyring unavailable: {e}"))
}

pub fn store_api_token(token: &str) -> Result<(), String> {
    token_entry()?.set_password(token).map_err(|e| e.to_string())
}

pub fn load_api_token() -> Option<String> {
    token_entry().ok()?.get_password().ok()
}

pub fn delete_api_token() -> Result<(), String> {
    match token_entry()?.delete_credential() {
        Ok(()) => Ok(()),
        // Не настроено — уже отсутствует, не ошибка с точки зрения вызывающего.
        Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(e.to_string()),
    }
}

pub fn store_connection_details(conn: &rusqlite::Connection, base_url: &str, email: &str) -> rusqlite::Result<()> {
    settings::set(conn, SETTINGS_KEY_BASE_URL, base_url)?;
    settings::set(conn, SETTINGS_KEY_EMAIL, email)
}

pub fn load_connection_details(conn: &rusqlite::Connection) -> (Option<String>, Option<String>) {
    (settings::get(conn, SETTINGS_KEY_BASE_URL).ok().flatten(), settings::get(conn, SETTINGS_KEY_EMAIL).ok().flatten())
}
