mod m0001_initial_schema;
mod m0002_jira_tables;

use rusqlite::{Connection, Result};

pub struct Migration {
    pub version: i64,
    pub name: &'static str,
    pub up: fn(&Connection) -> Result<()>,
}

/// Порядок — по возрастанию `version`. Применяются по порядку, без пропусков.
pub fn all() -> Vec<Migration> {
    vec![
        Migration { version: 1, name: "initial_schema", up: m0001_initial_schema::up },
        Migration { version: 2, name: "jira_tables", up: m0002_jira_tables::up },
    ]
}

/// Применяет все ещё не применённые миграции, по порядку, каждую в своей
/// транзакции (атомарно: либо вся миграция применилась, либо откатилась
/// целиком при ошибке). Таблица `_migrations` — журнал того, что уже
/// применено, чтобы повторный запуск на той же БД был no-op.
pub fn run(conn: &mut Connection) -> Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS _migrations (
            version INTEGER PRIMARY KEY,
            name TEXT NOT NULL,
            appliedAtUtc INTEGER NOT NULL
        );",
    )?;

    let applied: Vec<i64> = {
        let mut stmt = conn.prepare("SELECT version FROM _migrations")?;
        let rows = stmt.query_map([], |row| row.get::<_, i64>(0))?;
        rows.collect::<Result<Vec<i64>>>()?
    };

    let mut migrations = all();
    migrations.sort_by_key(|m| m.version);

    for migration in migrations.into_iter().filter(|m| !applied.contains(&m.version)) {
        let tx = conn.transaction()?;
        (migration.up)(&tx)?;
        tx.execute(
            "INSERT INTO _migrations (version, name, appliedAtUtc) VALUES (?1, ?2, ?3)",
            rusqlite::params![migration.version, migration.name, super::ids::now_utc_seconds()],
        )?;
        tx.commit()?;
    }

    Ok(())
}
