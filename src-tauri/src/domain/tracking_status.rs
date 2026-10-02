use serde::{Deserialize, Serialize};

/// Состояния движка трекинга (ТЗ, раздел 4). Используется и в
/// `activity_events.state` (Итерация 1 — db), и в заглушке статуса для
/// `get_app_info` (Итерация 0). Реального Activity Tracker ещё нет —
/// появится в Итерации 2.
#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum TrackingStatus {
    Tracking,
    Idle,
    Paused,
    Locked,
    Suspended,
    Unknown,
}

impl TrackingStatus {
    /// Для хранения в SQLite (TEXT-колонка) — то же самое представление,
    /// что и serde `rename_all = "UPPERCASE"`.
    pub fn as_db_str(self) -> &'static str {
        match self {
            TrackingStatus::Tracking => "TRACKING",
            TrackingStatus::Idle => "IDLE",
            TrackingStatus::Paused => "PAUSED",
            TrackingStatus::Locked => "LOCKED",
            TrackingStatus::Suspended => "SUSPENDED",
            TrackingStatus::Unknown => "UNKNOWN",
        }
    }

    pub fn from_db_str(value: &str) -> Option<Self> {
        match value {
            "TRACKING" => Some(TrackingStatus::Tracking),
            "IDLE" => Some(TrackingStatus::Idle),
            "PAUSED" => Some(TrackingStatus::Paused),
            "LOCKED" => Some(TrackingStatus::Locked),
            "SUSPENDED" => Some(TrackingStatus::Suspended),
            "UNKNOWN" => Some(TrackingStatus::Unknown),
            _ => None,
        }
    }
}
