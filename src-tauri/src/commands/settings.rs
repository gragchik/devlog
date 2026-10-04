use tauri::{AppHandle, State};
use tauri_plugin_autostart::ManagerExt;

use crate::db::app_database::AppDatabase;
use crate::db::repositories::settings;
use crate::tracking::tracker::{
    DEFAULT_IDLE_THRESHOLD_SECONDS, DEFAULT_POLL_INTERVAL_SECONDS, SETTINGS_KEY_IDLE_THRESHOLD, SETTINGS_KEY_POLL_INTERVAL,
};
use crate::tracking::whitelist::{WhitelistEntry, WHITELIST_SETTINGS_KEY};

#[tauri::command]
pub fn get_whitelist(db: State<AppDatabase>) -> Result<Vec<WhitelistEntry>, String> {
    let conn = db.lock();
    Ok(crate::tracking::whitelist::load_whitelist(&conn))
}

#[tauri::command]
pub fn set_whitelist(db: State<AppDatabase>, entries: Vec<WhitelistEntry>) -> Result<(), String> {
    let conn = db.lock();
    let json = serde_json::to_string(&entries).map_err(|e| e.to_string())?;
    settings::set(&conn, WHITELIST_SETTINGS_KEY, &json).map_err(|e| e.to_string())
}

/// Пороги трекера. Отдаются/принимаются вместе — на Settings-странице это
/// одна форма. Изменение требует перезапуска приложения, чтобы вступить в
/// силу (трекер читает их один раз при старте потока, см. `tracker.rs`) —
/// UI обязан явно предупредить об этом.
#[derive(serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrackerThresholds {
    pub idle_threshold_seconds: u64,
    pub poll_interval_seconds: u64,
}

#[tauri::command]
pub fn get_tracker_thresholds(db: State<AppDatabase>) -> Result<TrackerThresholds, String> {
    let conn = db.lock();
    let read = |key: &str, default: u64| {
        settings::get(&conn, key).ok().flatten().and_then(|v| v.parse::<u64>().ok()).filter(|v| *v > 0).unwrap_or(default)
    };
    Ok(TrackerThresholds {
        idle_threshold_seconds: read(SETTINGS_KEY_IDLE_THRESHOLD, DEFAULT_IDLE_THRESHOLD_SECONDS),
        poll_interval_seconds: read(SETTINGS_KEY_POLL_INTERVAL, DEFAULT_POLL_INTERVAL_SECONDS),
    })
}

#[tauri::command]
pub fn set_tracker_thresholds(db: State<AppDatabase>, thresholds: TrackerThresholds) -> Result<(), String> {
    if thresholds.idle_threshold_seconds == 0 || thresholds.poll_interval_seconds == 0 {
        return Err("thresholds must be positive".to_string());
    }
    let conn = db.lock();
    settings::set(&conn, SETTINGS_KEY_IDLE_THRESHOLD, &thresholds.idle_threshold_seconds.to_string()).map_err(|e| e.to_string())?;
    settings::set(&conn, SETTINGS_KEY_POLL_INTERVAL, &thresholds.poll_interval_seconds.to_string()).map_err(|e| e.to_string())
}

/// FR-01.3: "запуск вместе с Windows — опционально и выключен по умолчанию".
#[tauri::command]
pub fn get_autostart_enabled(app: AppHandle) -> Result<bool, String> {
    app.autolaunch().is_enabled().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn set_autostart_enabled(app: AppHandle, enabled: bool) -> Result<(), String> {
    let manager = app.autolaunch();
    if enabled {
        manager.enable().map_err(|e| e.to_string())
    } else {
        manager.disable().map_err(|e| e.to_string())
    }
}
