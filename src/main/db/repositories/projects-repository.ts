import type Database from 'better-sqlite3'
import type { CreateProjectInput, Project, UpdateProjectInput } from '@shared/types/project'
import { DEFAULT_ISSUE_REGEX } from '@shared/types/project'
import { generateId, nowUtcSeconds } from '../ids'

/** Сырая строка таблицы `projects` (JSON/boolean-поля ещё не раскодированы). */
interface ProjectRow {
  id: string
  name: string
  repoPath: string
  enabled: number
  issueRegex: string
  preferencesJson: string
  createdAtUtc: number
  updatedAtUtc: number
}

function rowToProject(row: ProjectRow): Project {
  return {
    id: row.id,
    name: row.name,
    repoPath: row.repoPath,
    enabled: row.enabled === 1,
    issueRegex: row.issueRegex,
    preferences: JSON.parse(row.preferencesJson) as Record<string, unknown>,
    createdAtUtc: row.createdAtUtc,
    updatedAtUtc: row.updatedAtUtc
  }
}

export class ProjectsRepository {
  constructor(private readonly db: Database.Database) {}

  create(input: CreateProjectInput): Project {
    const id = generateId()
    const now = nowUtcSeconds()
    this.db
      .prepare(
        `INSERT INTO projects (id, name, repoPath, enabled, issueRegex, preferencesJson, createdAtUtc, updatedAtUtc)
         VALUES (@id, @name, @repoPath, @enabled, @issueRegex, @preferencesJson, @createdAtUtc, @updatedAtUtc)`
      )
      .run({
        id,
        name: input.name,
        repoPath: input.repoPath,
        enabled: (input.enabled ?? true) ? 1 : 0,
        issueRegex: input.issueRegex ?? DEFAULT_ISSUE_REGEX,
        preferencesJson: JSON.stringify(input.preferences ?? {}),
        createdAtUtc: now,
        updatedAtUtc: now
      })
    return this.getById(id) as Project
  }

  getById(id: string): Project | null {
    const row = this.db.prepare('SELECT * FROM projects WHERE id = ?').get(id) as ProjectRow | undefined
    return row ? rowToProject(row) : null
  }

  getByRepoPath(repoPath: string): Project | null {
    const row = this.db.prepare('SELECT * FROM projects WHERE repoPath = ?').get(repoPath) as
      | ProjectRow
      | undefined
    return row ? rowToProject(row) : null
  }

  list(): Project[] {
    const rows = this.db.prepare('SELECT * FROM projects ORDER BY name').all() as ProjectRow[]
    return rows.map(rowToProject)
  }

  update(id: string, patch: UpdateProjectInput): Project {
    const existing = this.getById(id)
    if (!existing) {
      throw new Error(`Project not found: ${id}`)
    }
    const next: Project = {
      ...existing,
      ...patch,
      preferences: patch.preferences ?? existing.preferences,
      updatedAtUtc: nowUtcSeconds()
    }
    this.db
      .prepare(
        `UPDATE projects SET name = @name, enabled = @enabled, issueRegex = @issueRegex,
           preferencesJson = @preferencesJson, updatedAtUtc = @updatedAtUtc
         WHERE id = @id`
      )
      .run({
        id,
        name: next.name,
        enabled: next.enabled ? 1 : 0,
        issueRegex: next.issueRegex,
        preferencesJson: JSON.stringify(next.preferences),
        updatedAtUtc: next.updatedAtUtc
      })
    return next
  }

  remove(id: string): void {
    this.db.prepare('DELETE FROM projects WHERE id = ?').run(id)
  }
}
