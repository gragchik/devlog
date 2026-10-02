/** Зеркалит `src-tauri/src/domain/work_session.rs`. См. app-info.ts о причинах ручной синхронизации. */
export type WorkSessionSource = 'detected' | 'manual'
export type WorkSessionReviewStatus = 'detected' | 'unassigned' | 'reviewed' | 'excluded'
export type Confidence = 'high' | 'medium' | 'low'

export interface WorkSession {
  id: string
  startedAtUtc: number
  endedAtUtc: number | null
  timezoneId: string
  projectId: string | null
  issueKey: string | null
  activeSeconds: number
  manualSecondsOverride: number | null
  description: string | null
  source: WorkSessionSource
  confidence: Confidence
  reviewStatus: WorkSessionReviewStatus
  isManuallyEdited: boolean
  deletedAt: number | null
  createdAtUtc: number
  updatedAtUtc: number
}

export interface DaySessionsView {
  localDate: string
  sessions: WorkSession[]
  totalActiveSeconds: number
  unassignedSeconds: number
  excludedSeconds: number
}

/** Патч для `update_session` — double-option семантика на Rust-стороне:
 * поле отсутствует в объекте → не трогать; поле равно `null` → очистить;
 * поле со значением → установить. В TS это просто означает "не добавляйте
 * ключ в объект, если не хотите его менять" (а не `undefined`-значение —
 * `JSON.stringify` всё равно отбрасывает `undefined`-поля, что совпадает
 * с нужной семантикой "поле отсутствует").
 */
export interface UpdateWorkSessionPatch {
  startedAtUtc?: number
  endedAtUtc?: number | null
  projectId?: string | null
  issueKey?: string | null
  activeSeconds?: number
  manualSecondsOverride?: number | null
  description?: string | null
  confidence?: Confidence
  reviewStatus?: WorkSessionReviewStatus
}

export interface CreateWorkSessionInput {
  startedAtUtc: number
  endedAtUtc: number | null
  timezoneId: string
  activeSeconds: number
  source: WorkSessionSource
  projectId?: string | null
  issueKey?: string | null
  manualSecondsOverride?: number | null
  description?: string | null
  confidence?: Confidence
  reviewStatus?: WorkSessionReviewStatus
  isManuallyEdited?: boolean
}
