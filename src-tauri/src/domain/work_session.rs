use super::confidence::Confidence;
use serde::{Deserialize, Serialize};

/// Как сессия была создана (FR-04.3 "источник").
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum WorkSessionSource {
    Detected,
    Manual,
}

impl WorkSessionSource {
    pub fn as_db_str(self) -> &'static str {
        match self {
            WorkSessionSource::Detected => "detected",
            WorkSessionSource::Manual => "manual",
        }
    }

    pub fn from_db_str(value: &str) -> Option<Self> {
        match value {
            "detected" => Some(WorkSessionSource::Detected),
            "manual" => Some(WorkSessionSource::Manual),
            _ => None,
        }
    }
}

/// Статус разбора задачи для сессии (FR-04.7).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum WorkSessionReviewStatus {
    Detected,
    Unassigned,
    Reviewed,
    Excluded,
}

impl WorkSessionReviewStatus {
    pub fn as_db_str(self) -> &'static str {
        match self {
            WorkSessionReviewStatus::Detected => "detected",
            WorkSessionReviewStatus::Unassigned => "unassigned",
            WorkSessionReviewStatus::Reviewed => "reviewed",
            WorkSessionReviewStatus::Excluded => "excluded",
        }
    }

    pub fn from_db_str(value: &str) -> Option<Self> {
        match value {
            "detected" => Some(WorkSessionReviewStatus::Detected),
            "unassigned" => Some(WorkSessionReviewStatus::Unassigned),
            "reviewed" => Some(WorkSessionReviewStatus::Reviewed),
            "excluded" => Some(WorkSessionReviewStatus::Excluded),
            _ => None,
        }
    }
}

/// Тип операции в журнале правок (`session_edits.operation`). Полный набор
/// соответствует операциям из FR-04/FR-06; на Итерации 1 репозитории
/// реально производят только `Create`/`Update`/`Delete`/`Restore`/`Undo` —
/// `Split`/`Merge`/`Exclude` появятся вместе с Session Engine/UI
/// (Итерации 4–6), вариант объявлен полностью заранее, чтобы не
/// мигрировать схему позже.
#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SessionEditOperation {
    Create,
    Update,
    Split,
    Merge,
    Exclude,
    Restore,
    Delete,
    Undo,
}

impl SessionEditOperation {
    pub fn as_db_str(self) -> &'static str {
        match self {
            SessionEditOperation::Create => "create",
            SessionEditOperation::Update => "update",
            SessionEditOperation::Split => "split",
            SessionEditOperation::Merge => "merge",
            SessionEditOperation::Exclude => "exclude",
            SessionEditOperation::Restore => "restore",
            SessionEditOperation::Delete => "delete",
            SessionEditOperation::Undo => "undo",
        }
    }

    pub fn from_db_str(value: &str) -> Option<Self> {
        match value {
            "create" => Some(SessionEditOperation::Create),
            "update" => Some(SessionEditOperation::Update),
            "split" => Some(SessionEditOperation::Split),
            "merge" => Some(SessionEditOperation::Merge),
            "exclude" => Some(SessionEditOperation::Exclude),
            "restore" => Some(SessionEditOperation::Restore),
            "delete" => Some(SessionEditOperation::Delete),
            "undo" => Some(SessionEditOperation::Undo),
            _ => None,
        }
    }
}

/// Редактируемая рабочая сессия (FR-04, раздел 5 "work_sessions"). В
/// отличие от `ActivityEvent` — то, что показывается и правится в
/// UI/overlay. `ended_at_utc == None` означает текущую незакрытую сессию.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkSession {
    pub id: String,
    pub started_at_utc: i64,
    pub ended_at_utc: Option<i64>,
    pub timezone_id: String,
    pub project_id: Option<String>,
    pub issue_key: Option<String>,
    pub active_seconds: i64,
    pub manual_seconds_override: Option<i64>,
    pub description: Option<String>,
    pub source: WorkSessionSource,
    pub confidence: Confidence,
    pub review_status: WorkSessionReviewStatus,
    /// FR-04.3: признак ручной корректировки — отличает от "как было обнаружено".
    pub is_manually_edited: bool,
    pub deleted_at: Option<i64>,
    pub created_at_utc: i64,
    pub updated_at_utc: i64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateWorkSessionInput {
    pub started_at_utc: i64,
    pub ended_at_utc: Option<i64>,
    pub timezone_id: String,
    pub active_seconds: i64,
    pub source: WorkSessionSource,
    #[serde(default)]
    pub project_id: Option<String>,
    #[serde(default)]
    pub issue_key: Option<String>,
    #[serde(default)]
    pub manual_seconds_override: Option<i64>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub confidence: Option<Confidence>,
    #[serde(default)]
    pub review_status: Option<WorkSessionReviewStatus>,
    #[serde(default)]
    pub is_manually_edited: Option<bool>,
}

/// Патч для `WorkSessionsRepository::update`. Поля-`Option<Option<T>>`
/// различают "не трогать" (`None`) от "явно обнулить" (`Some(None)`) —
/// намеренно **не** `#[derive(Deserialize)]`: serde по умолчанию не
/// различает "поле отсутствует" и "поле равно `null`" без отдельного
/// `deserialize_with`-хака. Когда это понадобится для реальной IPC-команды
/// (Итерация 5+), добавить его туда же, а не раньше.
#[derive(Debug, Clone, Default)]
pub struct UpdateWorkSessionPatch {
    pub started_at_utc: Option<i64>,
    pub ended_at_utc: Option<Option<i64>>,
    pub project_id: Option<Option<String>>,
    pub issue_key: Option<Option<String>>,
    pub active_seconds: Option<i64>,
    pub manual_seconds_override: Option<Option<i64>>,
    pub description: Option<Option<String>>,
    pub confidence: Option<Confidence>,
    pub review_status: Option<WorkSessionReviewStatus>,
}
