//! Приоритет сопоставления активности с задачей (FR-03.4):
//! (1) явное закрепление в overlay — ещё не существует (Итерация 6),
//! принимается как параметр уже сейчас, чтобы не менять сигнатуру позже;
//! (2) активный репозиторий и его ветка — основное в этой итерации;
//! (3) пользовательское правило project → task (`preferences.defaultIssueKey`);
//! (4) «Нераспределено».
//!
//! Вся логика здесь чистая (принимает уже собранные данные, не делает
//! файловых/git-вызовов сама) — таким образом полностью unit-тестируема.
//! Реальный сбор (`git_adapter`, `head_mtime`) — тонкий и непроверяемый
//! unit-тестами слой, вызывается из `tracking::tracker`.

use std::time::SystemTime;

use crate::domain::confidence::Confidence;
use crate::domain::project::Project;
use crate::tracking::git_adapter::GitBranchStatus;
use crate::tracking::issue_key::extract_issue_key;

#[derive(Debug, Clone, PartialEq)]
pub struct GitContextResult {
    pub project_id: Option<String>,
    pub branch: Option<String>,
    pub issue_key: Option<String>,
    pub confidence: Confidence,
    pub reason: String,
}

fn unassigned(reason: impl Into<String>) -> GitContextResult {
    GitContextResult { project_id: None, branch: None, issue_key: None, confidence: Confidence::Low, reason: reason.into() }
}

/// `preferences.defaultIssueKey` — правило "этот репозиторий без issue-key
/// в ветке по умолчанию относится к такой-то задаче" (например,
/// обслуживание инфраструктуры без тикета на каждую мелочь).
fn project_default_issue_key(project: &Project) -> Option<String> {
    project.preferences.get("defaultIssueKey").and_then(|v| v.as_str()).map(str::to_string)
}

