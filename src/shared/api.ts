import { invoke } from '@tauri-apps/api/core'
import type {
  DraftSubmitResult,
  JiraConnectionInput,
  JiraConnectionStatus,
  JiraIssueSummary,
  JiraUser,
  SubmitOutcome,
  UnknownSubmissionView,
  WorklogDayView,
  WorklogDraftEdit
} from './types/jira'
import type { CreateProjectInput, Project, UpdateProjectInput } from './types/project'
import type { TrackerThresholds, WhitelistEntry } from './types/settings'
import type { TemplateSettings, WorklogReminder } from './types/worklog'
import type { CreateWorkSessionInput, DaySessionsView, UpdateWorkSessionPatch, WorkSession } from './types/work-session'

/**
 * Тонкая типизированная обёртка над `invoke()` — один файл со всеми
 * командами, используется и main-window, и overlay (один и тот же сервис
 * на обе стороны, ТЗ раздел 6/FR-06.9). Не раскидываем строковые имена
 * каналов по компонентам — опечатка в названии команды иначе всплывает
 * только в рантайме.
 */
export const api = {
  getSessionsForDay: (localDate: string) => invoke<DaySessionsView>('get_sessions_for_day', { localDate }),
  updateSession: (id: string, patch: UpdateWorkSessionPatch) => invoke<WorkSession>('update_session', { id, patch }),
  splitSession: (id: string, atUtc: number) => invoke<[WorkSession, WorkSession]>('split_session', { id, atUtc }),
  mergeSessions: (firstId: string, secondId: string) => invoke<WorkSession>('merge_sessions', { firstId, secondId }),
  excludeSession: (id: string) => invoke<WorkSession>('exclude_session', { id }),
  restoreSession: (id: string) => invoke<WorkSession>('restore_session', { id }),
  undoSessionEdit: (id: string) => invoke<WorkSession | null>('undo_session_edit', { id }),
  createManualSession: (input: CreateWorkSessionInput) => invoke<WorkSession>('create_manual_session', { input }),

  listProjects: () => invoke<Project[]>('list_projects'),
  addProject: (input: CreateProjectInput) => invoke<Project>('add_project', { input }),
  updateProject: (id: string, patch: UpdateProjectInput) => invoke<Project>('update_project', { id, patch }),
  removeProject: (id: string) => invoke<void>('remove_project', { id }),

  getWhitelist: () => invoke<WhitelistEntry[]>('get_whitelist'),
  setWhitelist: (entries: WhitelistEntry[]) => invoke<void>('set_whitelist', { entries }),
  getTrackerThresholds: () => invoke<TrackerThresholds>('get_tracker_thresholds'),
  setTrackerThresholds: (thresholds: TrackerThresholds) => invoke<void>('set_tracker_thresholds', { thresholds }),
  getAutostartEnabled: () => invoke<boolean>('get_autostart_enabled'),
  setAutostartEnabled: (enabled: boolean) => invoke<void>('set_autostart_enabled', { enabled }),

  getTrackingPaused: () => invoke<boolean>('get_tracking_paused'),
  setTrackingPaused: (paused: boolean) => invoke<boolean>('set_tracking_paused', { paused }),

  getPinnedIssue: () => invoke<string | null>('get_pinned_issue'),
  setPinnedIssue: (issueKey: string | null) => invoke<void>('set_pinned_issue', { issueKey }),

  getOverlayShortcut: () => invoke<string>('get_overlay_shortcut'),
  setOverlayShortcut: (accelerator: string) => invoke<void>('set_overlay_shortcut', { accelerator }),

  showMainWindow: (tab?: 'dashboard' | 'timeline' | 'worklog' | 'settings') =>
    invoke<void>('show_main_window_command', { tab: tab ?? null }),

  // Jira / Worklog Review — Rust разрешает их только главному окну.
  jiraGetConnectionStatus: () => invoke<JiraConnectionStatus>('jira_get_connection_status'),
  jiraSaveConnection: (input: JiraConnectionInput) => invoke<JiraConnectionStatus>('jira_save_connection', { input }),
  jiraClearConnection: () => invoke<void>('jira_clear_connection'),
  jiraTestConnection: () => invoke<JiraUser>('jira_test_connection'),
  jiraFetchIssue: (issueKey: string) => invoke<JiraIssueSummary>('jira_fetch_issue', { issueKey }),
  worklogGetDay: (localDate: string) => invoke<WorklogDayView>('worklog_get_day', { localDate }),
  worklogGenerateDrafts: (localDate: string) => invoke<WorklogDayView>('worklog_generate_drafts', { localDate }),
  worklogUpdateDraft: (id: string, edit: WorklogDraftEdit) => invoke<unknown>('worklog_update_draft', { id, edit }),
  worklogDeleteDraft: (id: string) => invoke<void>('worklog_delete_draft', { id }),
  worklogGetTemplates: () => invoke<TemplateSettings>('worklog_get_templates'),
  worklogSaveTemplates: (value: TemplateSettings) => invoke<TemplateSettings>('worklog_save_templates', { value }),
  worklogApplyTemplate: (draftId: string, templateId: string) =>
    invoke<unknown>('worklog_apply_template', { draftId, templateId }),
  worklogRecalculateDraft: (draftId: string) => invoke<unknown>('worklog_recalculate_draft', { draftId }),
  worklogGetReminder: () => invoke<WorklogReminder | null>('worklog_get_reminder'),
  worklogDismissReminder: (localDate: string) => invoke<void>('worklog_dismiss_reminder', { localDate }),
  jiraSubmitDrafts: (draftIds: string[]) => invoke<DraftSubmitResult[]>('jira_submit_drafts', { draftIds }),
  jiraReconcileSubmission: (submissionId: string) => invoke<SubmitOutcome>('jira_reconcile_submission', { submissionId }),
  jiraListUnknownSubmissions: () => invoke<UnknownSubmissionView[]>('jira_list_unknown_submissions'),
  jiraResolveSubmissionManually: (submissionId: string, posted: boolean) =>
    invoke<SubmitOutcome>('jira_resolve_submission_manually', { submissionId, posted })
}

/** Приводит ошибку из `invoke()` (обычно строка от Tauri) к читаемому тексту. */
export function errorMessage(err: unknown): string {
  return typeof err === 'string' ? err : err instanceof Error ? err.message : String(err)
}
