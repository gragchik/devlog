//! Worklog Review (FR-05.5, FR-07, FR-08): черновики из сессий дня и их
//! безопасная отправка в Jira. Не зависит от Tauri — команды в
//! `commands/jira.rs` только переводят IPC в вызовы этого слоя.

pub mod drafts;
pub mod reminder;
pub mod submission;
pub mod templates;
