//! Глобальный хоткей overlay (FR-06.2) — регистрация при старте по
//! сохранённому в `settings` значению (или дефолту), и живое
//! переназначение из Settings UI без перезапуска приложения.

use rusqlite::Connection;
use tauri::AppHandle;
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut};

use crate::db::repositories::settings;

pub const DEFAULT_OVERLAY_SHORTCUT: &str = "Ctrl+Alt+W";
pub const SHORTCUT_SETTINGS_KEY: &str = "overlay.shortcut";

pub fn configured_shortcut(conn: &Connection) -> String {
    settings::get(conn, SHORTCUT_SETTINGS_KEY)
        .ok()
        .flatten()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| DEFAULT_OVERLAY_SHORTCUT.to_string())
}

/// Разбирает и регистрирует `accelerator` как единственный глобальный
/// хоткей приложения. Мы никогда не регистрируем больше одного
/// одновременно, поэтому `unregister_all()` здесь безопасен — если в
/// будущем появятся другие глобальные хоткеи, это нужно будет пересмотреть
/// на точечный `unregister(старый)`.
pub fn register_overlay_shortcut(app: &AppHandle, accelerator: &str) -> Result<(), String> {
    let shortcut: Shortcut = accelerator.parse().map_err(|e| format!("invalid shortcut '{accelerator}': {e}"))?;
    let _ = app.global_shortcut().unregister_all();
    app.global_shortcut().register(shortcut).map_err(|e| e.to_string())
}
