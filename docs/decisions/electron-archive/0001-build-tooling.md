> **⚠️ SUPERSEDED (2026-10-02):** стек изменён с Electron на Tauri (решение
> пользователя). Этот ADR описывает решение для Electron-реализации, которая
> больше не разрабатывается (код остался только в git-истории, коммиты
> `a8b22e2`/`7283498`). Сохранён как исторический референс. Актуальное
> решение по build-tooling — ADR-0005.

# ADR-0001: Build-tooling — electron-vite + electron-builder

**Статус:** ~~принято (Итерация 0)~~ — superseded, см. баннер выше.

## Контекст

Нужна связка dev/build-инструментов для Electron-приложения с тремя целями
сборки (main, два preload-скрипта, два renderer-приложения — main-window и
overlay) на TypeScript + React + Vite, плюс отдельный шаг упаковки в Windows
installer (появится в Итерации 10).

Рассматривались:

- **Electron Forge + `@electron-forge/plugin-vite`** — официальный
  CLI-тулинг Electron, но plugin-vite исторически имел нестабильную
  поддержку multi-entry preload/renderer конфигураций и собственный формат
  конфига, дублирующий часть Vite-настроек.
- **electron-vite** (пакет `electron-vite`, отдельный от Forge) — тонкая
  надстройка над Vite специально под архитектуру main/preload/renderer
  Electron-приложений; нативно поддерживает несколько входных точек через
  `rollupOptions.input` для каждого из трёх контекстов, HMR для renderer,
  автоматический формат вывода (ESM/CJS) по `package.json#type`.
- **Ручная сборка через чистый Vite** без специализированной надстройки —
  требует самостоятельно решать проблему трёх разных `tsconfig`/окружений
  (main — Node, preload — Node+browser globals, renderer — browser) и ручного
  перезапуска Electron при пересборке main/preload.

## Решение

- **electron-vite** для dev/build (`npm run dev` / `npm run build`).
  Конфиг — `electron.vite.config.ts`, три секции (`main`, `preload`,
  `renderer`), у renderer `root: src/renderer` и два HTML entry
  (`main-window`, `overlay`).
- **electron-builder** для Windows-инсталлятора — решение зафиксировано
  сейчас, сама зависимость и конфигурация добавляются в Итерации 10
  (паковать нечего, пока нет готового UI/фич). Причина выбора: более гибкая
  настройка NSIS (нужна для корпоративного Windows-окружения, где может
  требоваться per-user install без прав администратора) по сравнению с
  Squirrel.Windows из классического Electron Forge maker.

## Последствия

- main/preload собираются в CJS- или ESM-формате автоматически исходя из
  `package.json#type: "module"` — используем ESM; preload-скрипты electron-vite
  всегда выводит с расширением `.mjs` (особенность загрузки preload в
  Electron), main — `.js` (т.к. корневой `package.json` уже помечен как
  `"type": "module"`, поэтому `.js` трактуется как ESM рантаймом).
- Нужно вручную поддерживать соответствие путей в `windows/*.ts`
  (`loadFile`/`loadURL`) структуре вывода `out/renderer/<entry>/index.html` —
  обеспечивается через `renderer.root` в конфиге.
- Два independent tsconfig для web/node контекстов (`tsconfig.web.json`,
  `tsconfig.node.json`) — компромисс ради строгой типизации каждого
  контекста без утечки Node-типов в renderer и наоборот.

## Версии на момент принятия

- `electron@^44.5.1` (последняя стабильная на момент разработки).
- `electron-vite@^5.0.0` (первая стабильная линия с поддержкой Vite 6/7).
- `vite@^6.0.7`.
