# Архитектура DevLog

Этот документ описывает принятые структурные решения. Детали отдельных
технических выборов — в `docs/decisions/*.md` (ADR). Статус по итерациям —
в `docs/iterations/*.md`.

## Процессы и окна

- **Main process** (`src/main/`) — единственный источник правды: владеет
  SQLite (с Итерации 1), Activity Tracker (с Итерации 2), Git/Jira
  адаптерами, глобальным shortcut, треем. Renderer-процессы никогда не
  обращаются к файловой системе, сети или дочерним процессам напрямую.
- **Main window** (`src/main/windows/main-window.ts`) — создаётся при
  старте, закрытие (`X`) не завершает процесс: `close` перехватывается и
  заменяется на `hide()`, пока приложение не получает явную команду выхода
  (`before-quit` → `markAppQuitting()`). Это реализует FR-01.1 ("закрытие
  главного окна не завершает слежение").
- **Overlay window** (`src/main/windows/overlay-window.ts`) — отдельное
  `BrowserWindow`: `frame: false`, `alwaysOnTop: true`, `skipTaskbar: true`,
  создаётся один раз и держится скрытым (`hide()`), а не уничтожается —
  это даёт быстрое повторное открытие без пересоздания процесса/окна
  (цель FR-06.4, p95 ~500мс; фактическое время открытия пока не замерено
  инструментально — см. риски Итерации 0).
- **Tray** (`src/main/tray/tray.ts`) — не зависит от видимости окон; меню
  строится заново при каждом изменении состояния (`updateTrayState`).

У main-window и overlay-window — **разные preload-скрипты**
(`main-preload.ts` / `overlay-preload.ts`) и, соответственно, разные формы
API в `window.devlog` на renderer-стороне. Это сознательное решение: overlay
не должен иметь доступ к методам, которые имеют смысл только в контексте
главного окна (и наоборот), даже если сейчас (Итерация 0) оба API
пересекаются почти полностью.

## IPC: типизированный whitelist

Единственный канал связи renderer → main — вызовы из `src/shared/ipc-contracts/`:

- `channels.ts` — явный список допустимых каналов (`IPC_CHANNELS`). Всё, что
  не входит в этот список, не регистрируется в `ipcMain.handle` и не
  экспонируется через `contextBridge`.
- `types.ts` — типы запрос/ответ для каждого канала, используются и в
  preload (аргументы `ipcRenderer.invoke`), и в main (`ipcMain.handle`), и в
  renderer (через импорт из `@shared/...`) — один источник истины для формы
  данных на обеих сторонах IPC-границы.

Каждый `ipcMain.handle` в `src/main/ipc/handlers.ts` проверяет
`event.senderFrame` на соответствие одному из созданных приложением окон
(`isTrustedSender`), а привилегированные операции (например,
`overlay:close`) — что отправитель это именно overlay, а не main-window
(`isOverlaySender`). Это защита от компрометированного/неожиданного sender'а
(ТЗ, раздел 7: "проверка sender").

Все окна создаются с `nodeIntegration: false`, `contextIsolation: true`,
`sandbox: true` — renderer не имеет доступа ни к Node.js API, ни к
нефильтрованному `ipcRenderer`.

## Shared-код

`src/shared/` — код без зависимостей от Electron API, используемый и в
main, и в renderer (через алиас `@shared/*` в обоих `tsconfig.*.json` и в
`electron.vite.config.ts`):

- `ipc-contracts/` — см. выше.
- `types/` — доменные типы без побочных эффектов (например,
  `TrackingStatus`).
- `utils/` — чистые функции (например, `secondsToHms`), тестируемые в
  изоляции через Vitest без поднятия Electron.

## Security-периметр

`src/main/security.ts` применяется один раз при старте (`app.whenReady`):

- `setWindowOpenHandler` на каждый новый `webContents` → `{ action: 'deny' }`
  (никаких всплывающих Electron-окон по клику на ссылку — внешние ссылки
  уходят в системный браузер через `shell.openExternal`, см.
  `main-window.ts`).
- `will-navigate` блокируется для всего, что не `file://` и не
  `http://localhost` (последнее — только для dev-сервера Vite).
- CSP-заголовок на все ответы через `session.defaultSession.webRequest`
  **плюс** `<meta http-equiv="Content-Security-Policy">` в каждом HTML (два
  слоя защиты — на случай, если один механизм не сработает для конкретного
  протокола/типа ресурса).

## Слой данных (Итерация 1)

`src/main/db/`:

- `database.ts` — `createDatabase(filePath)`: открывает файл, включает
  `journal_mode = WAL` и `foreign_keys = ON`, применяет миграции. Единая
  точка создания соединения — используется и реальным приложением
  (`app-database.ts`), и тестами (временный файл на диске,
  `tests/integration/helpers/temp-database.ts`).
- `app-database.ts` — синглтон `getAppDatabase()` на реальном пути
  `app.getPath('userData')/devlog.sqlite3`. Открывается один раз при
  `app.whenReady()` (`src/main/index.ts`), закрывается в `before-quit`.
  Один и тот же процесс main обслуживает и главное окно, и overlay — значит,
  они неизбежно работают с одним и тем же соединением (ТЗ: "Все изменения
  из overlay и основного окна выполняются через один сервис"). При первом
  открытии чистит `session_edits` старше 30 дней (FR-04.8).
- `migrate.ts` + `migrations/000N-*.ts` — пронумерованные миграции,
  журналируются в `_migrations`; каждая применяется в собственной
  транзакции. Повторный запуск на той же БД — no-op.
- `repositories/*.ts` — по одному классу на таблицу (`ProjectsRepository`,
  `ActivityEventsRepository`, `WorkSessionsRepository`,
  `SessionEditsRepository`, `WorklogDraftsRepository`,
  `SettingsRepository`). `activity_events` — только `insert`/чтение
  (первичная история неизменяема); мутации `work_sessions`
  (`update`/`softDelete`/`restore`/`undoLastEdit`) всегда внутри
  `db.transaction()` и атомарно пишут snapshot до/после в `session_edits`.

Время хранится как **эпоха в секундах UTC** (`INTEGER`) во всех таблицах;
`timezoneId` (IANA, например `Europe/Bishkek`) — отдельным полем на
`work_sessions`, для будущего деления по локальным суткам (Session Engine,
Итерация 4 — в Итерации 1 эта логика не реализована).

**Чего в этом слое сознательно нет:** определение перекрытия интервалов и
автоматическое разрешение конфликтов (FR-04.6), split/merge (FR-04),
таблицы `issues_cache`/`jira_submissions` (появятся в Итерации 7 вместе с
Jira-интеграцией — см. ADR-0004). IPC-каналы к этим репозиториям тоже не
добавлены — Dashboard/Timeline (Итерация 5) и overlay-редактирование
(Итерация 6) подключат их позже; Итерация 1 — только сам слой данных и его
тесты.

## Что сознательно НЕ сделано в Итерации 0

(чтобы не создавать иллюзию готовности) — подробности и обоснование в
`docs/iterations/00.md`:

- Нет SQLite, нет Activity Tracker, нет Git/Jira адаптеров, нет
  Session Engine — только заглушка `trackingStatusStub: 'UNKNOWN'`.
- Нет UI для Dashboard/Timeline/Settings — main window показывает только
  статус соединения IPC.
- Нет сохранения позиции overlay между запусками (FR-06.4 "сохранять
  последнюю позицию по дисплею") — пока всегда позиционируется у правого
  верхнего края ближайшего к курсору дисплея.
- Нет UI переназначения hotkey при конфликте (FR-06.2) — только
  `Notification` с предупреждением.
