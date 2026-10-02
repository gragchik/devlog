import { useState } from 'react'
import { formatDuration } from '@shared/utils/format-duration'
import { SessionRow } from '../components/SessionRow'
import { api, errorMessage } from '@shared/api'
import { useDaySessions } from '@shared/hooks/useDaySessions'

type DateChoice = 'today' | 'yesterday' | string

function toDateInputValue(choice: DateChoice, resolvedDate: string | undefined): string {
  if (choice === 'today' || choice === 'yesterday') return resolvedDate ?? ''
  return choice
}

export function Timeline(): JSX.Element {
  const [dateChoice, setDateChoice] = useState<DateChoice>('today')
  const { view, loading, error, reload } = useDaySessions(dateChoice)
  const [manualError, setManualError] = useState<string | null>(null)

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

  const visibleSessions = view?.sessions ?? []

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
            {visibleSessions.length === 0 && <p className="muted">Нет сессий за этот день.</p>}
            {visibleSessions.map((session, i) => (
              <SessionRow key={session.id} session={session} nextSession={visibleSessions[i + 1] ?? null} onChanged={reload} />
            ))}
          </div>
        </>
      )}
    </div>
  )
}
