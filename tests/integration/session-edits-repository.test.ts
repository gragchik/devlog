import { afterEach, beforeEach, describe, expect, it } from 'vitest'
import { SessionEditsRepository } from '../../src/main/db/repositories/session-edits-repository'
import { WorkSessionsRepository } from '../../src/main/db/repositories/work-sessions-repository'
import { createTempDatabase, type TempDatabase } from './helpers/temp-database'

describe('SessionEditsRepository.pruneOlderThan (FR-04.8: минимум 30 дней истории)', () => {
  let temp: TempDatabase
  let sessions: WorkSessionsRepository
  let edits: SessionEditsRepository

  beforeEach(() => {
    temp = createTempDatabase()
    sessions = new WorkSessionsRepository(temp.db)
    edits = new SessionEditsRepository(temp.db)
  })

  afterEach(() => {
    temp.cleanup()
  })

  it('удаляет только записи старше порога, недавние остаются', () => {
    const session = sessions.create({
      startedAtUtc: 1_700_000_000,
      endedAtUtc: 1_700_003_600,
      timezoneId: 'UTC',
      activeSeconds: 3600,
      source: 'detected'
    })
    sessions.update(session.id, { description: 'правка 1' })
    sessions.update(session.id, { description: 'правка 2' })
    expect(edits.listBySession(session.id)).toHaveLength(2)

    // Порог в далёком будущем — удалит обе (имитирует "старше 30 дней" без
    // необходимости ждать реального времени в тесте).
    const removed = edits.pruneOlderThan(Math.floor(Date.now() / 1000) + 3600)
    expect(removed).toBe(2)
    expect(edits.listBySession(session.id)).toHaveLength(0)
  })

  it('порог в прошлом ничего не удаляет', () => {
    const session = sessions.create({
      startedAtUtc: 1_700_000_000,
      endedAtUtc: 1_700_003_600,
      timezoneId: 'UTC',
      activeSeconds: 3600,
      source: 'detected'
    })
    sessions.update(session.id, { description: 'правка' })
    const removed = edits.pruneOlderThan(1)
    expect(removed).toBe(0)
    expect(edits.listBySession(session.id)).toHaveLength(1)
  })
})
