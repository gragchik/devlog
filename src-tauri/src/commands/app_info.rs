use crate::domain::tracking_status::TrackingStatus;
use serde::Serialize;
use tauri::AppHandle;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppInfo {
    pub version: String,
    pub platform: String,
    pub tracking_status_stub: TrackingStatus,
}

/// Версия приложения, платформа и заглушка статуса трекера. Вызывается и из
/// main-window, и из overlay (один и тот же Rust-процесс/состояние —
/// ТЗ: "все изменения из overlay и основного окна выполняются через один
/// сервис").
#[tauri::command]
pub fn get_app_info(app: AppHandle) -> AppInfo {
    AppInfo {
        version: app.package_info().version.to_string(),
        platform: std::env::consts::OS.to_string(),
        tracking_status_stub: TrackingStatus::Unknown,
    }
}
