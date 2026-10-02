//! Git CLI adapter (FR-03.2): только текущая ветка через `git branch
//! --show-current`, без чтения исходников/истории/конфига. Требует `git`
//! в PATH пользователя — обычное условие для разработчика.
//!
//! Как и Win32-адаптер, не покрыт unit-тестами (реальный `git`-процесс и
//! файловая система) — логика приоритета/резолюшна, которая использует
//! результат этой функции, тестируется отдельно и полностью
//! (`git_context::resolve`).

use std::path::Path;
use std::process::Command;
use std::time::SystemTime;

#[derive(Debug, Clone, PartialEq)]
pub enum GitBranchStatus {
    OnBranch(String),
    /// `git branch --show-current` возвращает пустую строку в detached
    /// HEAD — это не ошибка, а документированное поведение git.
    DetachedHead,
    NotARepo,
    /// `git` не найден в PATH или другая ошибка запуска процесса.
    GitUnavailable(String),
}

pub fn get_current_branch(repo_path: &Path) -> GitBranchStatus {
    let output = Command::new("git").arg("-C").arg(repo_path).arg("branch").arg("--show-current").output();

    let output = match output {
        Ok(o) => o,
        Err(err) => return GitBranchStatus::GitUnavailable(err.to_string()),
    };

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        if stderr.contains("not a git repository") {
            return GitBranchStatus::NotARepo;
        }
        return GitBranchStatus::GitUnavailable(stderr.trim().to_string());
    }

    let branch = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if branch.is_empty() {
        GitBranchStatus::DetachedHead
    } else {
        GitBranchStatus::OnBranch(branch)
    }
}

/// Время последнего изменения `.git/HEAD` — используется как недорогая
/// эвристика "какой из нескольких настроенных репозиториев сейчас активен"
/// (переключение ветки/коммит/merge всегда трогают `HEAD`). См.
/// `git_context::pick_most_recently_active`.
pub fn head_mtime(repo_path: &Path) -> Option<SystemTime> {
    std::fs::metadata(repo_path.join(".git").join("HEAD")).and_then(|m| m.modified()).ok()
}
