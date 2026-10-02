# DevLog

Персональный автоматический Worklog для Windows: фоновый сбор сигналов
рабочей активности, сопоставление с Git/Jira-задачами, черновики worklog и
быстрый overlay поверх IDE для правки сессий без переключения окон.

Полное техническое задание: [`.claude/tasks/DEVLOG_CLAUDE_CODE_TZ.md`](.claude/tasks/DEVLOG_CLAUDE_CODE_TZ.md).
Архитектура: [`docs/architecture.md`](docs/architecture.md).
Технические решения (ADR): [`docs/decisions/`](docs/decisions/).
Статус по итерациям: [`docs/iterations/`](docs/iterations/).

**Статус проекта:** Итерация 0 (каркас, на **Tauri** — см. примечание о
миграции в начале ТЗ) — см. [`docs/iterations/00.md`](docs/iterations/00.md)
для деталей и известных рисков. Activity Tracker, SQLite, Git/Jira-интеграция
и реальный UI **ещё не реализованы** — приложение сейчас демонстрирует
только рабочий скелет (окна, трей, hotkey, типизированные команды).

> Ранняя версия (Итерации 0–1) была реализована на Electron и переписана
> на Tauri по решению пользователя — см. `docs/*/electron-archive/` и
> git-историю (коммиты `a8b22e2`, `7283498`).

## Требования

- Windows 10/11 x64.
- Node.js ^20.19 / ^22.12 / >=24 (см. `package.json#engines`, если есть).
- **Rust** (через [rustup](https://rustup.rs)) + **MSVC Build Tools**
  (компонент «Desktop development with C++» из
  [Visual Studio Build Tools](https://visualstudio.microsoft.com/visual-cpp-build-tools/)) —
  без них Rust-код не слинкуется на Windows.
- WebView2 Runtime — на Windows 11 предустановлен; на Windows 10 может
  потребоваться установка отдельно.

## Команды

```bash
npm install          # установка зависимостей frontend
npm run dev           # tauri dev — поднимает Vite dev-сервер + Rust-ядро, открывает приложение
npm run build          # tauri build — production-сборка + installer (NSIS)
npm run build:vite     # только frontend-бандл, без Rust/упаковки
npm run typecheck      # tsc --noEmit (frontend)
npm run lint           # eslint (frontend)

cd src-tauri
cargo check            # быстрая проверка типов Rust-кода
cargo clippy --all-targets   # линт
cargo test              # unit-тесты Rust-кода
```

> **Первая сборка Rust-кода занимает ~5 минут** (компилируются все
> транзитивные зависимости Tauri с нуля). Дальнейшие инкрементальные
> пересборки — секунды.

## Структура

```
src-tauri/           — Rust-ядро (Tauri): окна, трей, shortcuts, команды, (позже) БД/трекинг/Jira
  src/commands/       — #[tauri::command] функции, вызываемые из frontend через invoke()
  src/windows/        — создание/позиционирование main-window и overlay
  src/tray/           — системный трей
  capabilities/       — permission-манифесты (по одному на окно)
src/                 — frontend (React + TypeScript + Vite)
  main-window/        — главное окно
  overlay/            — overlay поверх других приложений
  shared/             — типы, синхронизированные вручную с Rust DTO; общие стили
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
