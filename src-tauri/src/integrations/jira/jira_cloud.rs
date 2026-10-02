//! Jira Cloud REST API v3 (ADR-0004). Единственное место в проекте, где
//! реально ходим в сеть к Jira — всё остальное (команды, UI) работает через
//! трейт `JiraProvider`.

use async_trait::async_trait;
use chrono::{TimeZone, Utc};
use reqwest::Client;
use serde_json::json;

use super::adf::plain_text_to_adf;
use super::JiraProvider;
use crate::domain::jira::{JiraError, JiraIssueSummary, JiraWorklogResult, WorklogEntry};

pub struct JiraCloudProvider {
    base_url: String,
    email: String,
    api_token: String,
    client: Client,
}

impl JiraCloudProvider {
    pub fn new(base_url: impl Into<String>, email: impl Into<String>, api_token: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into().trim_end_matches('/').to_string(),
            email: email.into(),
            api_token: api_token.into(),
            client: Client::new(),
        }
    }
}

/// Jira worklog API ожидает `started` как `yyyy-MM-ddTHH:mm:ss.SSSZZZZ`
/// (смещение без двоеточия, напр. `+0000`) — не обычный RFC3339.
fn format_started(started_at_utc: i64) -> String {
    let dt = Utc.timestamp_opt(started_at_utc, 0).single().expect("valid unix timestamp");
    dt.format("%Y-%m-%dT%H:%M:%S%.3f+0000").to_string()
}

#[async_trait]
impl JiraProvider for JiraCloudProvider {
    async fn verify_connection(&self) -> Result<(), JiraError> {
        let url = format!("{}/rest/api/3/myself", self.base_url);
        let response = self.client.get(&url).basic_auth(&self.email, Some(&self.api_token)).send().await.map_err(|e| JiraError::NetworkUnknown(e.to_string()))?;

        if response.status().is_success() {
            Ok(())
        } else {
            let status = response.status().as_u16();
            let message = response.text().await.unwrap_or_default();
            Err(JiraError::ApiError { status, message })
        }
    }

    async fn search_issues(&self, query: &str) -> Result<Vec<JiraIssueSummary>, JiraError> {
        // Плейсхолдеры JQL экранировать не нужно — `query` подставляется как
        // query-параметр через `reqwest::RequestBuilder::query`, а не в URL
        // вручную; кавычки внутри самого JQL-выражения всё же стоит убрать,
        // чтобы не сломать синтаксис JQL.
        let jql = format!("text ~ \"{}*\" ORDER BY updated DESC", query.replace('"', "'"));
        let url = format!("{}/rest/api/3/search", self.base_url);
        let response = self
            .client
            .get(&url)
            .basic_auth(&self.email, Some(&self.api_token))
            .query(&[("jql", jql.as_str()), ("maxResults", "20"), ("fields", "summary")])
            .send()
            .await
            .map_err(|e| JiraError::NetworkUnknown(e.to_string()))?;

        let status = response.status();
        if !status.is_success() {
            let message = response.text().await.unwrap_or_default();
            return Err(JiraError::ApiError { status: status.as_u16(), message });
        }

        let parsed: serde_json::Value = response.json().await.map_err(|e| JiraError::NetworkUnknown(format!("не удалось разобрать ответ поиска: {e}")))?;
        let issues = parsed["issues"].as_array().cloned().unwrap_or_default();
        Ok(issues
            .into_iter()
            .filter_map(|issue| {
                let key = issue["key"].as_str()?.to_string();
                let title = issue["fields"]["summary"].as_str().unwrap_or("").to_string();
                Some(JiraIssueSummary { issue_key: key, title })
            })
            .collect())
    }

