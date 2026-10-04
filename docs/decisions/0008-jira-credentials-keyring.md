# ADR-0008: Хранение Jira API token — крейт `keyring` (Windows Credential Manager)

**Статус:** принято (Итерация 7). Требование ТЗ FR-07.4: выбор между
`keyring` и `tauri-plugin-stronghold` зафиксировать в ADR.

## Контекст

Jira Cloud (ADR-0004) авторизуется по email + API token (Basic auth).
Токен — полноценный секрет с правами пользователя в Jira. ТЗ: "пароли в
plaintext не хранить", "секреты только в Rust-слое, никогда не передаются
во frontend". В Electron-версии планировался `safeStorage` (DPAPI);
после перехода на Tauri нужен эквивалент.

## Варианты

| | `keyring` (windows-native) | `tauri-plugin-stronghold` |
|---|---|---|
| Где лежит секрет | Windows Credential Manager (защищён DPAPI учётки Windows) | Свой зашифрованный файл-снапшот |
| Ключ шифрования | Управляет ОС, привязан к логину пользователя | Нужен пароль/ключ, который надо где-то взять — либо спрашивать у пользователя при каждом старте, либо хранить рядом (тогда смысла мало) |
| Нативные зависимости | Только Win32 API (`windows-sys`), без C-кода | IOTA Stronghold — заметно тяжелее по сборке и размеру |
| Видимость пользователю | Запись "DevLog / jira-api-token" в "Диспетчере учётных данных" — можно удалить вручную | Непрозрачный файл |

## Решение

`keyring = { version = "3", features = ["windows-native"] }`:
служба `DevLog`, учётная запись `jira-api-token`
(`src-tauri/src/integrations/jira/credentials.rs`). В SQLite (`settings`)
лежат только несекретные `jira.baseUrl` и `jira.email`.

Совместимость с `x86_64-pc-windows-msvc`: feature `windows-native`
использует `windows-sys` (чистые Win32-биндинги), без C-зависимостей —
собирается тем же `tauri build`, что и остальной проект.

## Правила обращения с токеном

- Frontend отправляет токен один раз в `jira_save_connection` и сразу
  очищает поле; ни одна команда токен не возвращает
  (`JiraConnectionStatus` — без него). Пустое поле при повторном
  сохранении = "оставить прежний токен".
- `JiraConnectionInput` имеет ручной `Debug` с `<redacted>` — токен не
  попадёт в лог даже через `{:?}`.
- "Отключить" в Settings удаляет запись из Credential Manager.
- Тест `credentials::tests::secret_round_trips_through_credential_manager`
  проверяет запись/чтение/удаление в настоящем Credential Manager под
  отдельным тестовым именем (не трогает реальный токен).

## Последствия

- Токен привязан к учётной записи Windows: другой пользователь той же
  машины его не прочитает; при переносе профиля на другую машину токен
  нужно ввести заново — для персонального приложения это приемлемо.
- OAuth 2.0 (3LO) и PAT для Data Center не реализованы; при появлении —
  тот же модуль `credentials`, другие имена записей.
