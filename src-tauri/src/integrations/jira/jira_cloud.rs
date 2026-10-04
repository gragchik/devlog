//! Jira Cloud REST API v3 (ADR-0004). Единственное место в проекте, где
//! реально ходим в сеть к Jira — всё остальное (команды, UI) работает через
//! трейт `JiraProvider`.

use std::time::Duration;

use async_trait::async_trait;
use chrono::{DateTime, TimeZone, Utc};
use reqwest::{redirect, Client, RequestBuilder, Response};
use serde_json::{json, Value};

use super::adf::plain_text_to_adf;
use super::JiraProvider;
use crate::domain::jira::{JiraError, JiraIssueSummary, JiraUser, JiraWorklogResult, RemoteWorklog, WorklogEntry};

/// Без таймаута зависший запрос (VPN "наполовину" поднят) держал бы
/// команду бесконечно. Таймаут POST'а — это `NetworkUnknown`, не повод
/// для повторной отправки (FR-07.6).
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

/// Окно поиска собственных worklog'ов вокруг `started` при reconciliation
/// и remote-проверке дублей — Jira может нормализовать `started`
/// (секунды/таймзона), поэтому ищем не точное совпадение, а близкое.
const STARTED_MATCH_WINDOW_SECONDS: i64 = 60;

pub struct JiraCloudProvider {
    base_url: String,
    email: String,
    api_token: String,
    client: Client,
}

impl JiraCloudProvider {
    pub fn new(base_url: impl Into<String>, email: impl Into<String>, api_token: impl Into<String>) -> Self {
        let client = Client::builder()
            .timeout(REQUEST_TIMEOUT)
            // FR-07.10: Jira за SSO/VPN-порталом отвечает редиректом на
            // страницу логина. Следуя ему, мы получили бы 200 с HTML и
            // приняли бы это за успех — поэтому редиректы не исполняем, а
            // показываем понятную ошибку (см. `JiraError` Display для 3xx).
            .redirect(redirect::Policy::none())
            .build()
            .expect("reqwest client with static config must build");
        Self { base_url: base_url.into().trim_end_matches('/').to_string(), email: email.into(), api_token: api_token.into(), client }
    }

    fn authed(&self, builder: RequestBuilder) -> RequestBuilder {
        builder.basic_auth(&self.email, Some(&self.api_token)).header("Accept", "application/json")
    }
}

/// Jira worklog API ожидает `started` как `yyyy-MM-ddTHH:mm:ss.SSSZZZZ`
/// (смещение без двоеточия, напр. `+0000`) — не обычный RFC3339.
fn format_started(started_at_utc: i64) -> String {
    let dt = Utc.timestamp_opt(started_at_utc, 0).single().expect("valid unix timestamp");
    dt.format("%Y-%m-%dT%H:%M:%S%.3f+0000").to_string()
}

fn parse_started(value: &str) -> Option<i64> {
    DateTime::parse_from_str(value, "%Y-%m-%dT%H:%M:%S%.3f%z").ok().map(|dt| dt.timestamp())
}

/// Ошибочный HTTP-статус → `ApiError` (сервер точно ответил). Успешный —
/// отдаём ответ дальше.
async fn ensure_success(response: Response) -> Result<Response, JiraError> {
    let status = response.status();
    if status.is_success() {
        Ok(response)
    } else {
        let message = response.text().await.unwrap_or_default();
        Err(JiraError::ApiError { status: status.as_u16(), message })
    }
}

/// Тело успешного ответа, которое должно быть JSON. HTML вместо JSON —
/// признак прокси/SSO-страницы, а не Jira API.
async fn read_json(response: Response) -> Result<Value, JiraError> {
    response
        .json()
        .await
        .map_err(|e| JiraError::ApiError { status: 0, message: format!("ответ не похож на Jira REST API (прокси/SSO-страница?): {e}") })
}

fn network_error(e: reqwest::Error) -> JiraError {
    JiraError::NetworkUnknown(e.to_string())
}

#[async_trait]
impl JiraProvider for JiraCloudProvider {
    async fn verify_connection(&self) -> Result<JiraUser, JiraError> {
        let url = format!("{}/rest/api/3/myself", self.base_url);
        let response = ensure_success(self.authed(self.client.get(&url)).send().await.map_err(network_error)?).await?;
        let body = read_json(response).await?;
        let account_id = body["accountId"]
            .as_str()
            .ok_or_else(|| JiraError::ApiError { status: 0, message: "в ответе /myself нет accountId — это не Jira Cloud?".into() })?;
        Ok(JiraUser { account_id: account_id.to_string(), display_name: body["displayName"].as_str().unwrap_or(account_id).to_string() })
    }

