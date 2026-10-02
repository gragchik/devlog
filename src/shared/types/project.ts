/**
 * Локальный репозиторий, явно добавленный пользователем (FR-03.1).
 * `issueRegex` — настраиваемый паттерн извлечения issue key из имени ветки
 * (FR-03.3), по умолчанию `DEFAULT_ISSUE_REGEX`.
 */
export interface Project {
  id: string
  name: string
  repoPath: string
  enabled: boolean
  issueRegex: string
  /** Произвольные пользовательские настройки проекта (project → task правила и т.п., FR-03.4). */
  preferences: Record<string, unknown>
  createdAtUtc: number
  updatedAtUtc: number
}

export const DEFAULT_ISSUE_REGEX = '\\b[A-Z][A-Z0-9]+-\\d+\\b'

export type CreateProjectInput = Pick<Project, 'name' | 'repoPath'> &
  Partial<Pick<Project, 'enabled' | 'issueRegex' | 'preferences'>>

export type UpdateProjectInput = Partial<
  Pick<Project, 'name' | 'enabled' | 'issueRegex' | 'preferences'>
>
