use std::fmt;

/// Ошибки доменного уровня репозиториев — отдельно от "просто SQLite
/// упало", чтобы вызывающий код (и тесты) мог различать "данные
/// нарушают инвариант" от "БД недоступна/повреждена".
#[derive(Debug)]
pub enum RepoError {
    Sqlite(rusqlite::Error),
    /// ТЗ, раздел 4: `end > start`. Нарушение этого инварианта — не
    /// SQL-ошибка (CHECK-constraint в схеме тоже это проверяет, как второй
    /// рубеж защиты — см. миграцию 0001), а ошибка уровня приложения с
    /// понятным сообщением до похода в БД.
    InvalidInterval { started_at_utc: i64, ended_at_utc: i64 },
    /// `split()` требует точку строго внутри `[started_at_utc,
    /// ended_at_utc)` и наличия `ended_at_utc` (нельзя разбить открытую
    /// сессию — непонятно, где заканчивается вторая половина).
    InvalidSplitPoint { reason: String },
    NotFound(String),
}

impl fmt::Display for RepoError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RepoError::Sqlite(e) => write!(f, "sqlite error: {e}"),
            RepoError::InvalidInterval { started_at_utc, ended_at_utc } => write!(
                f,
                "session interval invalid: endedAtUtc ({ended_at_utc}) must be > startedAtUtc ({started_at_utc})"
            ),
            RepoError::InvalidSplitPoint { reason } => write!(f, "invalid split point: {reason}"),
            RepoError::NotFound(what) => write!(f, "not found: {what}"),
        }
    }
}

impl std::error::Error for RepoError {}

impl From<rusqlite::Error> for RepoError {
    fn from(e: rusqlite::Error) -> Self {
        RepoError::Sqlite(e)
    }
}

pub type RepoResult<T> = Result<T, RepoError>;
