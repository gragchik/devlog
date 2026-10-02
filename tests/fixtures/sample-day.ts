import type { CreateActivityEventInput } from '@shared/types/activity-event'
import type { CreateWorkSessionInput } from '@shared/types/work-session'

const HOUR = 3600
const POLL_STEP_SECONDS = 300 // 5 минут — целевой интервал опроса (FR-02.1)

export interface SampleDayFixture {
  /** Эпоха-секунды начала суток (UTC), от которого считаются все смещения. */
  startOfDayUtc: number
  timezoneId: string
  events: CreateActivityEventInput[]
  sessions: CreateWorkSessionInput[]
}

interface RangeSpec {
  fromOffsetSeconds: number
  toOffsetSeconds: number
  state: CreateActivityEventInput['state']
  issueKey: string | null
  branch: string | null
}

function buildEventsForRange(startOfDayUtc: number, spec: RangeSpec): CreateActivityEventInput[] {
  const events: CreateActivityEventInput[] = []
  for (let offset = spec.fromOffsetSeconds; offset < spec.toOffsetSeconds; offset += POLL_STEP_SECONDS) {
    events.push({
      timestampUtc: startOfDayUtc + offset,
      processNameSanitized: spec.state === 'IDLE' ? 'webstorm64.exe' : 'webstorm64.exe',
      appCategory: 'ide',
      projectId: null,
      branch: spec.branch,
      detectedIssueKey: spec.issueKey,
      idleSeconds: spec.state === 'IDLE' ? POLL_STEP_SECONDS : 0,
      state: spec.state,
      confidence: spec.issueKey ? 'high' : 'low',
      reason: spec.issueKey ? `branch regex match: ${spec.issueKey}` : 'idle timeout exceeded'
    })
  }
  return events
}

/**
 * Сценарий 1 из ТЗ (раздел 9, E2E): "Работа 09:00–10:30 OB-448, 10:30–11:00
 * idle, 11:00–12:00 OB-419 → 1:30 + 1:00, idle не входит". Используется и
 * здесь (Итерация 1, проверка репозиториев), и будет переиспользован
 * Session Engine тестами в Итерации 4.
 */
export function buildSampleDayFixture(startOfDayUtc: number, timezoneId = 'UTC'): SampleDayFixture {
  const ranges: RangeSpec[] = [
    { fromOffsetSeconds: 9 * HOUR, toOffsetSeconds: 10.5 * HOUR, state: 'TRACKING', issueKey: 'OB-448', branch: 'feature/OB-448-mass-payments' },
    { fromOffsetSeconds: 10.5 * HOUR, toOffsetSeconds: 11 * HOUR, state: 'IDLE', issueKey: null, branch: null },
    { fromOffsetSeconds: 11 * HOUR, toOffsetSeconds: 12 * HOUR, state: 'TRACKING', issueKey: 'OB-419', branch: 'feature/OB-419-refunds' }
  ]

  const events = ranges.flatMap((range) => buildEventsForRange(startOfDayUtc, range))

  const sessions: CreateWorkSessionInput[] = [
    {
      startedAtUtc: startOfDayUtc + 9 * HOUR,
      endedAtUtc: startOfDayUtc + 10.5 * HOUR,
      timezoneId,
      issueKey: 'OB-448',
      activeSeconds: 1.5 * HOUR,
      source: 'detected',
      confidence: 'high',
      reviewStatus: 'detected'
    },
    {
      startedAtUtc: startOfDayUtc + 11 * HOUR,
      endedAtUtc: startOfDayUtc + 12 * HOUR,
      timezoneId,
      issueKey: 'OB-419',
      activeSeconds: 1 * HOUR,
      source: 'detected',
      confidence: 'high',
      reviewStatus: 'detected'
    }
  ]

  return { startOfDayUtc, timezoneId, events, sessions }
}
