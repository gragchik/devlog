use serde::Serialize;
use tauri::AppHandle;

/// Состояния движка трекинга (ТЗ, раздел 4). Реального Activity Tracker в
/// Итерации 0 нет — тип используется только как заглушка для сквозной
/// проверки команды `get_app_info`. Реализация — Итерация 2/4.
// Остальные варианты, кроме Unknown, пока нигде не конструируются — реальный
// движок появится в Итерации 2/4. Подавляем dead_code, а не удаляем
// варианты, чтобы тип сразу соответствовал полному контракту ТЗ.
#[allow(dead_code)]
#[derive(Serialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum TrackingStatus {
    Tracking,
    Idle,
    Paused,
    Locked,
    Suspended,
    Unknown,
}

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
