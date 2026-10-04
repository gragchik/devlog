/** Зеркалит `src-tauri/src/domain/jira.rs`, `domain/worklog_draft.rs`,
 * `worklog/submission.rs` и `commands/jira.rs`. См. app-info.ts о причинах
 * ручной синхронизации. API token сюда не входит никогда — frontend его
 * только отправляет при сохранении, но не получает обратно. */

export interface JiraConnectionStatus {
  configured: boolean
  baseUrl: string | null
  email: string | null
}

export interface JiraConnectionInput {
  baseUrl: string
  email: string
  /** Пустая строка — оставить уже сохранённый токен. */
  apiToken: string
}

export interface JiraUser {
  accountId: string
  displayName: string
}

export interface JiraIssueSummary {
  issueKey: string
  title: string
}

export type JiraSubmissionState = 'pending' | 'posted' | 'unknown' | 'failed'

export interface JiraSubmission {
  id: string
  localDraftId: string
  issueKey: string
  payloadHash: string
  remoteWorklogId: string | null
  state: JiraSubmissionState
  attemptedAtUtc: number
  resolvedAtUtc: number | null
}

export type WorklogDraftStatus = 'draft' | 'submitted'

export interface WorklogDraftView {
  id: string
  localDay: string
  issueKey: string
  timeSpentSeconds: number
  startedAtUtc: number
  comment: string | null
  selectedSessionIds: string[]
  status: WorklogDraftStatus
  updatedAtUtc: number
  issueTitle: string | null
  lastSubmission: JiraSubmission | null
}

export interface WorklogDayView {
  localDate: string
  drafts: WorklogDraftView[]
}

/** Поле отсутствует — не менять. */
export interface WorklogDraftEdit {
  issueKey?: string
  timeSpentSeconds?: number
  startedAtUtc?: number
  comment?: string
}

export type SubmitOutcome =
  | { kind: 'posted'; remoteWorklogId: string | null }
  | { kind: 'failed'; message: string }
  | { kind: 'unknown'; message: string }
  | { kind: 'blocked'; message: string }

export interface DraftSubmitResult {
  draftId: string
  outcome: SubmitOutcome
}

export interface UnknownSubmissionView extends JiraSubmission {
  /** День черновика (для перехода к нему), если черновик ещё существует. */
  localDay: string | null
}
