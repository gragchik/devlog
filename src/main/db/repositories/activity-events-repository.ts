import type Database from 'better-sqlite3'
import type { ActivityEvent, CreateActivityEventInput } from '@shared/types/activity-event'
import { generateId, nowUtcSeconds } from '../ids'

interface ActivityEventRow {
  id: string
  timestampUtc: number
  processNameSanitized: string
  appCategory: string
  projectId: string | null
  branch: string | null
  detectedIssueKey: string | null
  idleSeconds: number | null
  state: string
  confidence: string
  reason: string
  createdAtUtc: number
}

function rowToEvent(row: ActivityEventRow): ActivityEvent {
  return {
    id: row.id,
    timestampUtc: row.timestampUtc,
    processNameSanitized: row.processNameSanitized,
    appCategory: row.appCategory,
    projectId: row.projectId,
    branch: row.branch,
    detectedIssueKey: row.detectedIssueKey,
    idleSeconds: row.idleSeconds,
    state: row.state as ActivityEvent['state'],
    confidence: row.confidence as ActivityEvent['confidence'],
    reason: row.reason,
    createdAtUtc: row.createdAtUtc
  }
}

/**
 * Сырой, неизменяемый слой наблюдений (FR-02, FR-04.2). Только `insert` +
 * чтение — здесь намеренно нет `update`/`delete`: правки происходят только
 * на уровне `work_sessions`, первичная история событий не трогается.
 */
export class ActivityEventsRepository {
  constructor(private readonly db: Database.Database) {}

  insert(input: CreateActivityEventInput): ActivityEvent {
    const id = generateId()
    const createdAtUtc = nowUtcSeconds()
    this.db
      .prepare(
        `INSERT INTO activity_events
           (id, timestampUtc, processNameSanitized, appCategory, projectId, branch,
            detectedIssueKey, idleSeconds, state, confidence, reason, createdAtUtc)
         VALUES
           (@id, @timestampUtc, @processNameSanitized, @appCategory, @projectId, @branch,
            @detectedIssueKey, @idleSeconds, @state, @confidence, @reason, @createdAtUtc)`
      )
      .run({
        id,
        timestampUtc: input.timestampUtc,
        processNameSanitized: input.processNameSanitized,
        appCategory: input.appCategory,
        projectId: input.projectId,
        branch: input.branch,
        detectedIssueKey: input.detectedIssueKey,
        idleSeconds: input.idleSeconds,
        state: input.state,
        confidence: input.confidence,
        reason: input.reason,
        createdAtUtc
      })
    return { ...input, id, createdAtUtc }
  }

  /** Для сидирования фикстур в тестах — одна транзакция на весь набор. */
  insertMany(inputs: readonly CreateActivityEventInput[]): ActivityEvent[] {
    const insertAll = this.db.transaction((items: readonly CreateActivityEventInput[]) => {
      return items.map((item) => this.insert(item))
    })
    return insertAll(inputs)
  }

  listByRange(startUtc: number, endUtcExclusive: number): ActivityEvent[] {
    const rows = this.db
      .prepare('SELECT * FROM activity_events WHERE timestampUtc >= ? AND timestampUtc < ? ORDER BY timestampUtc')
      .all(startUtc, endUtcExclusive) as ActivityEventRow[]
    return rows.map(rowToEvent)
  }
}
