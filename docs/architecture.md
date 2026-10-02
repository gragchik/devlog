# Архитектура DevLog (Tauri)

Этот документ описывает принятые структурные решения для Tauri-реализации
(заменяет прежнюю Electron-версию — см. историю в git и
`docs/*/electron-archive/`). Детали отдельных технических выборов —
`docs/decisions/*.md` (ADR). Статус по итерациям — `docs/iterations/*.md`.

## Процесс и окна

В отличие от Electron (main-процесс + по процессу на каждый renderer),
**у Tauri один процесс** на всё приложение: Rust-ядро и все
webview-окна (через системный WebView2 на Windows) работают в одном
процессе ОС. Это упрощает модель состояния (не нужен IPC для доступа к
"серверному" состоянию — оно просто в памяти Rust-процесса, доступно через
`tauri::State`/`AppHandle`), но означает, что падение Rust-кода валит всё
приложение целиком (ещё один довод писать Rust-код defensively: `Result`,
не `unwrap()` в коде, который может быть вызван из runtime-путей).

- **Main window** (`label: "main"`, объявлено декларативно в
  `tauri.conf.json`) — создаётся автоматически при старте. Закрытие (`X`)
  не завершает процесс: `on_window_event` в `lib.rs` перехватывает
  `WindowEvent::CloseRequested`, вызывает `api.prevent_close()` и
  `window.hide()` вместо реального закрытия (FR-01.1).
- **Overlay window** (`label: "overlay"`, создаётся программно в
  `windows/overlay_window.rs::ensure_overlay_window`) — `decorations(false)`,
  `always_on_top(true)`, `skip_taskbar(true)`, создаётся один раз при
  `setup()` и держится скрытым (`visible(false)` в билдере + `hide()`
  вместо `close()`), что даёт быстрое повторное открытие без пересоздания
  окна (FR-06.4).
- **Tray** (`tray/mod.rs`) — создаётся через `TrayIconBuilder` в Rust (не
  декларативно в `tauri.conf.json` — нужна динамика: пункт
  "Приостановить/Продолжить" меняет текст через `MenuItem::set_text`).
  Иконка встроена в бинарник через `include_bytes!` на этапе компиляции —
  не нужно резолвить путь к файлу в рантайме (в отличие от Electron, где
  путь к иконке трея приходилось резолвить относительно `__dirname`,
  что ломается по-разному в dev и после упаковки).

## Capabilities вместо IPC whitelist

Electron-версия использовала `contextBridge` + явный whitelist каналов в
`ipcMain.handle`. У Tauri другая модель:

- **Собственные команды приложения** (`#[tauri::command]`, например
  `get_app_info` в `commands/app_info.rs`) вызываются из frontend через
  `invoke('get_app_info')` (`@tauri-apps/api/core`) и **не гейтятся**
  permission-системой — её область действия — core/plugin API, не
  произвольные функции, которые сам разработчик зарегистрировал в
  `tauri::generate_handler!`. Контроль доступа к ним — факт регистрации в
  `generate_handler!`, ничего больше (это даже проще, чем Electron
  whitelist, т.к. нет отдельного слоя сериализации аргументов, который
  нужно было вручную валидировать на стороне `ipcMain` — `serde`
  десериализует и валидирует типы автоматически).
- **Встроенные/плагинные API** (открытие файлов, скрытие окна и т.п.)
  гейтятся через **capabilities** — JSON-манифесты в
  `src-tauri/capabilities/*.json`, каждый привязан к конкретным окнам по
  `label` (поле `"windows"`). У нас два манифеста:
  `capabilities/default.json` (окно `main`) и `capabilities/overlay.json`
  (окно `overlay`) — оба сейчас на базовом `core:default`, т.к. ни одно
  окно не делает ничего более привилегированного, чем скрыть себя
  (`getCurrentWindow().hide()` в overlay, по Escape/кнопке `✕`).
  Это прямая реализация принципа "у каждого окна — только необходимые ему
  права" (ТЗ, раздел 7): overlay **физически не может** вызвать, например,
  файловый API, если это не разрешено в его собственном capability-файле,
  даже если такой API когда-нибудь включат для main-окна.
