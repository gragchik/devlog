import { afterEach, beforeEach, describe, expect, it } from 'vitest'
import { WorklogDraftsRepository } from '../../src/main/db/repositories/worklog-drafts-repository'
import { createTempDatabase, type TempDatabase } from './helpers/temp-database'

describe('WorklogDraftsRepository', () => {
  let temp: TempDatabase
  let repo: WorklogDraftsRepository

  beforeEach(() => {
    temp = createTempDatabase()
    repo = new WorklogDraftsRepository(temp.db)
  })

  afterEach(() => {
    temp.cleanup()
  })

  it('создаёт черновик со статусом draft по умолчанию', () => {
    const draft = repo.create({
      localDay: '2026-10-01',
      issueKey: 'OB-448',
      timeSpentSeconds: 5400,
      startedAtUtc: 1_700_000_000,
      comment: null,
      selectedSessionIds: ['s1']
    })
    expect(draft.status).toBe('draft')
  })

  it('listByLocalDay фильтрует по дню', () => {
    repo.create({
      localDay: '2026-10-01',
      issueKey: 'OB-448',
      timeSpentSeconds: 100,
      startedAtUtc: 1,
      comment: null,
      selectedSessionIds: []
    })
    repo.create({
      localDay: '2026-10-02',
      issueKey: 'OB-419',
      timeSpentSeconds: 100,
      startedAtUtc: 2,
      comment: null,
      selectedSessionIds: []
    })
    expect(repo.listByLocalDay('2026-10-01')).toHaveLength(1)
    expect(repo.listByLocalDay('2026-10-01')[0]?.issueKey).toBe('OB-448')
  })

  it('update меняет комментарий и список сессий, сохраняя остальное', () => {
    const draft = repo.create({
      localDay: '2026-10-01',
      issueKey: 'OB-448',
      timeSpentSeconds: 100,
      startedAtUtc: 1,
      comment: null,
      selectedSessionIds: ['s1']
    })
    const updated = repo.update(draft.id, { comment: 'готово', selectedSessionIds: ['s1', 's2'] })
    expect(updated.comment).toBe('готово')
    expect(updated.selectedSessionIds).toEqual(['s1', 's2'])
    expect(updated.issueKey).toBe('OB-448')
  })

  it('remove удаляет черновик', () => {
    const draft = repo.create({
      localDay: '2026-10-01',
      issueKey: 'OB-448',
      timeSpentSeconds: 100,
      startedAtUtc: 1,
      comment: null,
      selectedSessionIds: []
    })
    repo.remove(draft.id)
    expect(repo.getById(draft.id)).toBeNull()
  })
})
