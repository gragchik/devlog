import { afterEach, beforeEach, describe, expect, it } from 'vitest'
import { SessionEditsRepository } from '../../src/main/db/repositories/session-edits-repository'
import {
  InvalidSessionIntervalError,
  WorkSessionsRepository
} from '../../src/main/db/repositories/work-sessions-repository'
import { createTempDatabase, type TempDatabase } from './helpers/temp-database'

const DAY_START = 1_700_000_000 // произвольная фиксированная эпоха для детерминизма тестов

describe('WorkSessionsRepository', () => {
  let temp: TempDatabase
  let repo: WorkSessionsRepository
  let edits: SessionEditsRepository

  beforeEach(() => {
    temp = createTempDatabase()
    repo = new WorkSessionsRepository(temp.db)
    edits = new SessionEditsRepository(temp.db)
  })

  afterEach(() => {
    temp.cleanup()
  })

  it('создаёт сессию и читает её обратно без изменений', () => {
    const session = repo.create({
      startedAtUtc: DAY_START,
      endedAtUtc: DAY_START + 3600,
      timezoneId: 'UTC',
      issueKey: 'OB-448',
      activeSeconds: 3600,
      source: 'detected'
    })

    expect(repo.getById(session.id)).toEqual(session)
    expect(session.reviewStatus).toBe('detected')
    expect(session.isManuallyEdited).toBe(false)
  })

  it('ручная сессия (FR-04.5) по умолчанию reviewStatus=reviewed и isManuallyEdited=true', () => {
    const session = repo.create({
      startedAtUtc: DAY_START,
      endedAtUtc: DAY_START + 1800,
      timezoneId: 'UTC',
      activeSeconds: 1800,
      source: 'manual',
      description: 'Созвон по архитектуре'
    })
    expect(session.reviewStatus).toBe('reviewed')
    expect(session.isManuallyEdited).toBe(true)
  })

  it('отвергает endedAtUtc <= startedAtUtc при создании', () => {
    expect(() =>
      repo.create({
        startedAtUtc: DAY_START,
        endedAtUtc: DAY_START,
        timezoneId: 'UTC',
        activeSeconds: 0,
        source: 'manual'
      })
    ).toThrow(InvalidSessionIntervalError)
  })

  it('listByRange возвращает только сессии внутри диапазона и по умолчанию без удалённых', () => {
    const inRange = repo.create({
      startedAtUtc: DAY_START + 100,
      endedAtUtc: DAY_START + 200,
      timezoneId: 'UTC',
      activeSeconds: 100,
      source: 'detected'
    })
    repo.create({
      startedAtUtc: DAY_START + 10_000,
      endedAtUtc: DAY_START + 10_100,
      timezoneId: 'UTC',
      activeSeconds: 100,
      source: 'detected'
    })
    const deleted = repo.create({
      startedAtUtc: DAY_START + 300,
      endedAtUtc: DAY_START + 400,
      timezoneId: 'UTC',
      activeSeconds: 100,
      source: 'detected'
    })
    repo.softDelete(deleted.id)

    const result = repo.listByRange(DAY_START, DAY_START + 1000)
    expect(result.map((s) => s.id)).toEqual([inRange.id])

    const withDeleted = repo.listByRange(DAY_START, DAY_START + 1000, { includeDeleted: true })
    expect(withDeleted.map((s) => s.id).sort()).toEqual([deleted.id, inRange.id].sort())
  })

  it('update атомарно меняет сессию и пишет snapshot до/после в session_edits', () => {
    const session = repo.create({
      startedAtUtc: DAY_START,
      endedAtUtc: DAY_START + 3600,
      timezoneId: 'UTC',
      issueKey: 'OB-448',
      activeSeconds: 3600,
      source: 'detected'
    })

    const updated = repo.update(session.id, { issueKey: 'OB-419', description: 'переназначено' })

    expect(updated.issueKey).toBe('OB-419')
    expect(updated.isManuallyEdited).toBe(true)

    const log = edits.listBySession(session.id)
    expect(log).toHaveLength(1)
    expect(log[0]?.operation).toBe('update')
    expect(log[0]?.previousState.issueKey).toBe('OB-448')
    expect(log[0]?.nextState.issueKey).toBe('OB-419')
  })

  it('update отвергает патч, который нарушает end > start, и не пишет в журнал', () => {
    const session = repo.create({
      startedAtUtc: DAY_START,
      endedAtUtc: DAY_START + 3600,
      timezoneId: 'UTC',
      activeSeconds: 3600,
      source: 'detected'
    })

    expect(() => repo.update(session.id, { endedAtUtc: DAY_START - 1 })).toThrow(InvalidSessionIntervalError)
    expect(edits.listBySession(session.id)).toHaveLength(0)
    expect(repo.getById(session.id)).toEqual(session) // не изменилась
  })

  it('softDelete/restore переключают deletedAt и журналируются', () => {
    const session = repo.create({
      startedAtUtc: DAY_START,
      endedAtUtc: DAY_START + 3600,
      timezoneId: 'UTC',
      activeSeconds: 3600,
      source: 'detected'
    })

    const deleted = repo.softDelete(session.id)
    expect(deleted.deletedAt).not.toBeNull()

    const restored = repo.restore(session.id)
    expect(restored.deletedAt).toBeNull()

    const ops = edits.listBySession(session.id).map((e) => e.operation)
    expect(ops).toEqual(['delete', 'restore'])
  })

  it('undoLastEdit откатывает последнюю правку, не удаляя её из журнала', () => {
    const session = repo.create({
      startedAtUtc: DAY_START,
      endedAtUtc: DAY_START + 3600,
      timezoneId: 'UTC',
      issueKey: 'OB-448',
      activeSeconds: 3600,
      source: 'detected'
    })
    repo.update(session.id, { issueKey: 'OB-419' })

    const afterUndo = repo.undoLastEdit(session.id)

    expect(afterUndo?.issueKey).toBe('OB-448')
    expect(repo.getById(session.id)?.issueKey).toBe('OB-448')

    const ops = edits.listBySession(session.id).map((e) => e.operation)
    expect(ops).toEqual(['update', 'undo']) // исходная правка осталась в журнале
  })

  it('undoLastEdit возвращает null, если у сессии нет правок', () => {
    const session = repo.create({
      startedAtUtc: DAY_START,
      endedAtUtc: DAY_START + 3600,
      timezoneId: 'UTC',
      activeSeconds: 3600,
      source: 'detected'
    })
    expect(repo.undoLastEdit(session.id)).toBeNull()
  })

  it('undo трогает только свою сессию, а не все строки таблицы (регрессия на забытый WHERE)', () => {
    const a = repo.create({
      startedAtUtc: DAY_START,
      endedAtUtc: DAY_START + 100,
      timezoneId: 'UTC',
      issueKey: 'OB-448',
      activeSeconds: 100,
      source: 'detected'
    })
    const b = repo.create({
      startedAtUtc: DAY_START + 1000,
      endedAtUtc: DAY_START + 1100,
      timezoneId: 'UTC',
      issueKey: 'OB-419',
      activeSeconds: 100,
      source: 'detected'
    })
    repo.update(a.id, { issueKey: 'OB-448-renamed' })

    repo.undoLastEdit(a.id)

    expect(repo.getById(a.id)?.issueKey).toBe('OB-448')
    expect(repo.getById(b.id)?.issueKey).toBe('OB-419') // не затронута
  })

  it('две параллельные правки одной сессии не повреждают БД и не теряются молча', async () => {
    const session = repo.create({
      startedAtUtc: DAY_START,
      endedAtUtc: DAY_START + 3600,
      timezoneId: 'UTC',
      activeSeconds: 3600,
      source: 'detected',
      description: 'исходное'
    })

    // better-sqlite3 синхронный, а JS однопоточный — "параллельность" здесь
    // означает два вызова, инициированных без ожидания друг друга
    // (как два почти одновременных IPC-запроса от main-window и overlay),
    // но фактически выполняющихся строго по очереди.
    const [resultA, resultB] = await Promise.all([
      Promise.resolve().then(() => repo.update(session.id, { description: 'правка A' })),
      Promise.resolve().then(() => repo.update(session.id, { description: 'правка B' }))
    ])

    const final = repo.getById(session.id)
    // Детерминированный результат: одна из правок победила целиком (не смесь полей).
    expect(['правка A', 'правка B']).toContain(final?.description)
    expect([resultA.description, resultB.description]).toContain(final?.description)

    // Обе правки зафиксированы в журнале — ни одна не потерялась бесследно.
    const ops = edits.listBySession(session.id)
    expect(ops).toHaveLength(2)
    expect(ops.map((e) => e.nextState.description).sort()).toEqual(['правка A', 'правка B'])
  })
})
