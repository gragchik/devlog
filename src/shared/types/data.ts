/** Зеркалит `src-tauri/src/commands/data.rs` и `db/app_database.rs::StartupReport`. */

export interface StartupReport {
  dbPath: string
  preMigrationBackup: string | null
  integrityError: string | null
  corruptCopy: string | null
  interruptedSubmissions: number
}

export interface ConsistencyIssue {
  kind: string
  message: string
}

export interface DiagnosticsView {
  appVersion: string
  dbPath: string
  dbSizeBytes: number
  schemaVersion: number
  latestSchemaVersion: number
  logPath: string | null
  startup: StartupReport
  integrityError: string | null
  issues: ConsistencyIssue[]
  /** Уже прошли redaction на стороне Rust. */
  logTail: string[]
}

export type ExportKind = 'json' | 'csv' | 'backup'

export type DeleteRequest = { kind: 'before'; localDate: string } | { kind: 'allHistory' } | { kind: 'everything' }

export interface DeleteSummary {
  activityEvents: number
  workSessions: number
  worklogDrafts: number
}

export const DELETE_CONFIRMATION = 'УДАЛИТЬ'
