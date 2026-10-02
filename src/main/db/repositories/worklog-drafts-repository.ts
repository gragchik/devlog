import type Database from 'better-sqlite3'
import type { CreateWorklogDraftInput, UpdateWorklogDraftPatch, WorklogDraft } from '@shared/types/worklog-draft'
import { generateId, nowUtcSeconds } from '../ids'

interface WorklogDraftRow {
  id: string
  localDay: string
  issueKey: string
  timeSpentSeconds: number
  startedAtUtc: number
  comment: string | null
  selectedSessionIdsJson: string
  status: string
  updatedAtUtc: number
}

function rowToDraft(row: WorklogDraftRow): WorklogDraft {
  return {
    id: row.id,
    localDay: row.localDay,
    issueKey: row.issueKey,
    timeSpentSeconds: row.timeSpentSeconds,
    startedAtUtc: row.startedAtUtc,
    comment: row.comment,
    selectedSessionIds: JSON.parse(row.selectedSessionIdsJson) as string[],
    status: row.status as WorklogDraft['status'],
    updatedAtUtc: row.updatedAtUtc
  }
}

/**
 * CRUD-слой для `worklog_drafts` (FR-08, раздел 5). UI составления отчёта
 * (Worklog Review) — Итерация 5/8, здесь только хранение.
 */
export class WorklogDraftsRepository {
  constructor(private readonly db: Database.Database) {}

  create(input: CreateWorklogDraftInput): WorklogDraft {
    const id = generateId()
    const updatedAtUtc = nowUtcSeconds()
    this.db
      .prepare(
        `INSERT INTO worklog_drafts
           (id, localDay, issueKey, timeSpentSeconds, startedAtUtc, comment, selectedSessionIdsJson, status, updatedAtUtc)
         VALUES
           (@id, @localDay, @issueKey, @timeSpentSeconds, @startedAtUtc, @comment, @selectedSessionIdsJson, @status, @updatedAtUtc)`
      )
      .run({
        id,
        localDay: input.localDay,
        issueKey: input.issueKey,
        timeSpentSeconds: input.timeSpentSeconds,
        startedAtUtc: input.startedAtUtc,
        comment: input.comment ?? null,
        selectedSessionIdsJson: JSON.stringify(input.selectedSessionIds),
        status: input.status ?? 'draft',
        updatedAtUtc
      })
    return this.getById(id) as WorklogDraft
  }

  getById(id: string): WorklogDraft | null {
    const row = this.db.prepare('SELECT * FROM worklog_drafts WHERE id = ?').get(id) as
      | WorklogDraftRow
      | undefined
    return row ? rowToDraft(row) : null
  }

  listByLocalDay(localDay: string): WorklogDraft[] {
    const rows = this.db
      .prepare('SELECT * FROM worklog_drafts WHERE localDay = ? ORDER BY startedAtUtc')
      .all(localDay) as WorklogDraftRow[]
    return rows.map(rowToDraft)
  }

  update(id: string, patch: UpdateWorklogDraftPatch): WorklogDraft {
    const existing = this.getById(id)
    if (!existing) {
      throw new Error(`WorklogDraft not found: ${id}`)
    }
    const next: WorklogDraft = { ...existing, ...patch, updatedAtUtc: nowUtcSeconds() }
    this.db
      .prepare(
        `UPDATE worklog_drafts SET
           issueKey = @issueKey, timeSpentSeconds = @timeSpentSeconds, startedAtUtc = @startedAtUtc,
           comment = @comment, selectedSessionIdsJson = @selectedSessionIdsJson, status = @status,
           updatedAtUtc = @updatedAtUtc
         WHERE id = @id`
      )
      .run({
        id,
        issueKey: next.issueKey,
        timeSpentSeconds: next.timeSpentSeconds,
        startedAtUtc: next.startedAtUtc,
        comment: next.comment,
        selectedSessionIdsJson: JSON.stringify(next.selectedSessionIds),
        status: next.status,
        updatedAtUtc: next.updatedAtUtc
      })
    return next
  }

  remove(id: string): void {
    this.db.prepare('DELETE FROM worklog_drafts WHERE id = ?').run(id)
  }
}
