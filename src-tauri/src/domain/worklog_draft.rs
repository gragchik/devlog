use serde::{Deserialize, Serialize};

/// Статус черновика worklog. Факт успешной/неуспешной отправки в Jira и
/// reconciliation после таймаута живут отдельно в `jira_submissions`
/// (Итерация 7) — здесь только "это ещё черновик" / "уже отправлялся".
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum WorklogDraftStatus {
    Draft,
    Submitted,
}

impl WorklogDraftStatus {
    pub fn as_db_str(self) -> &'static str {
        match self {
            WorklogDraftStatus::Draft => "draft",
            WorklogDraftStatus::Submitted => "submitted",
        }
    }

    pub fn from_db_str(value: &str) -> Option<Self> {
        match value {
            "draft" => Some(WorklogDraftStatus::Draft),
            "submitted" => Some(WorklogDraftStatus::Submitted),
            _ => None,
        }
    }
}

/// Черновик worklog-записи за локальный день (FR-08, раздел 5
/// "worklog_drafts"). Формируется из подтверждённых `work_sessions`,
/// редактируется пользователем до отправки в Jira.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorklogDraft {
    pub id: String,
    /// `YYYY-MM-DD` в локальной таймзоне пользователя.
    pub local_day: String,
    pub issue_key: String,
    pub time_spent_seconds: i64,
    pub started_at_utc: i64,
    pub comment: Option<String>,
    /// Сессии, из которых составлен черновик — для пересчёта при их правке.
    pub selected_session_ids: Vec<String>,
    pub status: WorklogDraftStatus,
    pub updated_at_utc: i64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateWorklogDraftInput {
    pub local_day: String,
    pub issue_key: String,
    pub time_spent_seconds: i64,
    pub started_at_utc: i64,
    #[serde(default)]
    pub comment: Option<String>,
    pub selected_session_ids: Vec<String>,
    #[serde(default)]
    pub status: Option<WorklogDraftStatus>,
}

#[derive(Debug, Clone, Default)]
pub struct UpdateWorklogDraftPatch {
    pub issue_key: Option<String>,
    pub time_spent_seconds: Option<i64>,
    pub started_at_utc: Option<i64>,
    pub comment: Option<Option<String>>,
    pub selected_session_ids: Option<Vec<String>>,
    pub status: Option<WorklogDraftStatus>,
}
