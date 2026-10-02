//! Доменные типы, не зависящие от Tauri/SQLite — аналог прежнего
//! `src/shared/types/` из Electron-версии. Используются и слоем БД
//! (`crate::db`), и (по мере появления) IPC-командами.

pub mod activity_event;
pub mod confidence;
pub mod project;
pub mod session_edit;
pub mod tracking_status;
pub mod work_session;

// Worklog Review в Итерации 5 — только read-only агрегация по issueKey,
// без персистентных черновиков (полноценный жизненный цикл черновика имеет
// смысл вместе с реальной отправкой в Jira — Итерация 7/8). Тип и
// репозиторий готовы заранее (схема уже есть с Итерации 1), но пока не
// используются.
#[allow(dead_code)]
pub mod worklog_draft;
