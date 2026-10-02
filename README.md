# DevLog

Персональный автоматический Worklog для Windows: фоновый сбор сигналов
рабочей активности, сопоставление с Git/Jira-задачами, черновики worklog и
быстрый overlay поверх IDE для правки сессий без переключения окон.

Полное техническое задание: [`.claude/tasks/DEVLOG_CLAUDE_CODE_TZ.md`](.claude/tasks/DEVLOG_CLAUDE_CODE_TZ.md).
Архитектура: [`docs/architecture.md`](docs/architecture.md).
Технические решения (ADR): [`docs/decisions/`](docs/decisions/).
Статус по итерациям: [`docs/iterations/`](docs/iterations/).

**Статус проекта:** Итерация 0 (каркас) — см.
[`docs/iterations/00.md`](docs/iterations/00.md) для деталей и известных
рисков. Activity Tracker, SQLite, Git/Jira-интеграция и реальный UI **ещё не
реализованы** — приложение сейчас демонстрирует только рабочий
скелет (окна, трей, hotkey, типизированный IPC).

## Требования

- Windows 10/11 x64.
- Node.js — см. `engines` в `package.json` (устанавливается через nvm-windows
  или напрямую с nodejs.org).

## Команды

```bash
npm install     # установка зависимостей (см. примечание ниже про Electron)
npm run dev     # запуск в dev-режиме (HMR для renderer, автоперезапуск main/preload)
npm run build   # production-сборка main/preload/renderer в out/
npm run preview # запуск собранного build без dev-сервера
npm run typecheck
npm run lint
npm run test
```

> **Примечание:** при первом `npm install` может не успеть скачаться
> бинарник Electron (~245MB с GitHub releases) — если `npm run dev` упадёт с
> `Error: Electron uninstall`, выполните `node node_modules/electron/install.js`
> повторно и запустите `npm run dev` ещё раз.

## Структура

```
src/
  main/        — Electron main process: окна, трей, shortcuts, IPC, (позже) трекинг/БД/Jira
  preload/     — типизированные preload-скрипты (отдельно для main-window и overlay)
  renderer/    — React-приложения (main-window, overlay) и общие компоненты
  shared/      — код без зависимостей от Electron: IPC-контракты, типы, утилиты
tests/
  unit/        — Vitest, чистые функции и логика без Electron
docs/
  architecture.md, decisions/, iterations/
```

## Горячие клавиши (по умолчанию)

- `Ctrl+Alt+W` — показать/скрыть overlay.
- `Escape` (в фокусе overlay) — скрыть overlay.

## Приватность

Трекер не читает содержимое окон, URL, нажатия клавиш, буфер обмена,
скриншоты или текст документов — только имя процесса активного окна, Git-ветку
текущего репозитория и системное idle-время. Подробности — раздел 3 (FR-02) в
ТЗ и `docs/security/` (будет заполняться по мере реализации Activity Tracker,
Итерация 2+).
