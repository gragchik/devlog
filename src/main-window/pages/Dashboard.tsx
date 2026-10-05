import { useEffect, useState } from 'react'
import { listen } from '@tauri-apps/api/event'
import type { WorklogReminder } from '@shared/types/worklog'
import { formatDuration, formatDurationShort, formatTimeOfDay } from '@shared/utils/format-duration'
import type { Navigate } from '../navigation'
import { api, errorMessage } from '@shared/api'
import { useDaySessions } from '@shared/hooks/useDaySessions'
import { useLiveTimer } from '@shared/hooks/useLiveTimer'
import { ToggleSwitch } from '@shared/components/ToggleSwitch'

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
  const [sessionActive, setSessionActive] = useState<boolean | null>(null)
  const [statusError, setStatusError] = useState<string | null>(null)

  useEffect(() => {
    function refreshStatus(): void {
      api.getTrackingPaused().then(setPaused).catch(() => {})
      api.getSessionActive().then(setSessionActive).catch(() => {})
    }
    refreshStatus()
    // Опрос — на случай, если состояние меняют из overlay/трея без
    // события (например, трей сам шлёт его — но лишний интервал как
    // страховка не повредит); `tracking:changed` — для немедленной
    // реакции в этом же тике.
    const interval = setInterval(refreshStatus, 5000)
    const unlistenPromise = listen('tracking:changed', refreshStatus)
    return () => {
      clearInterval(interval)
      void unlistenPromise.then((unlisten) => unlisten())
    }
  }, [])

  async function togglePause(): Promise<void> {
    if (paused === null) return
    try {
      setPaused(await api.setTrackingPaused(!paused))
    } catch (err) {
      setStatusError(errorMessage(err))
    }
  }

  async function toggleSessionActive(next: boolean): Promise<void> {
    try {
      setSessionActive(await api.setSessionActive(next))
    } catch (err) {
      setStatusError(errorMessage(err))
    }
  }

  // Таймер идёт, только когда сессия включена и не на паузе — пока оба
  // значения не загрузились (`null`), считаем, что не идёт, чтобы не
  // дёргать отображение лишний раз сразу после монтирования.
  const isRunning = sessionActive === true && paused === false
  const liveActiveSeconds = useLiveTimer(view?.totalActiveSeconds ?? 0, isRunning)

  const todaySessions = view ? view.sessions.filter((s) => s.deletedAt === null) : []
  const previewSessions = todaySessions
    .filter((s) => s.reviewStatus !== 'excluded')
    .slice()
    .sort((a, b) => b.startedAtUtc - a.startedAtUtc)
    .slice(0, 5)

  return (
    <div className="page">
      <h1>Dashboard</h1>

      <ReminderCard navigate={navigate} />

      <section className="card status-card">
        <div className="status-card-row">
          <span className="status-title">Активная сессия</span>
          <ToggleSwitch
            checked={sessionActive ?? false}
            onChange={(next) => void toggleSessionActive(next)}
            disabled={sessionActive === null}
          />
        </div>
        <div className="status-card-row">
          <div className={`status-dot ${sessionActive === false ? 'status-dot-off' : paused ? 'status-dot-paused' : 'status-dot-live'}`} />
          <span className="muted">
            {sessionActive === null ? 'Статус неизвестен' : sessionActive === false ? 'Сессия выключена' : paused ? 'На паузе' : 'Идёт отслеживание'}
          </span>
          <button type="button" onClick={togglePause} disabled={paused === null || sessionActive === false}>
            {paused ? 'Продолжить' : 'Приостановить'}
          </button>
        </div>
        {statusError && <div className="error">{statusError}</div>}
      </section>

      <section className="card">
        <h2>Время активной работы</h2>
        <p className="active-time-display">{formatDuration(liveActiveSeconds)}</p>
        {!isRunning && sessionActive !== null && (
          <p className="muted">{sessionActive === false ? 'Сессия выключена — время не считается.' : 'На паузе.'}</p>
        )}
      </section>

      <section className="card">
        <h2>Сегодня</h2>
        {loading && <p>Загрузка…</p>}
        {error && <p className="error">{error}</p>}
        {view && (
          <>
            <dl className="stats-grid">
              <dt>Итого</dt>
              <dd>{formatDuration(view.totalActiveSeconds)}</dd>
              <dt>Не распределено</dt>
              <dd>{formatDuration(view.unassignedSeconds)}</dd>
              <dt>Сессий</dt>
              <dd>{todaySessions.length}</dd>
            </dl>

            <ul className="today-preview">
              {previewSessions.length === 0 && <li className="muted today-preview-empty">Пока ничего не сделано.</li>}
              {previewSessions.map((s) => (
                <li key={s.id} className="today-preview-row">
                  <span className="today-preview-time">{formatTimeOfDay(s.startedAtUtc)}</span>
                  <span className="today-preview-task">{s.issueKey ?? s.description ?? 'Без задачи'}</span>
                  <span className="today-preview-duration">{formatDurationShort(s.activeSeconds)}</span>
                </li>
              ))}
            </ul>
            {todaySessions.length > 0 && (
              <button type="button" className="link-button" onClick={() => navigate('timeline', view.localDate)}>
                Вся активность →
              </button>
            )}
          </>
        )}
      </section>
    </div>
  )
}
