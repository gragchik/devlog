import type Database from 'better-sqlite3'
import { app } from 'electron'
import { join } from 'node:path'
import { createDatabase } from './database'
import { nowUtcSeconds } from './ids'
import { SessionEditsRepository } from './repositories/session-edits-repository'

let dbInstance: Database.Database | null = null

const SESSION_EDITS_RETENTION_DAYS = 30
const SECONDS_PER_DAY = 86_400

/**
 * Единственный экземпляр БД приложения, по пути `app.getPath('userData')`
 * (ТЗ, раздел 5). Main window и overlay работают через один и тот же
 * процесс main и, значит, через один и тот же `dbInstance` — отдельного
 * хранилища для overlay нет и не будет (ТЗ: "Все изменения из overlay и
 * основного окна выполняются через один сервис").
 */
export function getAppDatabase(): Database.Database {
  if (!dbInstance) {
    const dbPath = join(app.getPath('userData'), 'devlog.sqlite3')
    dbInstance = createDatabase(dbPath)
    // FR-04.8: держим минимум 30 дней журнала правок — старше можно чистить
    // при каждом старте, не дожидаясь отдельного maintenance-задания.
    new SessionEditsRepository(dbInstance).pruneOlderThan(
      nowUtcSeconds() - SESSION_EDITS_RETENTION_DAYS * SECONDS_PER_DAY
    )
  }
  return dbInstance
}

export function closeAppDatabase(): void {
  if (dbInstance) {
    dbInstance.close()
    dbInstance = null
  }
}
