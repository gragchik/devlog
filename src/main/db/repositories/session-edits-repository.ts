import type Database from 'better-sqlite3'
import type { CreateSessionEditInput, SessionEdit } from '@shared/types/session-edit'
import type { WorkSession } from '@shared/types/work-session'
import { generateId, nowUtcSeconds } from '../ids'

interface SessionEditRow {
  id: string
  sessionId: string
  editedAtUtc: number
  operation: string
  previousStateJson: string
  nextStateJson: string
}

function rowToEdit(row: SessionEditRow): SessionEdit {
  return {
    id: row.id,
    sessionId: row.sessionId,
    editedAtUtc: row.editedAtUtc,
    operation: row.operation as SessionEdit['operation'],
    previousState: JSON.parse(row.previousStateJson) as WorkSession,
    nextState: JSON.parse(row.nextStateJson) as WorkSession
  }
}

/**
 * Insert-only журнал правок (FR-04.2/FR-04.8). Используется изнутри
 * `WorkSessionsRepository` (одна транзакция на мутацию + запись в журнал) —
 * отдельно вызывать `append` имеет смысл только из другого репозитория,
 * который тоже меняет сессию атомарно.
 */
export class SessionEditsRepository {
  constructor(private readonly db: Database.Database) {}

  append(input: CreateSessionEditInput): SessionEdit {
    const id = generateId()
    const editedAtUtc = nowUtcSeconds()
    this.db
      .prepare(
        `INSERT INTO session_edits (id, sessionId, editedAtUtc, operation, previousStateJson, nextStateJson)
         VALUES (@id, @sessionId, @editedAtUtc, @operation, @previousStateJson, @nextStateJson)`
      )
      .run({
        id,
        sessionId: input.sessionId,
        editedAtUtc,
        operation: input.operation,
        previousStateJson: JSON.stringify(input.previousState),
        nextStateJson: JSON.stringify(input.nextState)
      })
    return { ...input, id, editedAtUtc }
  }

  listBySession(sessionId: string): SessionEdit[] {
    const rows = this.db
      .prepare('SELECT * FROM session_edits WHERE sessionId = ? ORDER BY editedAtUtc')
      .all(sessionId) as SessionEditRow[]
    return rows.map(rowToEdit)
  }

  getLatestForSession(sessionId: string): SessionEdit | null {
    const row = this.db
      .prepare('SELECT * FROM session_edits WHERE sessionId = ? ORDER BY editedAtUtc DESC, rowid DESC LIMIT 1')
      .get(sessionId) as SessionEditRow | undefined
    return row ? rowToEdit(row) : null
  }

  /**
   * FR-04.8: "журнал изменений, минимум для последних 30 дней" — удаляет
   * записи старше `olderThanUtc`, но вызывающий обязан сам выбрать порог
   * так, чтобы оставались как минимум 30 дней истории (см. вызов в
   * `app-database.ts`).
   */
  pruneOlderThan(olderThanUtc: number): number {
    const result = this.db.prepare('DELETE FROM session_edits WHERE editedAtUtc < ?').run(olderThanUtc)
    return result.changes
  }
}
