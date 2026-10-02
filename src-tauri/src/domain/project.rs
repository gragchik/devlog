use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const DEFAULT_ISSUE_REGEX: &str = r"\b[A-Z][A-Z0-9]+-\d+\b";

/// Локальный репозиторий, явно добавленный пользователем (FR-03.1).
/// `issue_regex` — настраиваемый паттерн извлечения issue key из имени
/// ветки (FR-03.3), по умолчанию `DEFAULT_ISSUE_REGEX`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Project {
    pub id: String,
    pub name: String,
    pub repo_path: String,
    pub enabled: bool,
    pub issue_regex: String,
    /// Произвольные пользовательские настройки проекта (project → task правила и т.п.).
    pub preferences: Value,
    pub created_at_utc: i64,
    pub updated_at_utc: i64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateProjectInput {
    pub name: String,
    pub repo_path: String,
    #[serde(default)]
    pub enabled: Option<bool>,
    #[serde(default)]
    pub issue_regex: Option<String>,
    #[serde(default)]
    pub preferences: Option<Value>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateProjectInput {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub enabled: Option<bool>,
    #[serde(default)]
    pub issue_regex: Option<String>,
    #[serde(default)]
    pub preferences: Option<Value>,
}
