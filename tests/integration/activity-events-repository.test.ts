import { afterEach, beforeEach, describe, expect, it } from 'vitest'
import { ActivityEventsRepository } from '../../src/main/db/repositories/activity-events-repository'
import { buildSampleDayFixture } from '../fixtures/sample-day'
import { createTempDatabase, type TempDatabase } from './helpers/temp-database'

const DAY_START = 1_700_000_000

describe('ActivityEventsRepository', () => {
  let temp: TempDatabase
  let repo: ActivityEventsRepository

  beforeEach(() => {
    temp = createTempDatabase()
    repo = new ActivityEventsRepository(temp.db)
  })

  afterEach(() => {
    temp.cleanup()
  })

  it('insert сохраняет событие и возвращает его с id/createdAtUtc', () => {
    const event = repo.insert({
      timestampUtc: DAY_START,
      processNameSanitized: 'webstorm64.exe',
      appCategory: 'ide',
      projectId: null,
      branch: 'main',
      detectedIssueKey: null,
      idleSeconds: 0,
      state: 'TRACKING',
      confidence: 'low',
      reason: 'no issue key in branch'
    })
    expect(event.id).toBeTruthy()
    expect(event.createdAtUtc).toBeGreaterThan(0)
  })

  it('фикстура дня сидируется одной транзакцией и читается обратно по диапазону', () => {
    const fixture = buildSampleDayFixture(DAY_START)
    const inserted = repo.insertMany(fixture.events)
    expect(inserted).toHaveLength(fixture.events.length)

    const all = repo.listByRange(DAY_START, DAY_START + 24 * 3600)
    expect(all).toHaveLength(fixture.events.length)

    // idle-период 10:30–11:00 должен быть представлен как отдельные IDLE-события,
    // не как TRACKING (иначе бездействие молча засчиталось бы работой).
    const idleEvents = all.filter((e) => e.state === 'IDLE')
    expect(idleEvents.length).toBeGreaterThan(0)
    expect(idleEvents.every((e) => e.detectedIssueKey === null)).toBe(true)
  })

  it('listByRange не включает правую границу (полуоткрытый интервал [start, end))', () => {
    repo.insert({
      timestampUtc: DAY_START + 100,
      processNameSanitized: 'x',
      appCategory: 'ide',
      projectId: null,
      branch: null,
      detectedIssueKey: null,
      idleSeconds: null,
      state: 'TRACKING',
      confidence: 'high',
      reason: 'test'
    })
    expect(repo.listByRange(DAY_START, DAY_START + 100)).toHaveLength(0)
    expect(repo.listByRange(DAY_START, DAY_START + 101)).toHaveLength(1)
  })
})
