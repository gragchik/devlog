import type Database from 'better-sqlite3'
import { randomUUID } from 'node:crypto'
import { mkdtempSync, rmSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { createDatabase } from '../../../src/main/db/database'

export interface TempDatabase {
  db: Database.Database
  filePath: string
  /** Закрывает соединение и удаляет временную директорию (включая WAL/SHM-файлы). */
  cleanup: () => void
}

/**
 * Реальный файл на диске (не `:memory:`) — нужен, чтобы тесты на
 * "перезапуск не теряет записи" могли закрыть одно соединение и открыть
 * новое на том же пути.
 */
export function createTempDatabase(): TempDatabase {
  const dir = mkdtempSync(join(tmpdir(), 'devlog-test-'))
  const filePath = join(dir, `${randomUUID()}.sqlite3`)
  const db = createDatabase(filePath)
  return {
    db,
    filePath,
    cleanup: () => {
      db.close()
      rmSync(dir, { recursive: true, force: true })
    }
  }
}
