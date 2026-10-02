import { afterEach, beforeEach, describe, expect, it } from 'vitest'
import { DEFAULT_ISSUE_REGEX } from '@shared/types/project'
import { ProjectsRepository } from '../../src/main/db/repositories/projects-repository'
import { createTempDatabase, type TempDatabase } from './helpers/temp-database'

describe('ProjectsRepository', () => {
  let temp: TempDatabase
  let repo: ProjectsRepository

  beforeEach(() => {
    temp = createTempDatabase()
    repo = new ProjectsRepository(temp.db)
  })

  afterEach(() => {
    temp.cleanup()
  })

  it('создаёт проект с дефолтным issueRegex и enabled=true', () => {
    const project = repo.create({ name: 'DevLog', repoPath: 'D:/soft/worklog' })
    expect(project.enabled).toBe(true)
    expect(project.issueRegex).toBe(DEFAULT_ISSUE_REGEX)
    expect(project.preferences).toEqual({})
  })

  it('repoPath уникален — повторное добавление того же пути падает', () => {
    repo.create({ name: 'DevLog', repoPath: 'D:/soft/worklog' })
    expect(() => repo.create({ name: 'DevLog copy', repoPath: 'D:/soft/worklog' })).toThrow()
  })

  it('update меняет только переданные поля', () => {
    const project = repo.create({ name: 'DevLog', repoPath: 'D:/soft/worklog' })
    const updated = repo.update(project.id, { enabled: false })
    expect(updated.enabled).toBe(false)
    expect(updated.name).toBe('DevLog')
    expect(updated.repoPath).toBe('D:/soft/worklog')
  })

  it('list возвращает все проекты по имени', () => {
    repo.create({ name: 'B', repoPath: '/b' })
    repo.create({ name: 'A', repoPath: '/a' })
    expect(repo.list().map((p) => p.name)).toEqual(['A', 'B'])
  })

  it('getByRepoPath находит проект по пути репозитория', () => {
    const project = repo.create({ name: 'DevLog', repoPath: 'D:/soft/worklog' })
    expect(repo.getByRepoPath('D:/soft/worklog')).toEqual(project)
    expect(repo.getByRepoPath('D:/does-not-exist')).toBeNull()
  })
})
