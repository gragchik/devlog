//! Jira-интеграция (ТЗ раздел 7, FR-07). `JiraProvider` — граница между
//! доменной/командной логикой и конкретным HTTP-клиентом: тесты бьют по
//! `wiremock`-серверу через `JiraCloudProvider` с переопределённым
//! `base_url`, без единой реальной сети (ADR-0004: провайдер должен быть
//! заменяем — Jira Cloud сегодня, Server/DC или другой трекер потом).

mod adf;
pub mod credentials;
mod jira_cloud;
mod payload_hash;

// Пока не используются ни одной командой (это ещё будет сделано в
// `commands/jira.rs`, следующий шаг Итерации 7) — без `#[allow(dead_code)]`
// компилятор ругается на неиспользуемый `pub use`, хотя сами модули (и их
// тесты) уже полноценно работают.
#[allow(unused_imports)]
pub use jira_cloud::JiraCloudProvider;
#[allow(unused_imports)]
pub use payload_hash::compute_payload_hash;

use async_trait::async_trait;

use crate::domain::jira::{JiraError, JiraIssueSummary, JiraWorklogResult, WorklogEntry};

#[async_trait]
pub trait JiraProvider: Send + Sync {
    /// Проверка соединения (FR-07.4: "Проверить соединение" в Settings) —
    /// должна фактически сходить в API, не просто проверить, что поля не
    /// пустые.
    async fn verify_connection(&self) -> Result<(), JiraError>;

    /// Поиск issue по ключу или JQL-подстроке — для автокомплита при
    /// составлении черновика (FR-07.2).
    async fn search_issues(&self, query: &str) -> Result<Vec<JiraIssueSummary>, JiraError>;

    /// FR-07.3/7.5: публикация одной worklog-записи. Успех — есть
    /// `remoteWorklogId`. Ошибка различает `ApiError` (точно не принято) и
    /// `NetworkUnknown` (неизвестно — см. FR-07.6, reconciliation).
    async fn post_worklog(&self, entry: &WorklogEntry) -> Result<JiraWorklogResult, JiraError>;
}
