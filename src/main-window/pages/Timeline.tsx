import { useState } from 'react'
import type { WorkSession } from '@shared/types/work-session'
import { formatDuration } from '@shared/utils/format-duration'
import { SessionRow } from '../components/SessionRow'
import { api, errorMessage } from '@shared/api'
import { useDaySessions } from '@shared/hooks/useDaySessions'

type DateChoice = 'today' | 'yesterday' | string

function toDateInputValue(choice: DateChoice, resolvedDate: string | undefined): string {
  if (choice === 'today' || choice === 'yesterday') return resolvedDate ?? ''
  return choice
}

type StatusFilter = 'all' | 'unassigned' | 'assigned' | 'excluded'

const STATUS_FILTERS: { id: StatusFilter; label: string }[] = [
  { id: 'all', label: 'Все' },
  { id: 'unassigned', label: 'Без задачи' },
  { id: 'assigned', label: 'С задачей' },
  { id: 'excluded', label: 'Исключённые' }
]

function matchesFilter(session: WorkSession, status: StatusFilter, query: string): boolean {
  const excluded = session.reviewStatus === 'excluded' || session.deletedAt !== null
  const statusOk =
    status === 'all' ||
    (status === 'excluded' && excluded) ||
    (status === 'unassigned' && !excluded && session.issueKey === null) ||
    (status === 'assigned' && !excluded && session.issueKey !== null)
  const q = query.trim().toUpperCase()
  const text = `${session.issueKey ?? ''} ${session.description ?? ''}`.toUpperCase()
  return statusOk && (q === '' || text.includes(q))
}

export function Timeline({ initialDate }: { initialDate?: string }): JSX.Element {
  const [dateChoice, setDateChoice] = useState<DateChoice>(initialDate ?? 'today')
  const { view, loading, error, reload } = useDaySessions(dateChoice)
  const [manualError, setManualError] = useState<string | null>(null)
  const [statusFilter, setStatusFilter] = useState<StatusFilter>('all')
  const [query, setQuery] = useState('')

  async function createManualSession(): Promise<void> {
    const description = window.prompt('Описание (например, «Созвон по архитектуре»):')
    if (description === null) return
    const now = Math.floor(Date.now() / 1000)
    try {
      await api.createManualSession({
        startedAtUtc: now - 30 * 60,
        endedAtUtc: now,
        timezoneId: Intl.DateTimeFormat().resolvedOptions().timeZone,
        activeSeconds: 30 * 60,
        source: 'manual',
        description,
        reviewStatus: 'reviewed'
      })
      reload()
    } catch (err) {
      setManualError(errorMessage(err))
    }
  }

  const allSessions = view?.sessions ?? []
  const visibleSessions = allSessions.filter((s) => matchesFilter(s, statusFilter, query))

  return (
    <div className="page">
      <h1>Timeline</h1>

      <div className="timeline-toolbar">
        <button type="button" className={dateChoice === 'yesterday' ? 'active' : ''} onClick={() => setDateChoice('yesterday')}>
          Вчера
        </button>
        <button type="button" className={dateChoice === 'today' ? 'active' : ''} onClick={() => setDateChoice('today')}>
          Сегодня
        </button>
        <input
          type="date"
          value={toDateInputValue(dateChoice, view?.localDate)}
          onChange={(e) => e.target.value && setDateChoice(e.target.value)}
        />
        <button type="button" onClick={createManualSession}>
          + Ручная сессия
        </button>
      </div>
      <div className="timeline-toolbar timeline-filters">
        {STATUS_FILTERS.map((f) => (
          <button key={f.id} type="button" className={statusFilter === f.id ? 'active' : ''} onClick={() => setStatusFilter(f.id)}>
            {f.label}
          </button>
        ))}
        <input placeholder="Задача или описание" value={query} onChange={(e) => setQuery(e.target.value)} />
      </div>
      {manualError && <p className="error">{manualError}</p>}

      {loading && <p>Загрузка…</p>}
      {error && <p className="error">{error}</p>}

      {view && (
        <>
          <p className="muted">
            {view.localDate} — итого {formatDuration(view.totalActiveSeconds)}, не распределено{' '}
            {formatDuration(view.unassignedSeconds)}
          </p>
          <div className="session-list">
            {visibleSessions.length === 0 && (
              <p className="muted">{allSessions.length === 0 ? 'Нет сессий за этот день.' : 'Нет сессий под фильтр.'}</p>
            )}
            {visibleSessions.map((session) => (
              <SessionRow
                key={session.id}
                session={session}
                // Соседняя сессия — по полному списку, а не отфильтрованному:
                // «объединить» должно работать только с реальным соседом.
                nextSession={allSessions[allSessions.indexOf(session) + 1] ?? null}
                onChanged={reload}
              />
            ))}
          </div>
        </>
      )}
    </div>
  )
}
