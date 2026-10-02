/**
 * Статус черновика worklog. Факт успешной/неуспешной отправки в Jira и
 * reconciliation после таймаута живут отдельно в `jira_submissions`
 * (Итерация 7) — здесь только "это ещё черновик" / "уже отправлялся".
 */
export type WorklogDraftStatus = 'draft' | 'submitted'

/**
 * Черновик worklog-записи за локальный день (FR-08, раздел 5
 * "worklog_drafts"). Формируется из подтверждённых `work_sessions`,
 * редактируется пользователем до отправки в Jira.
 */
export interface WorklogDraft {
  id: string
  /** `YYYY-MM-DD` в локальной таймзоне пользователя. */
  localDay: string
  issueKey: string
  timeSpentSeconds: number
  startedAtUtc: number
  comment: string | null
  /** Сессии, из которых составлен черновик — для пересчёта при их правке. */
  selectedSessionIds: string[]
  status: WorklogDraftStatus
  updatedAtUtc: number
}

export type CreateWorklogDraftInput = Omit<WorklogDraft, 'id' | 'updatedAtUtc' | 'status'> &
  Partial<Pick<WorklogDraft, 'status'>>

export type UpdateWorklogDraftPatch = Partial<
  Pick<WorklogDraft, 'issueKey' | 'timeSpentSeconds' | 'startedAtUtc' | 'comment' | 'selectedSessionIds' | 'status'>
>
