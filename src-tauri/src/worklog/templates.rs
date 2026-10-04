//! Шаблоны комментариев worklog (FR-08.1–8.3). Без AI: текст собирается
//! только из подтверждённых метаданных — ключ и название задачи, дата,
//! время и категории приложений, в которых шла работа. Никаких
//! утверждений о результате ("исправлен баг", "прошли тесты") шаблоны по
//! умолчанию не содержат (FR-08.2); пользовательские шаблоны — это уже
//! текст самого пользователя.

use std::collections::HashMap;

use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use crate::db::ids::generate_id;
use crate::db::repositories::settings;

pub const SETTINGS_KEY_TEMPLATES: &str = "worklog.templates";

/// Плейсхолдеры, которые понимает `render`. Любой другой `{...}` в шаблоне
/// — ошибка при сохранении (опечатка иначе молча ушла бы в Jira).
pub const PLACEHOLDERS: &[&str] = &["issueKey", "issueTitle", "activity", "date", "duration"];

const MAX_TEMPLATES: usize = 50;
const MAX_TEMPLATE_CHARS: usize = 2000;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommentTemplate {
    pub id: String,
    pub name: String,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TemplateSettings {
    pub templates: Vec<CommentTemplate>,
    /// Шаблон для новых черновиков ("Сформировать из сессий").
    pub default_template_id: String,
}

/// Данные одного черновика для подстановки.
pub struct TemplateContext<'a> {
    pub issue_key: &'a str,
    pub issue_title: Option<&'a str>,
    pub activity: Option<&'a str>,
    pub local_day: &'a str,
    pub time_spent_seconds: i64,
}

fn builtin(id: &str, name: &str, text: &str) -> CommentTemplate {
    CommentTemplate { id: id.into(), name: name.into(), text: text.into() }
}

pub fn default_settings() -> TemplateSettings {
    TemplateSettings {
        templates: vec![
            builtin("builtin-key", "Только задача", "Работа над задачей {issueKey}"),
            builtin("builtin-title", "Задача и название", "Работа над задачей {issueKey}: {issueTitle}"),
            builtin("builtin-activity", "Задача и где шла работа", "Работа над задачей {issueKey} ({activity})"),
        ],
        default_template_id: "builtin-activity".into(),
    }
}

/// Сохранённые шаблоны или встроенные, если ничего не сохранено / JSON
/// битый (настройка, а не данные — молчаливый fallback как у whitelist).
pub fn load(conn: &Connection) -> TemplateSettings {
    settings::get(conn, SETTINGS_KEY_TEMPLATES)
        .ok()
        .flatten()
        .and_then(|json| serde_json::from_str::<TemplateSettings>(&json).ok())
        .filter(|s| validate(s).is_ok())
        .unwrap_or_else(default_settings)
}

/// Проверяет и сохраняет. Пустой `id` у шаблона — новый шаблон, id
/// выдаётся здесь.
pub fn save(conn: &Connection, mut value: TemplateSettings) -> Result<TemplateSettings, String> {
    for t in &mut value.templates {
        t.name = t.name.trim().to_string();
        if t.id.trim().is_empty() {
            t.id = generate_id();
        }
    }
    validate(&value)?;
    let json = serde_json::to_string(&value).map_err(|e| e.to_string())?;
    settings::set(conn, SETTINGS_KEY_TEMPLATES, &json).map_err(|e| e.to_string())?;
    Ok(value)
}

fn unknown_placeholders(text: &str) -> Vec<String> {
    let mut unknown = Vec::new();
    let mut rest = text;
    while let Some(open) = rest.find('{') {
        let after = &rest[open + 1..];
        let Some(close) = after.find('}') else { break };
        let name = &after[..close];
        if !PLACEHOLDERS.contains(&name) {
            unknown.push(name.to_string());
        }
        rest = &after[close + 1..];
    }
    unknown
}

pub fn validate(value: &TemplateSettings) -> Result<(), String> {
    if value.templates.is_empty() {
        return Err("нужен хотя бы один шаблон".into());
    }
    if value.templates.len() > MAX_TEMPLATES {
        return Err(format!("не больше {MAX_TEMPLATES} шаблонов"));
    }
    let mut ids = std::collections::HashSet::new();
    for t in &value.templates {
        if t.name.is_empty() {
            return Err("у шаблона должно быть название".into());
        }
        if t.text.trim().is_empty() {
            return Err(format!("шаблон «{}» пустой", t.name));
        }
        if t.text.chars().count() > MAX_TEMPLATE_CHARS {
            return Err(format!("шаблон «{}» длиннее {MAX_TEMPLATE_CHARS} символов", t.name));
        }
        let unknown = unknown_placeholders(&t.text);
        if !unknown.is_empty() {
            return Err(format!("шаблон «{}»: неизвестные поля {{{}}}; доступны {{{}}}", t.name, unknown.join("}, {"), PLACEHOLDERS.join("}, {")));
        }
        if !ids.insert(t.id.as_str()) {
            return Err("повторяющийся id шаблона".into());
        }
    }
    if !value.templates.iter().any(|t| t.id == value.default_template_id) {
        return Err("шаблон по умолчанию не найден в списке".into());
    }
    Ok(())
}

fn format_duration(seconds: i64) -> String {
    let minutes = seconds / 60;
    match (minutes / 60, minutes % 60) {
        (0, m) => format!("{m}м"),
        (h, 0) => format!("{h}ч"),
        (h, m) => format!("{h}ч {m}м"),
    }
}

