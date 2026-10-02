use regex::Regex;

/// Извлекает issue key из имени ветки по настраиваемому паттерну
/// (FR-03.3). Перед матчингом ветка нормализуется: подчёркивания → дефисы,
/// приведение к верхнему регистру — ТЗ: "регистр и разделители
/// нормализовать", например `feature/ob_448-mass-payments` →
/// `FEATURE/OB-448-MASS-PAYMENTS` → матчится `OB-448`.
///
/// Известное (принятое) ограничение эвристики: если в ветке встречается
/// слово, случайно похожее на issue key (например `fix/grid-492-layout`
/// при дефолтном regex даст `GRID-492`), это ложное срабатывание — та же
/// цена, что и в оригинальном плане на Electron. Смягчается настраиваемым
/// `issue_regex` на проект (уже есть в `domain::project`).
pub fn extract_issue_key(branch: &str, pattern: &str) -> Option<String> {
    let normalized = branch.replace('_', "-").to_uppercase();
    let regex = Regex::new(pattern).ok()?;
    regex.find(&normalized).map(|m| m.as_str().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::project::DEFAULT_ISSUE_REGEX;

    #[test]
    fn extracts_key_from_typical_feature_branch() {
        assert_eq!(extract_issue_key("feature/OB-448-mass-payments", DEFAULT_ISSUE_REGEX), Some("OB-448".to_string()));
    }

    #[test]
    fn normalizes_lowercase_and_underscore() {
        assert_eq!(extract_issue_key("feature/ob_448_mass_payments", DEFAULT_ISSUE_REGEX), Some("OB-448".to_string()));
    }

    #[test]
    fn main_branch_has_no_issue_key() {
        assert_eq!(extract_issue_key("main", DEFAULT_ISSUE_REGEX), None);
        assert_eq!(extract_issue_key("master", DEFAULT_ISSUE_REGEX), None);
        assert_eq!(extract_issue_key("develop", DEFAULT_ISSUE_REGEX), None);
    }

    #[test]
    fn respects_custom_pattern_per_project() {
        // Проект с нестандартной схемой ключей (например, только цифры после префикса "TASK").
        let custom_pattern = r"TASK-\d+";
        assert_eq!(extract_issue_key("task-42-refactor", custom_pattern), Some("TASK-42".to_string()));
        assert_eq!(extract_issue_key("OB-448-mass-payments", custom_pattern), None);
    }

    #[test]
    fn invalid_pattern_returns_none_instead_of_panicking() {
        assert_eq!(extract_issue_key("feature/OB-448", "(unclosed"), None);
    }
}
