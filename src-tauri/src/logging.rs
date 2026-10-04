//! Диагностический лог (Итерация 9: "безопасное логирование, redaction").
//!
//! Один файл `devlog.log` в `app_log_dir()`, ротация по размеру (текущий +
//! один предыдущий). **Каждая** строка проходит `redact()` перед записью —
//! вызывающему коду не нужно помнить, что можно логировать, а что нет:
//! токены, Basic/Bearer-заголовки, пароли, e-mail и длинные непрозрачные
//! строки вырезаются всегда (NFR раздел 7, E2E-сценарий 10).
//!
//! Что сюда НЕ пишется по построению: заголовки окон, содержимое
//! документов, clipboard, нажатия клавиш — трекер их вообще не собирает
//! (FR-02.4).

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, Mutex, OnceLock};

use regex::Regex;

const LOG_FILE: &str = "devlog.log";
const ROTATED_FILE: &str = "devlog.log.1";
const MAX_LOG_BYTES: u64 = 1024 * 1024;

static LOG_DIR: OnceLock<PathBuf> = OnceLock::new();
/// Сериализует запись и ротацию между потоками (трекер, команды, panic hook).
static WRITE_LOCK: Mutex<()> = Mutex::new(());

#[derive(Debug, Clone, Copy)]
enum Level {
    Info,
    Warn,
    Error,
}

impl Level {
    fn as_str(self) -> &'static str {
        match self {
            Level::Info => "INFO",
            Level::Warn => "WARN",
            Level::Error => "ERROR",
        }
    }
}