/// Среди кандидатов (проект + время последнего изменения `.git/HEAD`,
/// `None` если стат не удался/не git-репозиторий) выбирает тот, чей HEAD
/// менялся позже всех — недорогая эвристика "активного" репозитория при
/// нескольких настроенных (FR-03, правило 2). Без наблюдений времени —
/// берём первый по списку, лишь бы не вернуть `None` без необходимости.
pub fn pick_most_recently_active<'a>(candidates: &'a [(&'a Project, Option<SystemTime>)]) -> Option<&'a Project> {
    candidates
        .iter()
        .filter_map(|(p, mtime)| mtime.map(|m| (*p, m)))
        .max_by_key(|(_, mtime)| *mtime)
        .map(|(p, _)| p)
        .or_else(|| candidates.first().map(|(p, _)| *p))
}

/// `explicit_issue_key` — ручное закрепление задачи (overlay, Итерация 6).
/// Пока всегда `None` отовсюду, где вызывается эта функция в Итерации 3.
pub fn resolve(
    explicit_issue_key: Option<&str>,
    active_project: Option<&Project>,
    branch_status: Option<&GitBranchStatus>,
) -> GitContextResult {
    if let Some(key) = explicit_issue_key {
        return GitContextResult {
            project_id: active_project.map(|p| p.id.clone()),
            branch: None,
            issue_key: Some(key.to_string()),
            confidence: Confidence::High,
            reason: "explicit issue key pinned by user".to_string(),
        };
    }

    let Some(project) = active_project else {
        return unassigned("no configured project repo detected as active");
    };

    let Some(status) = branch_status else {
        return unassigned("active project has no branch status available");
    };

    match status {
        GitBranchStatus::OnBranch(branch) => {
            if let Some(key) = extract_issue_key(branch, &project.issue_regex) {
                GitContextResult {
                    project_id: Some(project.id.clone()),
                    branch: Some(branch.clone()),
                    issue_key: Some(key),
                    confidence: Confidence::High,
                    reason: format!("branch '{branch}' matched issue regex"),
                }
            } else if let Some(default_key) = project_default_issue_key(project) {
                GitContextResult {
                    project_id: Some(project.id.clone()),
                    branch: Some(branch.clone()),
                    issue_key: Some(default_key),
                    confidence: Confidence::Medium,
                    reason: format!("branch '{branch}' has no issue key, used project default rule"),
                }
            } else {
                GitContextResult {
                    project_id: Some(project.id.clone()),
                    branch: Some(branch.clone()),
                    issue_key: None,
                    confidence: Confidence::Low,
                    reason: format!("branch '{branch}' has no issue key and no default rule — unassigned"),
                }
            }
        }
        GitBranchStatus::DetachedHead => GitContextResult {
            project_id: Some(project.id.clone()),
            branch: None,
            issue_key: None,
            confidence: Confidence::Low,
            reason: "detached HEAD".to_string(),
        },
        GitBranchStatus::NotARepo => unassigned("configured path is not a git repository"),
        GitBranchStatus::GitUnavailable(err) => unassigned(format!("git unavailable: {err}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn project(issue_regex: &str, preferences: serde_json::Value) -> Project {
        Project {
            id: "p1".to_string(),
            name: "DevLog".to_string(),
            repo_path: "D:/soft/worklog".to_string(),
            enabled: true,
            issue_regex: issue_regex.to_string(),
            preferences,
            created_at_utc: 0,
            updated_at_utc: 0,
        }
    }

    #[test]
    fn feature_branch_resolves_with_high_confidence() {
        let p = project(crate::domain::project::DEFAULT_ISSUE_REGEX, json!({}));
        let status = GitBranchStatus::OnBranch("feature/OB-448-mass-payments".to_string());
        let result = resolve(None, Some(&p), Some(&status));
        assert_eq!(result.issue_key.as_deref(), Some("OB-448"));
        assert_eq!(result.confidence, Confidence::High);
        assert_eq!(result.project_id.as_deref(), Some("p1"));
    }

    #[test]
    fn main_branch_is_unassigned_not_fabricated() {
        let p = project(crate::domain::project::DEFAULT_ISSUE_REGEX, json!({}));
        let status = GitBranchStatus::OnBranch("main".to_string());
        let result = resolve(None, Some(&p), Some(&status));
        assert_eq!(result.issue_key, None);
        assert_eq!(result.confidence, Confidence::Low);
    }

    #[test]
    fn falls_back_to_project_default_issue_key_when_branch_has_none() {
        let p = project(crate::domain::project::DEFAULT_ISSUE_REGEX, json!({"defaultIssueKey": "OPS-1"}));
        let status = GitBranchStatus::OnBranch("main".to_string());
        let result = resolve(None, Some(&p), Some(&status));
        assert_eq!(result.issue_key.as_deref(), Some("OPS-1"));
        assert_eq!(result.confidence, Confidence::Medium);
    }

    #[test]
    fn explicit_issue_key_wins_over_branch_detection() {
        let p = project(crate::domain::project::DEFAULT_ISSUE_REGEX, json!({}));
        let status = GitBranchStatus::OnBranch("main".to_string());
        let result = resolve(Some("OB-999"), Some(&p), Some(&status));
        assert_eq!(result.issue_key.as_deref(), Some("OB-999"));
        assert_eq!(result.confidence, Confidence::High);
    }

    #[test]
    fn no_active_project_is_unassigned() {
        let result = resolve(None, None, None);
        assert_eq!(result.issue_key, None);
        assert_eq!(result.project_id, None);
    }

    #[test]
    fn detached_head_is_unassigned_but_keeps_project() {
        let p = project(crate::domain::project::DEFAULT_ISSUE_REGEX, json!({}));
        let result = resolve(None, Some(&p), Some(&GitBranchStatus::DetachedHead));
        assert_eq!(result.issue_key, None);
        assert_eq!(result.project_id.as_deref(), Some("p1"));
        assert_eq!(result.branch, None);
    }

    #[test]
    fn not_a_repo_is_fully_unassigned() {
        let p = project(crate::domain::project::DEFAULT_ISSUE_REGEX, json!({}));
        let result = resolve(None, Some(&p), Some(&GitBranchStatus::NotARepo));
        assert_eq!(result.project_id, None);
    }

    #[test]
    fn pick_most_recently_active_prefers_latest_head_mtime() {
        let p1 = project(crate::domain::project::DEFAULT_ISSUE_REGEX, json!({}));
        let mut p2 = p1.clone();
        p2.id = "p2".to_string();

        let earlier = SystemTime::UNIX_EPOCH;
        let later = SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1000);

        let candidates = [(&p1, Some(earlier)), (&p2, Some(later))];
        let picked = pick_most_recently_active(&candidates).unwrap();
        assert_eq!(picked.id, "p2");
    }

    #[test]
    fn pick_most_recently_active_falls_back_to_first_when_no_mtimes() {
        let p1 = project(crate::domain::project::DEFAULT_ISSUE_REGEX, json!({}));
        let candidates = [(&p1, None)];
        let picked = pick_most_recently_active(&candidates).unwrap();
        assert_eq!(picked.id, "p1");
    }
}