- Shared TS-типы контрактов команд живут в `src/shared/types/` (например
  `app-info.ts`) и **синхронизируются вручную** с Rust DTO
  (`#[derive(Serialize)]` структуры в `commands/*.rs`) — Tauri, в отличие
  от нашего Electron IPC-слоя, не даёт одного файла типов на оба конца
  границы автоматически. Если расхождение станет проблемой по мере роста
  числа команд — рассмотреть `tauri-specta`/`ts-rs` (не делаем этого в
  Итерации 0, намеренно, чтобы не тащить лишнюю абстракцию раньше, чем она
  понадобится).

## Security

- **CSP** — `tauri.conf.json#app.security.csp`, применяется Tauri
  автоматически ко всем webview (эквивалент Electron CSP-заголовка +
  meta-тега, но не нужно дублировать в HTML).
- **Никакого произвольного remote-контента** — оба окна грузят только
  собственные `index.html`/`overlay.html` (dev: с Vite dev-сервера на
  `localhost:1420`, prod: из встроенного в бинарник `frontendDist`).
- **Single instance** (FR-01.3) — `tauri-plugin-single-instance`,
  регистрируется первым плагином в билдере (это требование самого плагина).
- Секреты (будущие Jira-токены, Итерация 7) будут жить только в
  Rust-слое — frontend их не увидит, в соответствии с тем же принципом,
  что был в Electron-версии ("токены только в main process").

## Слой данных (Итерация 1)

`src-tauri/src/db/`:

- `database.rs` — `create_database(path)`: открывает файл, включает
  `journal_mode = WAL` и `foreign_keys = ON`, применяет миграции. Единая
  точка создания соединения — используется и реальным приложением
  (`app_database.rs`), и тестами (`db::test_support::temp_database()`,
  временный файл на диске через крейт `tempfile`).
- `app_database.rs` — `AppDatabase(Mutex<Connection>)`, управляется как
  Tauri managed state (`app.manage(...)`), открывается в `setup()` до
  создания окон, путь — `app.path().app_data_dir()` (аналог Electron
  `app.getPath('userData')`). Один и тот же процесс Tauri обслуживает и
  главное окно, и overlay — значит, они неизбежно работают с одним и тем
  же `AppDatabase` (ТЗ: "Все изменения из overlay и основного окна
  выполняются через один сервис"). При инициализации чистит
  `session_edits` старше 30 дней (FR-04.8).
- `migrations/` — пронумерованные миграции (`m0001_initial_schema.rs` и
  далее), журналируются в `_migrations`; каждая применяется в собственной
  транзакции. Повторный запуск на той же БД — no-op.
- `repositories/*.rs` — **свободные функции** над `&Connection`/
  `&mut Connection` (не классы, как было в TS-версии — идиоматичнее для
  Rust, без self-referential borrow-проблем): `projects`,
  `activity_events` (только `insert`/чтение — первичная история
  неизменяема), `work_sessions` (CRUD + атомарные
  `update`/`soft_delete`/`restore`/`undo_last_edit`, каждая мутация внутри
  `conn.transaction()` вместе со snapshot-записью в `session_edits`),
  `session_edits`, `worklog_drafts`, `settings`.
- `error.rs` — `RepoError` (`Sqlite`/`InvalidInterval`/`NotFound`) поверх
  `rusqlite::Error`, чтобы вызывающий код (и тесты) мог различать "БД
  недоступна" от "патч нарушает доменный инвариант" (`end > start`).

Время хранится как **эпоха в секундах UTC** (`i64`) во всех таблицах;
`timezoneId` (IANA, например `Europe/Bishkek`) — отдельным полем на
`work_sessions`, для будущего деления по локальным суткам (Session Engine,
Итерация 4 — в Итерации 1 эта логика не реализована).

**Доменные типы** — `src-tauri/src/domain/` (аналог прежнего
`src/shared/types/`): `Project`, `ActivityEvent`, `WorkSession`,
`SessionEdit`, `WorklogDraft` + связанные enum'ы (`Confidence`,
`TrackingStatus`, `WorkSessionSource/ReviewStatus`,
`SessionEditOperation`). Все `#[derive(Serialize, Deserialize)]` — готовы
к будущей IPC-экспозиции, но пока ни одна IPC-команда их не использует
(только `get_app_info` из Итерации 0). Ручная синхронизация с TS-типами
фронтенда (как и раньше) — не автогенерируется.

