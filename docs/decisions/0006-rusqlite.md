# ADR-0006: SQLite-драйвер — rusqlite (замена ADR-0002)

**Статус:** принято и проверено (Итерация 1, Tauri).

## Контекст

После перехода на Tauri нужен SQLite-доступ из Rust-ядра. Старое ADR-0002
(`better-sqlite3`) перенесено в `docs/decisions/electron-archive/`.

## Решение

**`rusqlite`** с фичей `bundled` (компилирует SQLite C-амальгаму прямо в
бинарник — не нужна системная libsqlite3, как и в случае с `better-sqlite3`
на Node, но здесь это решается на уровне Cargo, без отдельного native
addon/ABI вообще). ID — `uuid` крейт (`v4`). Ошибки — `rusqlite::Error`
оборачивается в собственный `RepoError` (`db::error`) там, где нужно
различать "SQL упал" от "нарушен доменный инвариант" (например,
`end > start` для сессий).

## Почему rusqlite проще, чем была история с better-sqlite3/Electron

В Node-мире (ADR-0002) главный риск был ABI-совместимость нативного
addon'а между версией Node, с которой ставился пакет, и версией Node,
встроенной в Electron — именно поэтому там потребовался отдельный
smoke-test внутри настоящего Electron-процесса. В Rust такой проблемы нет
**в принципе**: `rusqlite` с фичей `bundled` компилируется как часть сборки
самого приложения одним и тем же тулчейном — нет двух разных "рантаймов",
которые могут разойтись по ABI.

## Проверено практически

- `cargo test` — 37/37 тестов проходят, включая:
  - миграции/создание всех таблиц, идемпотентность повторного запуска,
    WAL-режим, CHECK-constraint на уровне самого SQLite;
  - CRUD каждого репозитория (projects/activity_events/work_sessions/
    session_edits/worklog_drafts/settings);
  - атомарность `work_sessions::update`/`soft_delete`/`restore`/
    `undo_last_edit` — snapshot до/после в `session_edits` в одной
    транзакции;
  - **настоящий** многопоточный тест (`std::thread::spawn` + `Arc<Mutex<Connection>>`,
    не имитация через `Promise.all` в однопоточном JS, как было в
    Electron-версии) — два потока одновременно обновляют одну сессию через
    общий `Mutex<Connection>` (ту же конструкцию, что использует реальный
    `AppDatabase`), БД не повреждается, обе правки видны в журнале.
- Сквозной smoke-test реального приложения (`npm run dev`): файл БД
  реально создаётся по `app.path().app_data_dir()`
  (`%APPDATA%\dev.devlog.app\devlog.sqlite3` + WAL/SHM), без ошибок при
  старте/остановке.

## Последствия

- `AppDatabase(Mutex<Connection>)` как Tauri managed state — единственная
  точка доступа к БД из будущих IPC-команд; `Mutex`, а не `RwLock`, т.к.
  `rusqlite::Connection` не `Sync` и не поддерживает параллельные читатели
  без собственного connection pool (не нужен для однопользовательского
  desktop-приложения с умеренной частотой записи).
- Репозитории реализованы как свободные функции над `&Connection`/
  `&mut Connection`, а не как классы с собственным состоянием (в отличие
  от TS-версии с классами) — идиоматичнее для Rust, не требует
  self-referential borrow-гимнастики.
