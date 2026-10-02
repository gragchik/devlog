# ADR-0005: Build-tooling — Tauri CLI + Vite (замена ADR-0001)

**Статус:** принято и проверено (Итерация 0, Tauri).

## Контекст

После перехода со стека Electron на Tauri (решение пользователя,
2026-10-02) нужно заново зафиксировать dev/build-инструментарий. Старое
ADR-0001 (electron-vite + electron-builder) перенесено в
`docs/decisions/electron-archive/` как исторический референс.

## Решение

- **Frontend dev/build:** обычный **Vite** (без надстроек) — React +
  TypeScript, multi-page (два HTML-входа: `index.html` для main-window,
  `overlay.html` для overlay, см. `vite.config.ts`). Никакой
  Electron-специфичной надстройки больше не нужно: Tauri CLI сам
  оборачивает запуск/остановку Vite dev-сервера через
  `build.beforeDevCommand`/`beforeBuildCommand` в `tauri.conf.json`.
- **Rust backend dev/build:** обычный **Cargo**, оркestрируется Tauri CLI
  (`@tauri-apps/cli`, команды `tauri dev`/`tauri build`). Тот же бинарник
  Cargo собирает и dev, и release — в отличие от Electron, где main-процесс
  компилировался Vite отдельно от упаковки.
- **Installer:** встроенный **Tauri bundler** (`tauri build`, таргет
  `nsis` — см. `bundle.targets` в `tauri.conf.json`). Отдельный
  electron-builder/Forge не нужен — упрощение по сравнению с ADR-0001.
- **Иконки:** `npx tauri icon <source.png>` генерирует весь нужный набор
  (`.ico`, `.icns`, `32x32.png`, `128x128.png`, `128x128@2x.png`) из одного
  источника. Мобильные (iOS/Android) и Windows Store (`Square*.png`,
  `StoreLogo.png`) ассеты, которые команда генерирует попутно, удалены —
  не нужны для Windows desktop MVP.
- **Lint/test:** `eslint` (frontend, flat config) + `tsc --noEmit`
  (typecheck); `cargo clippy` + `cargo test` (backend). Frontend
  unit-тестов (Vitest) пока нет — вся реальная логика Итерации 0 живёт в
  Rust и покрыта `cargo test` (см. `src-tauri/src/windows/overlay_window.rs`);
  Vitest вернётся, когда во frontend появится код, достойный unit-тестов
  (Dashboard/Timeline, Итерация 5+) — держать тестовый раннер без единого
  теста не имеет смысла.

## Проверено практически

- `cargo check`/`cargo clippy --all-targets`/`cargo test` — чисто.
- `npm run typecheck`/`npm run lint` — чисто.
- `npm run build:vite` — production-бандл собирается (отдельные чанки для
  main/overlay, ~144KB общий vendor-чанк React).
- `npm run dev` (`tauri dev`) — полный dev-цикл: Vite dev-сервер + сборка
  Rust (~5 мин на первую чистую сборку из-за объёма зависимостей Tauri,
  ~13 сек на инкрементальную пересборку при правке Rust-файла, с
  автоматическим watch и перезапуском процесса) + реальный запуск
  приложения — окно открывается, overlay открывается/закрывается по
  глобальному хоткею (см. `docs/iterations/00.md`).

## Последствия

- Первая чистая Rust-сборка заметно дольше, чем был `electron-vite build`
  (~5 минут против секунд) — компилируется ~370 крейтов транзитивных
  зависимостей Tauri. Инкрементальные пересборки быстрые (~13 сек).
  Не являлось блокером для разработки, но стоит ожидать на CI/чистых
  машинах.
- `tauri.conf.json` — единственный источник конфигурации окон/bundle/CSP;
  в отличие от Electron, где эквивалентная конфигурация была разбросана по
  коду (`BrowserWindow` опции, electron-builder config).
