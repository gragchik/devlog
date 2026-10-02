import type Database from 'better-sqlite3'
import { MIGRATIONS } from './migrations'

/**
 * Применяет все ещё не применённые миграции из `MIGRATIONS`, по порядку,
 * каждую в своей транзакции (атомарно: либо вся миграция применилась, либо
 * откатилась целиком при ошибке). Таблица `_migrations` — журнал того, что
 * уже применено, чтобы повторный запуск на той же БД был no-op.
 */
export function runMigrations(db: Database.Database): void {
  db.exec(`
    CREATE TABLE IF NOT EXISTS _migrations (
      version INTEGER PRIMARY KEY,
      name TEXT NOT NULL,
      appliedAtUtc INTEGER NOT NULL
    );
  `)

  const appliedVersions = new Set(
    db.prepare('SELECT version FROM _migrations').all().map((row) => (row as { version: number }).version)
  )

  const pending = [...MIGRATIONS]
    .sort((a, b) => a.version - b.version)
    .filter((migration) => !appliedVersions.has(migration.version))

  for (const migration of pending) {
    const applyMigration = db.transaction(() => {
      migration.up(db)
      db.prepare('INSERT INTO _migrations (version, name, appliedAtUtc) VALUES (?, ?, ?)').run(
        migration.version,
        migration.name,
        Math.floor(Date.now() / 1000)
      )
    })
    applyMigration()
  }
}
