use std::time::{SystemTime, UNIX_EPOCH};
use uuid::Uuid;

pub fn generate_id() -> String {
    Uuid::new_v4().to_string()
}

/// Текущее время как эпоха в секундах UTC — единый формат хранения времени в БД.
pub fn now_utc_seconds() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock before UNIX_EPOCH")
        .as_secs() as i64
}
