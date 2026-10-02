import type Database from 'better-sqlite3'
import type { CreateWorkSessionInput, UpdateWorkSessionPatch, WorkSession } from '@shared/types/work-session'
import { generateId, nowUtcSeconds } from '../ids'
import { SessionEditsRepository } from './session-edits-repository'

interface WorkSessionRow {
  id: string
  startedAtUtc: number
  endedAtUtc: number | null
  timezoneId: string
  projectId: string | null
  issueKey: string | null
  activeSeconds: number
  manualSecondsOverride: number | null
  description: string | null
  source: string
  confidence: string
  reviewStatus: string
  isManuallyEdited: number
  deletedAt: number | null
  createdAtUtc: number
  updatedAtUtc: number
}

function rowToSession(row: WorkSessionRow): WorkSession {
  return {
    id: row.id,
    startedAtUtc: row.startedAtUtc,
    endedAtUtc: row.endedAtUtc,
    timezoneId: row.timezoneId,
    projectId: row.projectId,
    issueKey: row.issueKey,
    activeSeconds: row.activeSeconds,
    manualSecondsOverride: row.manualSecondsOverride,
    description: row.description,
    source: row.source as WorkSession['source'],
    confidence: row.confidence as WorkSession['confidence'],
    reviewStatus: row.reviewStatus as WorkSession['reviewStatus'],
    isManuallyEdited: row.isManuallyEdited === 1,
    deletedAt: row.deletedAt,
    createdAtUtc: row.createdAtUtc,
    updatedAtUtc: row.updatedAtUtc
  }
}

export class InvalidSessionIntervalError extends Error {
  constructor(startedAtUtc: number, endedAtUtc: number) {
    super(`Session interval invalid: endedAtUtc (${endedAtUtc}) must be > startedAtUtc (${startedAtUtc})`)
  }
}

function assertValidInterval(startedAtUtc: number, endedAtUtc: number | null): void {
  if (endedAtUtc !== null && endedAtUtc <= startedAtUtc) {
    throw new InvalidSessionIntervalError(startedAtUtc, endedAtUtc)
  }
}

/**
 * CRUD + журналируемые правки для `work_sessions` (FR-04). Каждая мутация
 * существующей сессии (`update`/`softDelete`/`restore`/`undoLastEdit`)
 * атомарно пишет и саму строку, и запись в `session_edits` — либо обе
 * операции применяются, либо ни одна (ТЗ, раздел 5: "транзакции для
 * изменения сессий").
 *
 * Разбиение/объединение интервалов (FR-04.6, конфликты перекрытия) —
 * ответственность Session Engine (Итерация 4), здесь не реализовано.
 */
export class WorkSessionsRepository {
  private readonly sessionEdits: SessionEditsRepository

  constructor(private readonly db: Database.Database) {
    this.sessionEdits = new SessionEditsRepository(db)
  }

  create(input: CreateWorkSessionInput): WorkSession {
    assertValidInterval(input.startedAtUtc, input.endedAtUtc)
    const id = generateId()
    const now = nowUtcSeconds()
    const row = {
      id,
      startedAtUtc: input.startedAtUtc,
      endedAtUtc: input.endedAtUtc,
      timezoneId: input.timezoneId,
      projectId: input.projectId ?? null,
      issueKey: input.issueKey ?? null,
      activeSeconds: input.activeSeconds,
      manualSecondsOverride: input.manualSecondsOverride ?? null,
      description: input.description ?? null,
      source: input.source,
      confidence: input.confidence ?? 'high',
      reviewStatus: input.reviewStatus ?? (input.source === 'manual' ? 'reviewed' : 'detected'),
      isManuallyEdited: (input.isManuallyEdited ?? input.source === 'manual') ? 1 : 0,
      deletedAt: null,
      createdAtUtc: now,
      updatedAtUtc: now
    }
    this.db
      .prepare(
        `INSERT INTO work_sessions
           (id, startedAtUtc, endedAtUtc, timezoneId, projectId, issueKey, activeSeconds,
            manualSecondsOverride, description, source, confidence, reviewStatus,
            isManuallyEdited, deletedAt, createdAtUtc, updatedAtUtc)
         VALUES
           (@id, @startedAtUtc, @endedAtUtc, @timezoneId, @projectId, @issueKey, @activeSeconds,
            @manualSecondsOverride, @description, @source, @confidence, @reviewStatus,
            @isManuallyEdited, @deletedAt, @createdAtUtc, @updatedAtUtc)`
      )
      .run(row)
    return this.getById(id) as WorkSession
  }

  getById(id: string): WorkSession | null {
    const row = this.db.prepare('SELECT * FROM work_sessions WHERE id = ?').get(id) as WorkSessionRow | undefined
    return row ? rowToSession(row) : null
  }

  /** По умолчанию без мягко удалённых — передайте `includeDeleted: true`, чтобы увидеть и их. */
  listByRange(startUtc: number, endUtcExclusive: number, options: { includeDeleted?: boolean } = {}): WorkSession[] {
    const sql = options.includeDeleted
      ? 'SELECT * FROM work_sessions WHERE startedAtUtc >= ? AND startedAtUtc < ? ORDER BY startedAtUtc'
      : 'SELECT * FROM work_sessions WHERE startedAtUtc >= ? AND startedAtUtc < ? AND deletedAt IS NULL ORDER BY startedAtUtc'
    const rows = this.db.prepare(sql).all(startUtc, endUtcExclusive) as WorkSessionRow[]
    return rows.map(rowToSession)
  }

