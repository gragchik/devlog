/** Зеркалит `src-tauri/src/worklog/templates.rs` и `worklog/reminder.rs`. */

export interface CommentTemplate {
  /** Пустая строка — новый шаблон, id выдаст Rust при сохранении. */
  id: string
  name: string
  text: string
}

export interface TemplateSettings {
  templates: CommentTemplate[]
  defaultTemplateId: string
}

/** Поля, которые понимают шаблоны (`{issueKey}` и т.д.). */
export const TEMPLATE_PLACEHOLDERS: { name: string; hint: string }[] = [
  { name: 'issueKey', hint: 'ключ задачи' },
  { name: 'issueTitle', hint: 'название из Jira (если загружено)' },
  { name: 'activity', hint: 'где шла работа: IDE, терминал…' },
  { name: 'date', hint: 'день, ГГГГ-ММ-ДД' },
  { name: 'duration', hint: 'время, «1ч 30м»' }
]

export interface WorklogReminder {
  localDate: string
  unassignedSeconds: number
  unreportedSeconds: number
  unreportedIssueCount: number
}