**Чего в этом слое сознательно нет:** определение перекрытия интервалов и
автоматическое разрешение конфликтов (FR-04.6), split/merge (FR-04),
таблицы `issues_cache`/`jira_submissions` (появятся в Итерации 7 вместе с
Jira-интеграцией). IPC-команды к этим репозиториям тоже не добавлены —
Dashboard/Timeline (Итерация 5) и overlay-редактирование (Итерация 6)
подключат их позже; Итерация 1 — только сам слой данных и его тесты
(отсюда `#[allow(dead_code)]` на модулях `db`/`domain` в `lib.rs` — временная
и осознанная пометка, снимется по мере подключения).

## Activity Tracker (Итерация 2)

`src-tauri/src/platform/windows_activity_adapter.rs` — тонкие обёртки над
Win32 (крейт `windows`, ADR-0007): `get_foreground_process_name()`,
`get_system_idle_seconds()`, `is_session_locked(foreground_process_name)`.
Не покрыты unit-тестами (нельзя осмысленно мокнуть ОС) — проверены вручную
на реальной машине (`cargo test manual_smoke -- --ignored --nocapture`) и
сквозным прогоном реального приложения.

`src-tauri/src/tracking/`:

- `whitelist.rs` — `WhitelistEntry{process_name, category}`,
  `default_whitelist()` (WebStorm/IDE-семейство JetBrains, VS Code,
  основные терминалы Windows), `load_whitelist(conn)` (читает
  пользовательский override из `settings` под ключом
  `activityTracker.whitelist`, молча откатывается на дефолт при
  отсутствии/битом JSON — UI редактирования появится в Итерации 5).
- `engine.rs` — **чистая** функция `decide(signals, whitelist) ->
  DecidedEvent`, без каких-либо Win32/БД вызовов внутри — вся ветвящаяся
  логика состояний полностью unit-тестируема (10 тестов на приоритеты
  Paused/Suspended/Locked/Idle/Unknown/Tracking). Приоритет сверху вниз:
  ручная пауза → разрыв между poll похожий на сон → заблокированный экран
  → превышен idle-порог → foreground-detector не смог определить процесс
  (Unknown) → проверка whitelist. Для не-whitelisted процессов реальное
  имя **не сохраняется** (`appCategory: "excluded"`, `processName:
  "excluded"`) — FR-02.3/FR-02.4.
- `tracker.rs` — `ActivityTrackerHandle` (атомарный флаг паузы, читается
  треем через `app.state::<ActivityTrackerHandle>()`) и `start(app)`,
  запускающий фоновый `std::thread` с циклом `sleep(5s) → собрать сигналы →
  decide() → activity_events::insert()`. Поток **не завязан на окна** (ТЗ,
  раздел 7) — переживает закрытие/скрытие любого окна, завершается только
  вместе с процессом.

**Suspend/resume определяется косвенно** — через сравнение фактического
интервала между двумя poll с ожидаемым (`SUSPEND_GAP_MULTIPLIER = 3`), а не
через нативный `PowerRegisterSuspendResumeNotification`-callback. Осознанное
упрощение Итерации 2 — см. ADR-0007 для деталей и условий пересмотра.