  /**
   * Применяет патч к сессии и атомарно пишет snapshot до/после в
   * `session_edits`. Бросает `InvalidSessionIntervalError`, если патч
   * нарушает `end > start`.
   */
  update(id: string, patch: UpdateWorkSessionPatch): WorkSession {
    const applyUpdate = this.db.transaction((): WorkSession => {
      const previous = this.getById(id)
      if (!previous) {
        throw new Error(`WorkSession not found: ${id}`)
      }
      const next: WorkSession = {
        ...previous,
        ...patch,
        isManuallyEdited: true,
        updatedAtUtc: nowUtcSeconds()
      }
      assertValidInterval(next.startedAtUtc, next.endedAtUtc)

      this.db
        .prepare(
          `UPDATE work_sessions SET
             startedAtUtc = @startedAtUtc, endedAtUtc = @endedAtUtc, projectId = @projectId,
             issueKey = @issueKey, activeSeconds = @activeSeconds,
             manualSecondsOverride = @manualSecondsOverride, description = @description,
             confidence = @confidence, reviewStatus = @reviewStatus, isManuallyEdited = 1,
             updatedAtUtc = @updatedAtUtc
           WHERE id = @id`
        )
        .run({
          id,
          startedAtUtc: next.startedAtUtc,
          endedAtUtc: next.endedAtUtc,
          projectId: next.projectId,
          issueKey: next.issueKey,
          activeSeconds: next.activeSeconds,
          manualSecondsOverride: next.manualSecondsOverride,
          description: next.description,
          confidence: next.confidence,
          reviewStatus: next.reviewStatus,
          updatedAtUtc: next.updatedAtUtc
        })

      this.sessionEdits.append({ sessionId: id, operation: 'update', previousState: previous, nextState: next })
      return next
    })
    return applyUpdate()
  }

  softDelete(id: string): WorkSession {
    return this.setDeletedAt(id, nowUtcSeconds(), 'delete')
  }

  restore(id: string): WorkSession {
    return this.setDeletedAt(id, null, 'restore')
  }

  private setDeletedAt(id: string, deletedAt: number | null, operation: 'delete' | 'restore'): WorkSession {
    const apply = this.db.transaction((): WorkSession => {
      const previous = this.getById(id)
      if (!previous) {
        throw new Error(`WorkSession not found: ${id}`)
      }
      const next: WorkSession = { ...previous, deletedAt, updatedAtUtc: nowUtcSeconds() }
      this.db
        .prepare('UPDATE work_sessions SET deletedAt = ?, updatedAtUtc = ? WHERE id = ?')
        .run(next.deletedAt, next.updatedAtUtc, id)
      this.sessionEdits.append({ sessionId: id, operation, previousState: previous, nextState: next })
      return next
    })
    return apply()
  }

  /**
   * Откатывает сессию к состоянию перед последней правкой в журнале.
   * Сама запись последней правки не удаляется (журнал append-only) — вместо
   * этого добавляется новая запись с `operation: 'undo'`, что позволяет
   * повторным undo идти дальше в историю, не теряя ни одной записи.
   */
  undoLastEdit(sessionId: string): WorkSession | null {
    const apply = this.db.transaction((): WorkSession | null => {
      const lastEdit = this.sessionEdits.getLatestForSession(sessionId)
      if (!lastEdit) {
        return null
      }
      const current = this.getById(sessionId)
      if (!current) {
        throw new Error(`WorkSession not found: ${sessionId}`)
      }
      const restored = lastEdit.previousState
      this.db
        .prepare(
          `UPDATE work_sessions SET
             startedAtUtc = @startedAtUtc, endedAtUtc = @endedAtUtc, projectId = @projectId,
             issueKey = @issueKey, activeSeconds = @activeSeconds,
             manualSecondsOverride = @manualSecondsOverride, description = @description,
             confidence = @confidence, reviewStatus = @reviewStatus, isManuallyEdited = @isManuallyEdited,
             deletedAt = @deletedAt, updatedAtUtc = @updatedAtUtc
           WHERE id = @id`
        )
        .run({
          id: sessionId,
          startedAtUtc: restored.startedAtUtc,
          endedAtUtc: restored.endedAtUtc,
          projectId: restored.projectId,
          issueKey: restored.issueKey,
          activeSeconds: restored.activeSeconds,
          manualSecondsOverride: restored.manualSecondsOverride,
          description: restored.description,
          confidence: restored.confidence,
          reviewStatus: restored.reviewStatus,
          isManuallyEdited: restored.isManuallyEdited ? 1 : 0,
          deletedAt: restored.deletedAt,
          updatedAtUtc: nowUtcSeconds()
        })
      const afterUndo = this.getById(sessionId) as WorkSession
      this.sessionEdits.append({ sessionId, operation: 'undo', previousState: current, nextState: afterUndo })
      return afterUndo
    })
    return apply()
  }
}