    async fn get_issue(&self, issue_key: &str) -> Result<JiraIssueSummary, JiraError> {
        let url = format!("{}/rest/api/3/issue/{issue_key}", self.base_url);
        let request = self.authed(self.client.get(&url)).query(&[("fields", "summary")]);
        let response = ensure_success(request.send().await.map_err(network_error)?).await?;
        let body = read_json(response).await?;
        Ok(JiraIssueSummary {
            issue_key: body["key"].as_str().unwrap_or(issue_key).to_string(),
            title: body["fields"]["summary"].as_str().unwrap_or("").to_string(),
        })
    }

    async fn list_worklogs_near(&self, issue_key: &str, started_at_utc: i64) -> Result<Vec<RemoteWorklog>, JiraError> {
        // `startedAfter`/`startedBefore` — миллисекунды; узкое окно вокруг
        // интересующего `started`, чтобы не тянуть всю историю задачи.
        let after_ms = ((started_at_utc - STARTED_MATCH_WINDOW_SECONDS) * 1000).to_string();
        let before_ms = ((started_at_utc + STARTED_MATCH_WINDOW_SECONDS) * 1000).to_string();
        let url = format!("{}/rest/api/3/issue/{issue_key}/worklog", self.base_url);
        let request = self.authed(self.client.get(&url)).query(&[("startedAfter", after_ms.as_str()), ("startedBefore", before_ms.as_str()), ("maxResults", "1000")]);
        let response = ensure_success(request.send().await.map_err(network_error)?).await?;
        let body = read_json(response).await?;
        Ok(body["worklogs"]
            .as_array()
            .map(|items| {
                items
                    .iter()
                    .filter_map(|w| {
                        Some(RemoteWorklog {
                            id: w["id"].as_str()?.to_string(),
                            author_account_id: w["author"]["accountId"].as_str().map(str::to_string),
                            started_at_utc: parse_started(w["started"].as_str()?)?,
                            time_spent_seconds: w["timeSpentSeconds"].as_i64()?,
                        })
                    })
                    .collect()
            })
            .unwrap_or_default())
    }

