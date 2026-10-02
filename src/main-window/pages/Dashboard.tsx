import { useEffect, useState } from 'react'
import { formatDuration } from '@shared/utils/format-duration'
import { api, errorMessage } from '../api'
import { useDaySessions } from '../hooks/useDaySessions'

export function Dashboard(): JSX.Element {
  const { view, loading, error } = useDaySessions('today')
  const [paused, setPaused] = useState<boolean | null>(null)
  const [pauseError, setPauseError] = useState<string | null>(null)

  useEffect(() => {
    api.getTrackingPaused().then(setPaused).catch(() => setPaused(null))
    const interval = setInterval(() => {
      api.getTrackingPaused().then(setPaused).catch(() => {})
    }, 5000)
    return () => clearInterval(interval)
  }, [])

  async function togglePause(): Promise<void> {
    if (paused === null) return
    try {
      const next = await api.setTrackingPaused(!paused)
      setPaused(next)
    } catch (err) {
      setPauseError(errorMessage(err))
    }
  }

  const current = view?.sessions.at(-1) ?? null
  const isCurrentLive =
    current !== null && current.endedAtUtc !== null && Date.now() / 1000 - current.endedAtUtc < 30;

  return (
    <div className="page">
      <h1>Dashboard</h1>

      <section className="card status-card">
        <div className={`status-dot ${paused ? 'status-dot-paused' : 'status-dot-live'}`} />
        <div>
          <div className="status-title">{paused === null ? 'Статус неизвестен' : paused ? 'Приостановлено' : 'Отслеживание'}</div>
          {pauseError && <div className="error">{pauseError}</div>}
        </div>
        <button type="button" onClick={togglePause} disabled={paused === null}>
          {paused ? 'Продолжить' : 'Приостановить'}
        </button>
      </section>

      <section className="card">
        <h2>Текущая задача</h2>
        {isCurrentLive && current ? (
          <p>
            <strong>{current.issueKey ?? 'Без задачи'}</strong> — {formatDuration(current.activeSeconds)}
          </p>
        ) : (
          <p className="muted">Сейчас нет активной сессии.</p>
        )}
      </section>

      <section className="card">
        <h2>Сегодня</h2>
        {loading && <p>Загрузка…</p>}
        {error && <p className="error">{error}</p>}
        {view && (
          <dl className="stats-grid">
            <dt>Итого</dt>
            <dd>{formatDuration(view.totalActiveSeconds)}</dd>
            <dt>Не распределено</dt>
            <dd>{formatDuration(view.unassignedSeconds)}</dd>
            <dt>Сессий</dt>
            <dd>{view.sessions.filter((s) => s.deletedAt === null).length}</dd>
          </dl>
        )}
      </section>
    </div>
  )
}
