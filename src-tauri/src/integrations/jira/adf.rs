//! Atlassian Document Format — формат комментария для Jira Cloud REST API
//! v3 (ТЗ FR-07.3: "comment — Atlassian Document Format"). Чистая функция,
//! не зависит от HTTP — полностью unit-тестируема.

use serde_json::{json, Value};

/// Превращает обычный многострочный текст в минимальный валидный ADF-документ
/// — один `paragraph`-узел на непустую строку. Пустые строки (двойной
/// перенос) дают отдельный пустой параграф — Jira это корректно отображает
/// как пустую строку между абзацами.
pub fn plain_text_to_adf(text: &str) -> Value {
    let paragraphs: Vec<Value> = text
        .split('\n')
        .map(|line| {
            if line.is_empty() {
                json!({ "type": "paragraph", "content": [] })
            } else {
                json!({
                    "type": "paragraph",
                    "content": [{ "type": "text", "text": line }]
                })
            }
        })
        .collect();

    json!({
        "type": "doc",
        "version": 1,
        "content": paragraphs
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_line_produces_one_paragraph_with_text() {
        let adf = plain_text_to_adf("Работа над задачей OB-448");
        assert_eq!(adf["type"], "doc");
        assert_eq!(adf["version"], 1);
        assert_eq!(adf["content"].as_array().unwrap().len(), 1);
        assert_eq!(adf["content"][0]["type"], "paragraph");
        assert_eq!(adf["content"][0]["content"][0]["type"], "text");
        assert_eq!(adf["content"][0]["content"][0]["text"], "Работа над задачей OB-448");
    }

    #[test]
    fn multiline_produces_one_paragraph_per_line() {
        let adf = plain_text_to_adf("Первая строка\nВторая строка");
        let content = adf["content"].as_array().unwrap();
        assert_eq!(content.len(), 2);
        assert_eq!(content[0]["content"][0]["text"], "Первая строка");
        assert_eq!(content[1]["content"][0]["text"], "Вторая строка");
    }

    #[test]
    fn empty_line_produces_empty_paragraph_not_a_crash() {
        let adf = plain_text_to_adf("Строка 1\n\nСтрока 2");
        let content = adf["content"].as_array().unwrap();
        assert_eq!(content.len(), 3);
        assert_eq!(content[1]["content"].as_array().unwrap().len(), 0);
    }

    #[test]
    fn empty_string_produces_single_empty_paragraph() {
        let adf = plain_text_to_adf("");
        assert_eq!(adf["content"].as_array().unwrap().len(), 1);
    }
}
