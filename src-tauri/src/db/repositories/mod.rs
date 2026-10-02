pub mod activity_events;

// См. комментарий у `domain::jira` — репозитории готовы и протестированы,
// ещё не подключены к командам (Итерация 7 в процессе).
#[allow(dead_code)]
pub mod issues_cache;
#[allow(dead_code)]
pub mod jira_submissions;
pub mod projects;
pub mod session_edits;
pub mod settings;
pub mod work_sessions;
#[allow(dead_code)]
pub mod worklog_drafts;
