# ADR-0004: Jira-провайдер — interface + Cloud primary + Mock

**Статус:** принято; реализация — Итерация 7.

## Контекст

ТЗ требует поддержки и Jira Cloud, и Data Center/Server через единый
`JiraProvider` interface (раздел FR-07.1), не блокируя локальный MVP, если
конкретное развёртывание пользователя не определено заранее.

## Решение

Пользователь подтвердил: основная цель — **Jira Cloud**
(REST API v3, `POST /rest/api/3/issue/{issueIdOrKey}/worklog`, комментарии в
формате Atlassian Document Format, авторизация — API token + email, Basic
auth). Data Center/Server адаптер (REST v2, PAT-токен, иной формат
комментария) — предусмотрен тем же интерфейсом, реализуется по необходимости
после Cloud, не раньше Итерации 7.

```ts
interface JiraProvider {
  testConnection(): Promise<JiraConnectionInfo>
  getIssue(issueKey: string): Promise<JiraIssueSummary>
  submitWorklog(entry: WorklogEntry): Promise<JiraWorklogResult>
  listOwnWorklogs(range: DateRange): Promise<JiraWorklogResult[]>
}
```

`JiraCloudProvider` — первая конкретная реализация.
`MockJiraProvider` — используется в unit/integration-тестах и в UI до того,
как пользователь подключит реальный Jira (чтобы Worklog Review экран можно
было разрабатывать и тестировать независимо от сетевого доступа).
`JiraDataCenterProvider` — заглушка/TODO до явного запроса.

## Последствия

- До Итерации 7 никакого реального HTTP-клиента к Jira в кодовой базе нет —
  только типы интерфейса (появятся вместе с реализацией, не раньше, чтобы не
  плодить мёртвый код в Итерации 0).
- Секреты (API token) хранятся через Electron `safeStorage`, не в SQLite
  plaintext — зафиксировано в НФТ, реализация в Итерации 7/9.
