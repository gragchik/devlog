//! Jira-интеграция (ТЗ раздел 7, FR-07). `JiraProvider` — граница между
//! доменной/командной логикой и конкретным HTTP-клиентом: тесты бьют по
//! `wiremock`-серверу через `JiraCloudProvider` с переопределённым
//! `base_url`, без единой реальной сети (ADR-0004: провайдер должен быть
//! заменяем — Jira Cloud сегодня, Server/DC или другой трекер потом).

mod adf;
pub mod credentials;
mod jira_cloud;
mod payload_hash;

pub use jira_cloud::JiraCloudProvider;
pub use payload_hash::compute_payload_hash;

use async_trait::async_trait;

use crate::domain::jira::{JiraError, JiraIssueSummary, JiraUser, JiraWorklogResult, RemoteWorklog, WorklogEntry};

/// Идентификатор провайдера в `issues_cache.sourceProvider`.
pub const JIRA_CLOUD_PROVIDER_ID: &str = "jira-cloud";

#[async_trait]
pub trait JiraProvider: Send + Sync {
    /// Проверка соединения и пользователя (FR-07.2) — должна фактически
    /// сходить в API, не просто проверить, что поля не пустые.
    async fn verify_connection(&self) -> Result<JiraUser, JiraError>;

    /// Название задачи по ключу (FR-07.2) — для preview и `issues_cache`.
    async fn get_issue(&self, issue_key: &str) -> Result<JiraIssueSummary, JiraError>;

    /// Worklog'и задачи с `started` в небольшом окне вокруг
    /// `started_at_utc` — remote-проверка дублей перед публикацией
    /// (FR-07.7) и reconciliation после таймаута (FR-07.6).
    async fn list_worklogs_near(&self, issue_key: &str, started_at_utc: i64) -> Result<Vec<RemoteWorklog>, JiraError>;

    /// FR-07.3/7.5: публикация одной worklog-записи. Успех — есть
    /// `remoteWorklogId`. Ошибка различает `ApiError` (точно не принято) и
    /// `NetworkUnknown` (неизвестно — см. FR-07.6, reconciliation).
    async fn post_worklog(&self, entry: &WorklogEntry) -> Result<JiraWorklogResult, JiraError>;
}