    async fn post_worklog(&self, entry: &WorklogEntry) -> Result<JiraWorklogResult, JiraError> {
        let url = format!("{}/rest/api/3/issue/{}/worklog", self.base_url, entry.issue_key);
        let body = json!({
            "started": format_started(entry.started_at_utc),
            "timeSpentSeconds": entry.time_spent_seconds,
            "comment": plain_text_to_adf(&entry.comment),
        });

        // FR-07.6: если `send()` вернула ошибку — соединение оборвалось/не
        // установилось до того, как мы получили хоть какой-то ответ от
        // сервера. Это и есть случай "неизвестно, принят ли запрос" —
        // `NetworkUnknown`, а не слепой retry.
        let response = self.client.post(&url).basic_auth(&self.email, Some(&self.api_token)).json(&body).send().await.map_err(|e| JiraError::NetworkUnknown(e.to_string()))?;

        let status = response.status();
        if status.is_success() {
            let parsed: serde_json::Value = response.json().await.map_err(|e| {
                // Ответ ПОЛУЧЕН с успешным статусом — запись почти наверняка
                // создана на сервере, просто тело неожиданного формата.
                // Для reconciliation безопаснее трактовать это тоже как
                // "неизвестно" (не ApiError — ошибки явно не было), чтобы
                // вызывающий код не считал попытку чисто проваленной.
                JiraError::NetworkUnknown(format!("Jira ответила {status}, но тело не разобрать (запись могла быть создана): {e}"))
            })?;
            let remote_id = parsed["id"].as_str().ok_or_else(|| JiraError::NetworkUnknown(format!("Jira ответила {status} без поля id в теле (запись могла быть создана)")))?;
            Ok(JiraWorklogResult { local_draft_id: entry.local_draft_id.clone(), remote_worklog_id: remote_id.to_string() })
        } else {
            let message = response.text().await.unwrap_or_default();
            Err(JiraError::ApiError { status: status.as_u16(), message })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn entry() -> WorklogEntry {
        WorklogEntry { issue_key: "OB-448".into(), started_at_utc: 1_700_000_000, time_spent_seconds: 3600, comment: "Работа над задачей".into(), local_draft_id: "draft-1".into() }
    }

    #[tokio::test]
    async fn verify_connection_succeeds_on_200() {
        let server = MockServer::start().await;
        Mock::given(method("GET")).and(path("/rest/api/3/myself")).respond_with(ResponseTemplate::new(200)).mount(&server).await;

        let provider = JiraCloudProvider::new(server.uri(), "user@example.com", "token");
        assert!(provider.verify_connection().await.is_ok());
    }

    #[tokio::test]
    async fn verify_connection_returns_api_error_on_401() {
        let server = MockServer::start().await;
        Mock::given(method("GET")).and(path("/rest/api/3/myself")).respond_with(ResponseTemplate::new(401).set_body_string("Unauthorized")).mount(&server).await;

        let provider = JiraCloudProvider::new(server.uri(), "user@example.com", "bad-token");
        let err = provider.verify_connection().await.unwrap_err();
        match err {
            JiraError::ApiError { status, .. } => assert_eq!(status, 401),
            other => panic!("expected ApiError, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn verify_connection_returns_network_unknown_when_server_unreachable() {
        // Ничего не слушает порт — реальная сетевая ошибка, не HTTP-ответ.
        let provider = JiraCloudProvider::new("http://127.0.0.1:1", "user@example.com", "token");
        let err = provider.verify_connection().await.unwrap_err();
        assert!(matches!(err, JiraError::NetworkUnknown(_)));
    }

    #[tokio::test]
    async fn post_worklog_succeeds_and_returns_remote_id() {
        let server = MockServer::start().await;
        Mock::given(method("POST")).and(path("/rest/api/3/issue/OB-448/worklog")).respond_with(ResponseTemplate::new(201).set_body_json(json!({ "id": "10042" }))).mount(&server).await;

        let provider = JiraCloudProvider::new(server.uri(), "user@example.com", "token");
        let result = provider.post_worklog(&entry()).await.unwrap();
        assert_eq!(result.remote_worklog_id, "10042");
        assert_eq!(result.local_draft_id, "draft-1");
    }

    #[tokio::test]
    async fn post_worklog_returns_api_error_on_400() {
        let server = MockServer::start().await;
        Mock::given(method("POST")).and(path("/rest/api/3/issue/OB-448/worklog")).respond_with(ResponseTemplate::new(400).set_body_string("Bad request")).mount(&server).await;

        let provider = JiraCloudProvider::new(server.uri(), "user@example.com", "token");
        let err = provider.post_worklog(&entry()).await.unwrap_err();
        match err {
            JiraError::ApiError { status, .. } => assert_eq!(status, 400),
            other => panic!("expected ApiError, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn post_worklog_returns_network_unknown_when_server_unreachable() {
        let provider = JiraCloudProvider::new("http://127.0.0.1:1", "user@example.com", "token");
        let err = provider.post_worklog(&entry()).await.unwrap_err();
        assert!(matches!(err, JiraError::NetworkUnknown(_)));
    }

    #[tokio::test]
    async fn search_issues_parses_key_and_summary() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/rest/api/3/search"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "issues": [
                    { "key": "OB-448", "fields": { "summary": "Массовые платежи" } },
                    { "key": "OB-419", "fields": { "summary": "Возвраты" } }
                ]
            })))
            .mount(&server)
            .await;

        let provider = JiraCloudProvider::new(server.uri(), "user@example.com", "token");
        let results = provider.search_issues("платеж").await.unwrap();
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].issue_key, "OB-448");
        assert_eq!(results[0].title, "Массовые платежи");
    }

    #[tokio::test]
    async fn search_issues_returns_empty_vec_when_no_issues_field() {
        let server = MockServer::start().await;
        Mock::given(method("GET")).and(path("/rest/api/3/search")).respond_with(ResponseTemplate::new(200).set_body_json(json!({}))).mount(&server).await;

        let provider = JiraCloudProvider::new(server.uri(), "user@example.com", "token");
        let results = provider.search_issues("платеж").await.unwrap();
        assert!(results.is_empty());
    }
}
