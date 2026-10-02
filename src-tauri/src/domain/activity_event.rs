use super::confidence::Confidence;
use super::tracking_status::TrackingStatus;
use serde::{Deserialize, Serialize};

/// Единичное пассивное наблюдение активности (FR-02, раздел 5
/// "activity_events"). Сырой слой, НЕ редактируется пользователем —
/// источник истины для Session Engine (Итерация 4). Заголовки окон, URL,
/// текст и т.п. сюда не попадают (FR-02.4) — хранится только то, что
/// перечислено в полях ниже.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ActivityEvent {
    pub id: String,
    pub timestamp_utc: i64,
    pub process_name_sanitized: String,
    /// Категория приложения. Конкретный набор (ide/terminal/excluded/...)
    /// определит Activity Tracker в Итерации 2 — здесь намеренно строка,
    /// не enum, чтобы не фиксировать прематурное решение.
    pub app_category: String,
    pub project_id: Option<String>,
    pub branch: Option<String>,
    pub detected_issue_key: Option<String>,
    pub idle_seconds: Option<i64>,
    pub state: TrackingStatus,
    pub confidence: Confidence,
    pub reason: String,
    pub created_at_utc: i64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateActivityEventInput {
    pub timestamp_utc: i64,
    pub process_name_sanitized: String,
    pub app_category: String,
    #[serde(default)]
    pub project_id: Option<String>,
    #[serde(default)]
    pub branch: Option<String>,
    #[serde(default)]
    pub detected_issue_key: Option<String>,
    #[serde(default)]
    pub idle_seconds: Option<i64>,
    pub state: TrackingStatus,
    pub confidence: Confidence,
    pub reason: String,
}
