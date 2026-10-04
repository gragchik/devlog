import { useEffect, useState } from 'react'
import { listen } from '@tauri-apps/api/event'
import type { WorklogReminder } from '@shared/types/worklog'
import { formatDuration, formatDurationShort } from '@shared/utils/format-duration'
import type { Navigate } from '../navigation'
import { api, errorMessage } from '@shared/api'
import { useDaySessions } from '@shared/hooks/useDaySessions'

function ReminderCard({ navigate }: { navigate: Navigate }): JSX.Element | null {
  const [reminder, setReminder] = useState<WorklogReminder | null>(null)

  useEffect(() => {
    const load = (): void => {
      api.worklogGetReminder().then(setReminder).catch(() => setReminder(null))
    }
    load()
    // Назначение задач/отправка меняют картину — перечитываем по тому же
    // событию, что и сессии (оно приходит и после правок в overlay).
    const unlistenPromise = listen('sessions:changed', load)
    return () => {
      void unlistenPromise.then((unlisten) => unlisten())
    }
  }, [])

  if (!reminder) return null

  async function dismiss(): Promise<void> {
    if (!reminder) return
    await api.worklogDismissReminder(reminder.localDate).catch(() => {})
    setReminder(null)
  }

  return (
    <section className="card reminder-card">
      <h2>Отчёт за {reminder.localDate} не закрыт</h2>
      <p>
        {reminder.unreportedSeconds > 0 && (
          <>
            {formatDurationShort(reminder.unreportedSeconds)} по {reminder.unreportedIssueCount} задач(ам) ещё не
            отправлено в Jira.{' '}
          </>
        )}
        {reminder.unassignedSeconds > 0 && <>{formatDurationShort(reminder.unassignedSeconds)} без задачи.</>}
      </p>
      <div className="reminder-actions">
        {reminder.unassignedSeconds > 0 && (
          <button type="button" onClick={() => navigate('timeline', reminder.localDate)}>
            Назначить задачи
          </button>
        )}
        <button type="button" className="primary" onClick={() => navigate('worklog', reminder.localDate)}>
          Подготовить отчёт
        </button>
        <button type="button" className="link-button" onClick={() => void dismiss()}>
          Скрыть
        </button>
      </div>
    </section>
  )
}

export function Dashboard({ navigate }: { navigate: Navigate }): JSX.Element {
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

      <ReminderCard navigate={navigate} />

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