/// Порядок важен: сначала специфичные шаблоны (заголовки, ключ=значение),
/// потом общий "длинная непрозрачная строка".
static REDACTIONS: LazyLock<Vec<(Regex, &'static str)>> = LazyLock::new(|| {
    let rules: [(&str, &'static str); 6] = [
        // Authorization: Basic xxx / Bearer xxx
        (r"(?i)\b(basic|bearer)\s+[A-Za-z0-9+/=._~-]{6,}", "$1 <redacted>"),
        // token=..., "apiToken": "...", password: ..., secret=...
        (r#"(?i)((?:api[_-]?token|access[_-]?token|token|password|passwd|pwd|secret|api[_-]?key)"?\s*[:=]\s*"?)[^\s"&,;}]+"#, "$1<redacted>"),
        // https://user:pass@host
        (r"(?i)(https?://)[^/\s:@]+:[^/\s@]+@", "$1<redacted>@"),
        // Atlassian API token
        (r"ATATT[A-Za-z0-9_=-]+", "<redacted>"),
        (r"[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,}", "<email>"),
        // Любая длинная непрозрачная строка (токены, ключи, base64).
        (r"\b[A-Za-z0-9+/_-]{32,}={0,2}", "<redacted>"),
    ];
    rules.into_iter().map(|(pattern, replacement)| (Regex::new(pattern).expect("static regex"), replacement)).collect()
});

/// Вырезает секреты и персональные данные из строки. Используется и для
/// лога, и для текста ошибок, уходящего в UI диагностики.
pub fn redact(text: &str) -> String {
    let mut out = text.to_string();
    for (regex, replacement) in REDACTIONS.iter() {
        out = regex.replace_all(&out, *replacement).into_owned();
    }
    out
}

/// Вызывается один раз в `setup()`. До вызова строки идут только в stderr.
pub fn init(dir: &Path) -> std::io::Result<()> {
    fs::create_dir_all(dir)?;
    let _ = LOG_DIR.set(dir.to_path_buf());
    install_panic_hook();
    Ok(())
}

pub fn log_file_path() -> Option<PathBuf> {
    LOG_DIR.get().map(|d| d.join(LOG_FILE))
}

fn format_line(level: Level, area: &str, message: &str) -> String {
    let ts = chrono::Local::now().format("%Y-%m-%d %H:%M:%S%.3f %z");
    // Перевод строки внутри сообщения сломал бы построчный `tail`.
    let one_line = redact(message).replace(['\r', '\n'], " ⏎ ");
    format!("{ts} {} [{area}] {one_line}", level.as_str())
}

fn rotate_if_needed(dir: &Path) {
    let current = dir.join(LOG_FILE);
    if fs::metadata(&current).map(|m| m.len() >= MAX_LOG_BYTES).unwrap_or(false) {
        let _ = fs::rename(&current, dir.join(ROTATED_FILE));
    }
}

fn write(level: Level, area: &str, message: &str) {
    let line = format_line(level, area, message);
    if cfg!(debug_assertions) {
        eprintln!("{line}");
    }
    let Some(dir) = LOG_DIR.get() else {
        if !cfg!(debug_assertions) {
            eprintln!("{line}");
        }
        return;
    };
    let _guard = WRITE_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    rotate_if_needed(dir);
    // Ошибку записи лога некуда сообщить, кроме stderr — и это не повод
    // ронять трекер.
    if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(dir.join(LOG_FILE)) {
        let _ = writeln!(file, "{line}");
    }
}

pub fn info(area: &str, message: impl AsRef<str>) {
    write(Level::Info, area, message.as_ref());
}

pub fn warn(area: &str, message: impl AsRef<str>) {
    write(Level::Warn, area, message.as_ref());
}

pub fn error(area: &str, message: impl AsRef<str>) {
    write(Level::Error, area, message.as_ref());
}

/// Последние `max_lines` строк лога (текущий файл, при нехватке — и
/// ротированный) — для экрана диагностики. Строки уже прошли redaction
/// при записи.
pub fn tail(max_lines: usize) -> Vec<String> {
    let Some(dir) = LOG_DIR.get() else { return Vec::new() };
    tail_in(dir, max_lines)
}

fn tail_in(dir: &Path, max_lines: usize) -> Vec<String> {
    let read = |name: &str| fs::read_to_string(dir.join(name)).unwrap_or_default();
    let combined = format!("{}{}", read(ROTATED_FILE), read(LOG_FILE));
    let lines: Vec<&str> = combined.lines().collect();
    lines[lines.len().saturating_sub(max_lines)..].iter().map(|s| s.to_string()).collect()
}

/// Паника в любом потоке (трекер, команды) попадает в лог — иначе после
/// "приложение перестало считать время" не было бы никакого следа.
fn install_panic_hook() {
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let location = info.location().map(|l| format!("{}:{}", l.file(), l.line())).unwrap_or_default();
        let payload = info
            .payload()
            .downcast_ref::<&str>()
            .map(|s| s.to_string())
            .or_else(|| info.payload().downcast_ref::<String>().cloned())
            .unwrap_or_else(|| "<non-string panic>".into());
        let thread = std::thread::current().name().unwrap_or("unnamed").to_string();
        error("panic", format!("поток {thread} at {location}: {payload}"));
        default_hook(info);
    }));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redacts_authorization_headers() {
        let out = redact("Authorization: Basic dXNlckBleGFtcGxlLmNvbTpzZWNyZXQ= sent");
        assert!(!out.contains("dXNlck"), "{out}");
        assert!(out.contains("Basic <redacted>"));
        assert!(!redact("Bearer abc.def.ghijkl").contains("abc.def"));
    }

    #[test]
    fn redacts_key_value_secrets_in_text_and_json() {
        for input in [
            "api_token=supersecret123",
            r#"{"apiToken": "supersecret123", "x": 1}"#,
            "password: supersecret123",
            "?token=supersecret123&a=b",
        ] {
            let out = redact(input);
            assert!(!out.contains("supersecret123"), "{input} -> {out}");
        }
    }

    #[test]
    fn redacts_atlassian_tokens_emails_and_url_credentials() {
        let out = redact("token ATATT3xFfGF0abc for dev@example.com at https://bob:pw@acme.atlassian.net/x");
        assert!(!out.contains("ATATT3x"), "{out}");
        assert!(!out.contains("dev@example.com"), "{out}");
        assert!(!out.contains("bob:pw"), "{out}");
        assert!(out.contains("acme.atlassian.net"), "хост оставляем — нужен для диагностики: {out}");
    }

    #[test]
    fn redacts_long_opaque_strings_but_keeps_normal_text() {
        let out = redact("key=Zm9vYmFyYmF6cXV4cXV1eGNvcmdlZ3JhdWx0Z2FycGx5d2FsZG8 issue OB-448 failed with 401");
        assert!(!out.contains("Zm9vYmFy"), "{out}");
        assert!(out.contains("OB-448") && out.contains("401"), "{out}");
    }

    #[test]
    fn format_line_is_single_line_and_redacted() {
        let line = format_line(Level::Warn, "jira", "boom\nBasic c2VjcmV0c2VjcmV0");
        assert!(!line.contains('\n'));
        assert!(line.contains("WARN [jira]"));
        assert!(!line.contains("c2VjcmV0"));
    }

    #[test]
    fn rotation_and_tail_keep_recent_lines() {
        let dir = tempfile::TempDir::new().unwrap();
        fs::write(dir.path().join(ROTATED_FILE), "old-1\nold-2\n").unwrap();
        fs::write(dir.path().join(LOG_FILE), "new-1\nnew-2\n").unwrap();
        assert_eq!(tail_in(dir.path(), 3), vec!["old-2", "new-1", "new-2"]);

        fs::write(dir.path().join(LOG_FILE), vec![b'x'; MAX_LOG_BYTES as usize]).unwrap();
        rotate_if_needed(dir.path());
        assert!(!dir.path().join(LOG_FILE).exists());
        assert_eq!(fs::metadata(dir.path().join(ROTATED_FILE)).unwrap().len(), MAX_LOG_BYTES);
    }
}
