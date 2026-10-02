//! Хэш payload для обнаружения дублей (FR-07.6/7.7: "хэш payload",
//! "проверять локальные дубликаты... перед публикацией"). Чистая функция.

use std::fmt::Write as _;

use sha2::{Digest, Sha256};

pub fn compute_payload_hash(issue_key: &str, started_at_utc: i64, time_spent_seconds: i64, comment: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(issue_key.as_bytes());
    hasher.update(b"|");
    hasher.update(started_at_utc.to_le_bytes());
    hasher.update(b"|");
    hasher.update(time_spent_seconds.to_le_bytes());
    hasher.update(b"|");
    hasher.update(comment.as_bytes());

    // `GenericArray` из sha2 0.11 не реализует `LowerHex` напрямую (только
    // через generic-blanket, который тут не подходит) — собираем hex-строку
    // вручную побайтово вместо `format!("{:x}", ...)`.
    let digest = hasher.finalize();
    let mut hex = String::with_capacity(digest.len() * 2);
    for byte in digest {
        write!(hex, "{byte:02x}").expect("writing to String cannot fail");
    }
    hex
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_input_gives_same_hash() {
        let a = compute_payload_hash("OB-448", 1_700_000_000, 3600, "работа");
        let b = compute_payload_hash("OB-448", 1_700_000_000, 3600, "работа");
        assert_eq!(a, b);
    }

    #[test]
    fn different_comment_gives_different_hash() {
        let a = compute_payload_hash("OB-448", 1_700_000_000, 3600, "работа");
        let b = compute_payload_hash("OB-448", 1_700_000_000, 3600, "другая работа");
        assert_ne!(a, b);
    }

    #[test]
    fn different_issue_key_gives_different_hash() {
        let a = compute_payload_hash("OB-448", 1_700_000_000, 3600, "работа");
        let b = compute_payload_hash("OB-449", 1_700_000_000, 3600, "работа");
        assert_ne!(a, b);
    }

    #[test]
    fn different_time_spent_gives_different_hash() {
        let a = compute_payload_hash("OB-448", 1_700_000_000, 3600, "работа");
        let b = compute_payload_hash("OB-448", 1_700_000_000, 1800, "работа");
        assert_ne!(a, b);
    }
}
