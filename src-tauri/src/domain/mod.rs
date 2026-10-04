//! Доменные типы, не зависящие от Tauri/SQLite — аналог прежнего
//! `src/shared/types/` из Electron-версии. Используются и слоем БД
//! (`crate::db`), и (по мере появления) IPC-командами.

pub mod activity_event;
pub mod confidence;
pub mod jira;
pub mod project;
pub mod session_edit;
pub mod tracking_status;
pub mod work_session;
pub mod worklog_draft;