/// Убирает следы пустых плейсхолдеров: `()`, висящие `:`/`—`/`,` в конце
/// строки, двойные пробелы. Иначе "Работа над задачей OB-1: " при
/// незагруженном названии выглядела бы как недописанный текст.
fn tidy_line(line: &str) -> String {
    let mut s = line.replace("( )", "").replace("()", "");
    while s.contains("  ") {
        s = s.replace("  ", " ");
    }
    s = s.replace(" ,", ",").replace(" )", ")").replace("( ", "(");
    s.trim().trim_end_matches([':', '—', '-', ',', ';']).trim_end().to_string()
}

pub fn render(text: &str, ctx: &TemplateContext) -> String {
    let values: HashMap<&str, String> = HashMap::from([
        ("issueKey", ctx.issue_key.to_string()),
        ("issueTitle", ctx.issue_title.unwrap_or("").to_string()),
        ("activity", ctx.activity.unwrap_or("").to_string()),
        ("date", ctx.local_day.to_string()),
        ("duration", format_duration(ctx.time_spent_seconds)),
    ]);
    let mut out = text.to_string();
    for (name, value) in &values {
        out = out.replace(&format!("{{{name}}}"), value);
    }
    out.lines().map(tidy_line).collect::<Vec<_>>().join("\n").trim().to_string()
}

/// Категории приложений (из whitelist) → человекочитаемый, но только
/// фактический текст: "IDE, терминал" — где шла работа, без выводов о том,
/// что именно сделано. Категории уже отсортированы по убыванию времени.
pub fn describe_activity(categories: &[String]) -> Option<String> {
    let labels: Vec<String> = categories
        .iter()
        .filter(|c| !c.is_empty() && c.as_str() != "excluded")
        .map(|c| match c.as_str() {
            "ide" => "IDE".to_string(),
            "terminal" => "терминал".to_string(),
            "browser" => "браузер".to_string(),
            other => other.to_string(),
        })
        .collect();
    if labels.is_empty() {
        None
    } else {
        Some(labels.join(", "))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::test_support::temp_database;

    fn ctx<'a>(title: Option<&'a str>, activity: Option<&'a str>) -> TemplateContext<'a> {
        TemplateContext { issue_key: "OB-448", issue_title: title, activity, local_day: "2026-10-03", time_spent_seconds: 5400 }
    }

    #[test]
    fn renders_all_placeholders() {
        let text = "{issueKey} {issueTitle} ({activity}) {date} {duration}";
        assert_eq!(render(text, &ctx(Some("Платежи"), Some("IDE"))), "OB-448 Платежи (IDE) 2026-10-03 1ч 30м");
    }

    #[test]
    fn empty_placeholders_leave_no_dangling_punctuation() {
        let d = default_settings();
        let by_id = |id: &str| d.templates.iter().find(|t| t.id == id).unwrap().text.clone();
        assert_eq!(render(&by_id("builtin-title"), &ctx(None, None)), "Работа над задачей OB-448");
        assert_eq!(render(&by_id("builtin-activity"), &ctx(None, None)), "Работа над задачей OB-448");
        assert_eq!(render(&by_id("builtin-activity"), &ctx(None, Some("IDE, терминал"))), "Работа над задачей OB-448 (IDE, терминал)");
    }

    #[test]
    fn builtin_templates_make_no_claims_about_results() {
        // FR-08.2: шаблоны по умолчанию не утверждают, что что-то
        // исправлено/реализовано/протестировано.
        for t in default_settings().templates {
            let lower = t.text.to_lowercase();
            for word in ["исправ", "реализ", "тест", "fix", "done", "готов"] {
                assert!(!lower.contains(word), "{} содержит «{word}»", t.text);
            }
        }
    }

    #[test]
    fn duration_formats() {
        assert_eq!(format_duration(60), "1м");
        assert_eq!(format_duration(3600), "1ч");
        assert_eq!(format_duration(5400), "1ч 30м");
    }

    #[test]
    fn validate_rejects_unknown_placeholder_and_missing_default() {
        let mut s = default_settings();
        s.templates[0].text = "Работа над {issue}".into();
        assert!(validate(&s).unwrap_err().contains("issue"));

        let mut s = default_settings();
        s.default_template_id = "nope".into();
        assert!(validate(&s).is_err());

        let mut s = default_settings();
        s.templates.clear();
        assert!(validate(&s).is_err());
    }

    #[test]
    fn save_assigns_ids_and_load_round_trips() {
        let (conn, _dir) = temp_database();
        assert_eq!(load(&conn), default_settings());

        let mut s = default_settings();
        s.templates.push(CommentTemplate { id: String::new(), name: " Созвон ".into(), text: "Обсуждение {issueKey}".into() });
        let saved = save(&conn, s).unwrap();
        let added = saved.templates.last().unwrap();
        assert!(!added.id.is_empty());
        assert_eq!(added.name, "Созвон");
        assert_eq!(load(&conn), saved);
    }

    #[test]
    fn invalid_stored_json_falls_back_to_defaults() {
        let (conn, _dir) = temp_database();
        settings::set(&conn, SETTINGS_KEY_TEMPLATES, "{broken").unwrap();
        assert_eq!(load(&conn), default_settings());
    }

    #[test]
    fn describe_activity_maps_known_categories_and_skips_excluded() {
        let cats = vec!["ide".to_string(), "excluded".to_string(), "terminal".to_string(), "design".to_string()];
        assert_eq!(describe_activity(&cats).as_deref(), Some("IDE, терминал, design"));
        assert_eq!(describe_activity(&["excluded".to_string()]), None);
    }
}
