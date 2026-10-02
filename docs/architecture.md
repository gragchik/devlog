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
