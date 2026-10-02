import { invoke } from '@tauri-apps/api/core'
import type { CreateProjectInput, Project, UpdateProjectInput } from './types/project'
import type { TrackerThresholds, WhitelistEntry } from './types/settings'
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

  showMainWindow: () => invoke<void>('show_main_window_command')
}

/** Приводит ошибку из `invoke()` (обычно строка от Tauri) к читаемому тексту. */
export function errorMessage(err: unknown): string {
  return typeof err === 'string' ? err : err instanceof Error ? err.message : String(err)
}
