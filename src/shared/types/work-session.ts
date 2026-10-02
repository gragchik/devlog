import type { Confidence } from './activity-event'

/** Как сессия была создана (FR-04.3 "источник"). */
export type WorkSessionSource = 'detected' | 'manual'

/** Статус разбора задачи для сессии (FR-04.7). */
export type WorkSessionReviewStatus = 'detected' | 'unassigned' | 'reviewed' | 'excluded'

/**
 * Тип операции в журнале правок (`session_edits.operation`). Полный набор
 * соответствует операциям из FR-04/FR-06; на Итерации 1 репозитории
 * реально производят только `create`/`update`/`delete`/`restore`/`undo` —
 * `split`/`merge`/`exclude` появятся вместе с Session Engine/UI
 * (Итерации 4–6), тип объявлен полностью заранее, чтобы не мигрировать
 * схему `CHECK`/union позже.
 */
export type SessionEditOperation =
  | 'create'
  | 'update'
  | 'split'
  | 'merge'
  | 'exclude'
  | 'restore'
  | 'delete'
  | 'undo'

/**
 * Редактируемая рабочая сессия (FR-04, раздел 5 "work_sessions"). В отличие
 * от `ActivityEvent` — то, что показывается и правится в UI/overlay.
 * `endedAtUtc === null` означает текущую незакрытую сессию.
 */
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
  /** FR-04.3: признак ручной корректировки — отличает от "как было обнаружено". */
  isManuallyEdited: boolean
  deletedAt: number | null
  createdAtUtc: number
  updatedAtUtc: number
}

export type CreateWorkSessionInput = Pick<
  WorkSession,
  'startedAtUtc' | 'endedAtUtc' | 'timezoneId' | 'activeSeconds' | 'source'
> &
  Partial<
    Pick<
      WorkSession,
      | 'projectId'
      | 'issueKey'
      | 'manualSecondsOverride'
      | 'description'
      | 'confidence'
      | 'reviewStatus'
      | 'isManuallyEdited'
    >
  >

export type UpdateWorkSessionPatch = Partial<
  Pick<
    WorkSession,
    | 'startedAtUtc'
    | 'endedAtUtc'
    | 'projectId'
    | 'issueKey'
    | 'activeSeconds'
    | 'manualSecondsOverride'
    | 'description'
    | 'confidence'
    | 'reviewStatus'
  >
>
