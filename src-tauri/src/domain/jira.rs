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
#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JiraConnectionInput {
    pub base_url: String,
    pub email: String,
    pub api_token: String,
}

/// Ручной `Debug` вместо derive — токен не должен попасть в логи даже
/// случайным `{:?}` (NFR раздел 7: "безопасное логирование с redaction").
impl std::fmt::Debug for JiraConnectionInput {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("JiraConnectionInput").field("base_url", &self.base_url).field("email", &self.email).field("api_token", &"<redacted>").finish()
    }
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

/// Результат "Проверить соединение" (FR-07.2: "проверить соединение и
/// пользователя"). `account_id` нужен reconciliation — чтобы среди
/// worklog'ов задачи искать только свои.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JiraUser {
    pub account_id: String,
    pub display_name: String,
}

/// Worklog, уже существующий в Jira (FR-07.6/7.7: remote-проверка дублей
/// и reconciliation после таймаута).
#[derive(Debug, Clone, PartialEq)]
pub struct RemoteWorklog {
    pub id: String,
    pub author_account_id: Option<String>,
    pub started_at_utc: i64,
    pub time_spent_seconds: i64,
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
    /// Jira ответила, но с ошибкой (3xx/401/403/404/429/5xx и т.п.) — точно
    /// известно, что запрос не был принят (или причина иная, не "не знаем").
    /// `status = 0` — ответ получен, но не является ответом Jira API
    /// (например, HTML вместо JSON у `myself`).
    ApiError { status: u16, message: String },
    NotConfigured,
}

/// Тело ответа Jira может быть огромным (HTML SSO-страницы) — в UI и логи
/// идёт только начало.
const MAX_ERROR_BODY_CHARS: usize = 300;

/// FR-07.10: понятная ошибка для частых причин (VPN/SSO, токен, права)
/// вместо голого HTTP-кода.
fn api_error_hint(status: u16) -> &'static str {
    match status {
        300..=399 => "Jira перенаправила запрос — вероятно, она закрыта SSO/VPN-порталом. Подключитесь к VPN и проверьте адрес Jira",
        401 => "неверный email или API token",
        403 => "нет прав на это действие в Jira (или доступ к API запрещён политикой организации)",
        404 => "не найдено — проверьте ключ задачи и что у вас есть к ней доступ",
        429 => "слишком много запросов к Jira, повторите позже",
        500..=599 => "Jira временно недоступна",
        _ => "запрос отклонён Jira",
    }
}

impl std::fmt::Display for JiraError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            JiraError::NetworkUnknown(msg) => write!(f, "Jira недоступна или не ответила (проверьте сеть/VPN): {msg}"),
            JiraError::ApiError { status, message } => {
                let body: String = message.chars().take(MAX_ERROR_BODY_CHARS).collect();
                write!(f, "Jira {status}: {}", api_error_hint(*status))?;
                if !body.trim().is_empty() {
                    write!(f, " — {}", body.trim())?;
                }
                Ok(())
            }
            JiraError::NotConfigured => write!(f, "Jira не настроена — укажите адрес/email/токен в Settings"),
        }
    }
}

impl std::error::Error for JiraError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn connection_input_debug_never_prints_token() {
        let input = JiraConnectionInput { base_url: "https://a.atlassian.net".into(), email: "e@x".into(), api_token: "super-secret".into() };
        assert!(!format!("{input:?}").contains("super-secret"));
    }

    #[test]
    fn redirect_status_explains_sso_vpn() {
        let text = JiraError::ApiError { status: 302, message: String::new() }.to_string();
        assert!(text.contains("SSO/VPN"), "{text}");
    }

    #[test]
    fn long_error_body_is_truncated() {
        let text = JiraError::ApiError { status: 500, message: "x".repeat(10_000) }.to_string();
        assert!(text.chars().count() < MAX_ERROR_BODY_CHARS + 100);
    }
}
