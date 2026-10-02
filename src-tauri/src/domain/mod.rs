//! Доменные типы, не зависящие от Tauri/SQLite — аналог прежнего
//! `src/shared/types/` из Electron-версии. Используются и слоем БД
//! (`crate::db`), и (по мере появления) IPC-командами.

pub mod activity_event;
pub mod confidence;

// Итерация 7 (Jira Integration) в процессе — типы готовы и протестированы
// (`domain::jira`, `integrations::jira`), но ещё не подключены ни к одной
// IPC-команде (`commands/jira.rs` — следующий шаг). Без `#[allow(dead_code)]`
// компилятор шумит на весь модуль, хотя сам код и его unit-тесты рабочие.
#[allow(dead_code)]
pub mod jira;
pub mod project;
pub mod session_edit;
pub mod tracking_status;
pub mod work_session;

// См. комментарий у `domain::jira` — то же самое для черновиков.
#[allow(dead_code)]
pub mod worklog_draft;