    async fn post_worklog(&self, entry: &WorklogEntry) -> Result<JiraWorklogResult, JiraError> {
        let url = format!("{}/rest/api/3/issue/{}/worklog", self.base_url, entry.issue_key);
        let body = json!({
            "started": format_started(entry.started_at_utc),
            "timeSpentSeconds": entry.time_spent_seconds,
            "comment": plain_text_to_adf(&entry.comment),
        });

        // FR-07.8: не менять estimate задачи без явного подтверждения —
        // `adjustEstimate=leave` всегда (иначе Jira по умолчанию
        // уменьшает remaining estimate).
        let request = self.authed(self.client.post(&url)).query(&[("adjustEstimate", "leave")]).json(&body);

        // FR-07.6: если `send()` вернула ошибку — соединение оборвалось/не
        // установилось/истёк таймаут до того, как мы получили ответ. Это и
        // есть случай "неизвестно, принят ли запрос" — `NetworkUnknown`, а
        // не слепой retry.
        let response = request.send().await.map_err(network_error)?;

        let status = response.status();
        if status.is_success() {
            let parsed: Value = response.json().await.map_err(|e| {
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
    use wiremock::matchers::{body_partial_json, method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn entry() -> WorklogEntry {
        WorklogEntry { issue_key: "OB-448".into(), started_at_utc: 1_700_000_000, time_spent_seconds: 3600, comment: "Работа над задачей".into(), local_draft_id: "draft-1".into() }
    }

    #[test]
    fn started_round_trips_through_jira_format() {
        assert_eq!(format_started(1_700_000_000), "2023-11-14T22:13:20.000+0000");
        assert_eq!(parse_started("2023-11-14T22:13:20.000+0000"), Some(1_700_000_000));
        // Jira возвращает started в таймзоне профиля пользователя.
        assert_eq!(parse_started("2023-11-15T04:13:20.000+0600"), Some(1_700_000_000));
    }

    #[tokio::test]
    async fn verify_connection_returns_user_on_200() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/rest/api/3/myself"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "accountId": "acc-1", "displayName": "Dev User" })))
            .mount(&server)
            .await;

        let provider = JiraCloudProvider::new(server.uri(), "user@example.com", "token");
        let user = provider.verify_connection().await.unwrap();
        assert_eq!(user.account_id, "acc-1");
        assert_eq!(user.display_name, "Dev User");
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
    async fn verify_connection_does_not_follow_sso_redirect() {
        // FR-07.10: SSO-портал отвечает 302 на страницу логина. Если бы
        // клиент шёл по редиректу, HTML-страница с 200 выглядела бы как
        // успешное соединение.
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/rest/api/3/myself"))
            .respond_with(ResponseTemplate::new(302).insert_header("Location", format!("{}/sso/login", server.uri())))
            .mount(&server)
            .await;
        Mock::given(method("GET")).and(path("/sso/login")).respond_with(ResponseTemplate::new(200).set_body_string("<html>login</html>")).mount(&server).await;

        let provider = JiraCloudProvider::new(server.uri(), "user@example.com", "token");
        match provider.verify_connection().await.unwrap_err() {
            JiraError::ApiError { status, .. } => assert_eq!(status, 302),
            other => panic!("expected ApiError 302, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn verify_connection_rejects_html_200() {
        let server = MockServer::start().await;
        Mock::given(method("GET")).and(path("/rest/api/3/myself")).respond_with(ResponseTemplate::new(200).set_body_string("<html>proxy</html>")).mount(&server).await;

        let provider = JiraCloudProvider::new(server.uri(), "user@example.com", "token");
        assert!(matches!(provider.verify_connection().await.unwrap_err(), JiraError::ApiError { status: 0, .. }));
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
        Mock::given(method("POST"))
            .and(path("/rest/api/3/issue/OB-448/worklog"))
            .and(body_partial_json(json!({ "started": "2023-11-14T22:13:20.000+0000", "timeSpentSeconds": 3600, "comment": { "type": "doc" } })))
            .respond_with(ResponseTemplate::new(201).set_body_json(json!({ "id": "10042" })))
            .mount(&server)
            .await;

        let provider = JiraCloudProvider::new(server.uri(), "user@example.com", "token");
        let result = provider.post_worklog(&entry()).await.unwrap();
        assert_eq!(result.remote_worklog_id, "10042");
        assert_eq!(result.local_draft_id, "draft-1");
    }

    #[tokio::test]
    async fn post_worklog_always_leaves_estimate_untouched() {
        // FR-07.8: без `adjustEstimate=leave` мок не сматчится → 404 → ошибка.
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/rest/api/3/issue/OB-448/worklog"))
            .and(query_param("adjustEstimate", "leave"))
            .respond_with(ResponseTemplate::new(201).set_body_json(json!({ "id": "1" })))
            .mount(&server)
            .await;

        let provider = JiraCloudProvider::new(server.uri(), "user@example.com", "token");
        assert!(provider.post_worklog(&entry()).await.is_ok());
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
    async fn post_worklog_returns_network_unknown_when_success_body_is_garbage() {
        let server = MockServer::start().await;
        Mock::given(method("POST")).and(path("/rest/api/3/issue/OB-448/worklog")).respond_with(ResponseTemplate::new(201).set_body_string("not json")).mount(&server).await;

        let provider = JiraCloudProvider::new(server.uri(), "user@example.com", "token");
        assert!(matches!(provider.post_worklog(&entry()).await.unwrap_err(), JiraError::NetworkUnknown(_)));
    }

    #[tokio::test]
    async fn get_issue_parses_key_and_summary() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/rest/api/3/issue/OB-448"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "key": "OB-448", "fields": { "summary": "Массовые платежи" } })))
            .mount(&server)
            .await;

        let provider = JiraCloudProvider::new(server.uri(), "user@example.com", "token");
        let issue = provider.get_issue("OB-448").await.unwrap();
        assert_eq!(issue.issue_key, "OB-448");
        assert_eq!(issue.title, "Массовые платежи");
    }

    #[tokio::test]
    async fn get_issue_returns_api_error_on_404() {
        let server = MockServer::start().await;
        Mock::given(method("GET")).and(path("/rest/api/3/issue/OB-1")).respond_with(ResponseTemplate::new(404)).mount(&server).await;

        let provider = JiraCloudProvider::new(server.uri(), "user@example.com", "token");
        assert!(matches!(provider.get_issue("OB-1").await.unwrap_err(), JiraError::ApiError { status: 404, .. }));
    }

    #[tokio::test]
    async fn list_worklogs_near_parses_worklogs_in_window() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/rest/api/3/issue/OB-448/worklog"))
            .and(query_param("startedAfter", "1699999940000"))
            .and(query_param("startedBefore", "1700000060000"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "worklogs": [
                    { "id": "10042", "author": { "accountId": "acc-1" }, "started": "2023-11-14T22:13:20.000+0000", "timeSpentSeconds": 3600 },
                    { "id": "broken" }
                ]
            })))
            .mount(&server)
            .await;

        let provider = JiraCloudProvider::new(server.uri(), "user@example.com", "token");
        let worklogs = provider.list_worklogs_near("OB-448", 1_700_000_000).await.unwrap();
        assert_eq!(
            worklogs,
            vec![RemoteWorklog { id: "10042".into(), author_account_id: Some("acc-1".into()), started_at_utc: 1_700_000_000, time_spent_seconds: 3600 }]
        );
    }
}