## Git context и Jira key (Итерация 3)

`src-tauri/src/tracking/`:

- `git_adapter.rs` — тонкая обёртка над `git` CLI: `get_current_branch(path)`
  (`git -C <path> branch --show-current`, различает `OnBranch`/`DetachedHead`/
  `NotARepo`/`GitUnavailable`) и `head_mtime(path)` (mtime `.git/HEAD`).
  Не тестируется unit-тестами (реальный процесс/ФС) — то же обоснование,
  что и для Win32-адаптера.
- `issue_key.rs` — `extract_issue_key(branch, pattern)`: нормализация
  (`_`→`-`, uppercase) + regex (ТЗ FR-03.3, "регистр и разделители
  нормализовать"). Паттерн настраивается per-project (`Project.issue_regex`,
  Итерация 1), дефолт — `DEFAULT_ISSUE_REGEX`.
- `git_context.rs` — **чистая** логика приоритета (FR-03.4): явный pin
  (параметр уже принимается, реального источника до overlay-UI в Итерации 6
  нет) → активный репозиторий+его ветка → `preferences.defaultIssueKey`
  проекта → «Нераспределено». `pick_most_recently_active()` выбирает между
  несколькими настроенными репозиториями по дате изменения `.git/HEAD`
  (недорогая эвристика "где пользователь работал последним" — без чтения
  заголовков окон). 9 unit-тестов.
- `tracker.rs` расширен: при `state == Tracking` резолвит git-контекст и
  сливает его confidence с confidence состояния трекера через
  `weaker_confidence` (итоговая уверенность — всегда уверенность самого
  слабого звена цепочки вывода, не произвольно одна из двух).

**Проверено на реальных данных** (не только unit-тестами): добавлен
project, указывающий на сам репозиторий DevLog; `activity_events` корректно
показали `branch="master", issueKey=None` (ветка без тикета — не
приписывается задаче), и, после создания тестовой ветки
`feature/TEST-123-...`, **ровно на границе переключения** — переход на
`branch="feature/TEST-123-...", issueKey="TEST-123"`. Тестовая ветка
удалена после проверки.

**Чего в этом слое сознательно нет:** фактическое завершение/создание
`work_sessions` на смене ветки — это Session Engine (Итерация 4);
Итерация 3 лишь гарантирует, что каждое сырое `activity_event` корректно
помечено своим git-контекстом на момент наблюдения — необходимое, но не
достаточное условие для построения сессий. GitLab API/коммиты/MR — Итерация
11+ (опционально, после MVP).

## Что сознательно НЕ сделано в Итерации 0

- **Нет SQLite** — будет `rusqlite` (ADR готовится к Итерации 1, по
  аналогии с тем, как это было устроено на Electron/`better-sqlite3`, но
  без проблемы ABI-совместимости нативных Node-модулей: Rust-зависимости
  компилируются прямо в бинарник).
- **Нет Activity Tracker/Git/Jira** — Итерации 2/3/7.
- **Нет UI для Dashboard/Timeline/Settings** — main window показывает
  только статус соединения с Rust-ядром через `get_app_info`.
- **Workarea-aware позиционирование overlay** — tao/Tauri `Monitor` не
  даёт `workArea` (в отличие от Electron `display.workArea`); сейчас
  используются полные границы монитора. Точная версия через Win32
  `GetMonitorInfoW` — Итерация 6 (вместе с остальными тест-кейсами FR-06 на
  несколько мониторов/DPI).
- **Нет UI переназначения hotkey при конфликте** — только `eprintln!` в
  консоль (Tauri на Windows не имеет прямого аналога Electron
  `Notification` "из коробки" без отдельного плагина
  `tauri-plugin-notification`; добавить, если понадобится, не раньше
  Итерации 6, когда будет реальный UI настроек).
