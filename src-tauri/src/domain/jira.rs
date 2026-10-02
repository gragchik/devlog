use serde::{Deserialize, Serialize};

/// Статус отправки worklog (раздел 5 ТЗ: "jira_submissions.state").
/// `Unknown` — критично для FR-07.6: после сетевого таймаута нельзя
/// слепо повторить отправку, сервер мог уже принять POST — нужен
/// reconciliation, а не автоматический retry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum JiraSubmissionState {
    Pending,
    Posted,
    Unknown,
    Failed,
}

impl JiraSubmissionState {
    pub fn as_db_str(self) -> &'static str {
        match self {
            JiraSubmissionState::Pending => "pending",
            JiraSubmissionState::Posted => "posted",
            JiraSubmissionState::Unknown => "unknown",
            JiraSubmissionState::Failed => "failed",
        }
    }

    pub fn from_db_str(value: &str) -> Option<Self> {
        match value {
            "pending" => Some(JiraSubmissionState::Pending),
            "posted" => Some(JiraSubmissionState::Posted),
            "unknown" => Some(JiraSubmissionState::Unknown),
            "failed" => Some(JiraSubmissionState::Failed),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JiraSubmission {
    pub id: String,
    pub local_draft_id: String,
    pub issue_key: String,
    pub payload_hash: String,
    pub remote_worklog_id: Option<String>,
    pub state: JiraSubmissionState,
    pub attempted_at_utc: i64,
    pub resolved_at_utc: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CachedIssue {
    pub issue_key: String,
    pub title: String,
    pub fetched_at_utc: i64,
    pub source_provider: String,
}

/// Учётные данные соединения с Jira Cloud (FR-07.4). **Токен никогда не
/// хранится в этой структуре после загрузки с frontend** дольше, чем
/// нужно, чтобы сразу записать его в `keyring` — см.
/// `integrations::jira::credentials`.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JiraConnectionInput {
    pub base_url: String,
    pub email: String,
    pub api_token: String,
}

/// То же самое, но без токена — безопасно отдавать на frontend (FR-07.4:
/// "пароли в plaintext не хранить", токен не должен даже транзитом
/// появляться в ответах команд после первого сохранения).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JiraConnectionStatus {
    pub configured: bool,
    pub base_url: Option<String>,
    pub email: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JiraIssueSummary {
    pub issue_key: String,
    pub title: String,
}

/// Одна запись worklog, которую пользователь явно подтвердил к отправке
/// (FR-07.5: "preview... пользователь выбирает записи и явно нажимает
/// Отправить").
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorklogEntry {
    pub issue_key: String,
    pub started_at_utc: i64,
    pub time_spent_seconds: i64,
    pub comment: String,
    pub local_draft_id: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JiraWorklogResult {
    pub local_draft_id: String,
    pub remote_worklog_id: String,
}

#[derive(Debug, Clone)]
pub enum JiraError {
    /// Сеть/таймаут — **неизвестно**, принял ли сервер запрос (FR-07.6).
    NetworkUnknown(String),
    /// Jira ответила, но с ошибкой (401/403/404/429/5xx и т.п.) — точно
    /// известно, что запрос не был принят (или причина иная, не "не знаем").
    ApiError { status: u16, message: String },
    NotConfigured,
}

impl std::fmt::Display for JiraError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            JiraError::NetworkUnknown(msg) => write!(f, "сетевая ошибка (статус POST неизвестен): {msg}"),
            JiraError::ApiError { status, message } => write!(f, "Jira вернула ошибку {status}: {message}"),
            JiraError::NotConfigured => write!(f, "Jira не настроена — укажите адрес/email/токен в Settings"),
        }
    }
}

impl std::error::Error for JiraError {}
