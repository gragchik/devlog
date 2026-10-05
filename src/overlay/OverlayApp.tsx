import { getCurrentWindow } from '@tauri-apps/api/window'
import { listen } from '@tauri-apps/api/event'
import { useEffect, useState } from 'react'
import { api, errorMessage } from '@shared/api'
import { useDaySessions } from '@shared/hooks/useDaySessions'
import type { WorkSession } from '@shared/types/work-session'
import { formatDuration, formatDurationShort, formatTimeOfDay } from '@shared/utils/format-duration'
import { ToggleSwitch } from '@shared/components/ToggleSwitch'

type Tab = 'today' | 'yesterday' | 'unassigned'

function visibleSessions(sessions: WorkSession[], tab: Tab): WorkSession[] {
  const notDeleted = sessions.filter((s) => s.deletedAt === null)
  return tab === 'unassigned' ? notDeleted.filter((s) => s.reviewStatus === 'unassigned') : notDeleted
}

export function OverlayApp(): JSX.Element {
  const [tab, setTab] = useState<Tab>('today')
  // "Нераспределено" показывает сегодняшние unassigned-сессии — отдельного
  // дня под эту вкладку не заводим (ТЗ, пример макета FR-06.7).
  const dataDate = tab === 'yesterday' ? 'yesterday' : 'today'
  const { view, reload } = useDaySessions(dataDate)

  const [paused, setPaused] = useState<boolean | null>(null)
  const [sessionActive, setSessionActive] = useState<boolean | null>(null)
  const [pinnedIssue, setPinnedIssue] = useState<string | null>(null)
  const [error, setError] = useState<string | null>(null)

  useEffect(() => {
    function refreshStatus(): void {
      api.getTrackingPaused().then(setPaused).catch(() => {})
      api.getSessionActive().then(setSessionActive).catch(() => {})
    }
    refreshStatus()
    api.getPinnedIssue().then(setPinnedIssue).catch(() => {})
    const interval = setInterval(refreshStatus, 5000)
    const unlistenPromise = listen('tracking:changed', refreshStatus)
    return () => {
      clearInterval(interval)
      void unlistenPromise.then((unlisten) => unlisten())
    }
  }, [])

  useEffect(() => {
    // FR-06.5: Escape закрывает overlay. Дублирует обработчик в Rust
    // (before-input-event ловит его тоже) — здесь на случай, если фокус
    // внутри renderer.
    function onKeyDown(event: KeyboardEvent): void {
      if (event.key === 'Escape') {
        void getCurrentWindow().hide()
      }
    }
    window.addEventListener('keydown', onKeyDown)
    return () => window.removeEventListener('keydown', onKeyDown)
  }, [])

  async function togglePause(): Promise<void> {
    if (paused === null) return
    try {
      setPaused(await api.setTrackingPaused(!paused))
    } catch (err) {
      setError(errorMessage(err))
    }
  }

  async function toggleSessionActive(next: boolean): Promise<void> {
    try {
      setSessionActive(await api.setSessionActive(next))
    } catch (err) {
      setError(errorMessage(err))
    }
  }

  function switchTask(): void {
    const input = window.prompt('Issue key новой задачи (пусто — снять закрепление):', pinnedIssue ?? '')
    if (input === null) return
    const trimmed = input.trim()
    void api
      .setPinnedIssue(trimmed === '' ? null : trimmed)
      .then(() => {
        setPinnedIssue(trimmed === '' ? null : trimmed)
        reload()
      })
      .catch((err: unknown) => setError(errorMessage(err)))
  }

  function createQuickSession(): void {
    const description = window.prompt('Описание сессии:')
    if (description === null) return
    const now = Math.floor(Date.now() / 1000)
    void api
      .createManualSession({
        startedAtUtc: now - 30 * 60,
        endedAtUtc: now,
        timezoneId: Intl.DateTimeFormat().resolvedOptions().timeZone,
        activeSeconds: 30 * 60,
        source: 'manual',
        description,
        reviewStatus: 'reviewed'
      })
      .then(() => reload())
      .catch((err: unknown) => setError(errorMessage(err)))
  }

  function assignIssue(session: WorkSession): void {
    const input = window.prompt('Issue key:', session.issueKey ?? '')
    if (input === null) return
    const trimmed = input.trim()
    void api
      .updateSession(session.id, { issueKey: trimmed === '' ? null : trimmed })
      .then(() => reload())
      .catch((err: unknown) => setError(errorMessage(err)))
  }

  const rows = view ? visibleSessions(view.sessions, tab) : []
  const current = rows.at(-1) ?? null
  const isCurrentLive = current !== null && current.endedAtUtc !== null && Date.now() / 1000 - current.endedAtUtc < 30

  return (
    <div className="overlay-shell">
      <header className="overlay-header">
        <span className="overlay-title">DevLog</span>
        <span className={`overlay-status-dot ${sessionActive === false ? 'off' : paused ? 'paused' : 'live'}`} />
        <span className="overlay-status">{sessionActive === false ? 'Off' : paused ? 'Paused' : 'Tracking'}</span>
        <ToggleSwitch
          size="sm"
          checked={sessionActive ?? false}
          onChange={(next) => void toggleSessionActive(next)}
          disabled={sessionActive === null}
        />
        <button type="button" className="overlay-close" onClick={() => void getCurrentWindow().hide()} aria-label="Закрыть">
          ✕
        </button>
      </header>

      <div className="overlay-current">
        <span>
          Сейчас:{' '}
          <strong>
            {sessionActive === false ? 'сессия выключена' : isCurrentLive ? (current?.issueKey ?? 'Без задачи') : pinnedIssue ? pinnedIssue : 'нет активности'}
          </strong>
          {sessionActive !== false && isCurrentLive && current && <> • {formatDuration(current.activeSeconds)}</>}
        </span>
        <button type="button" onClick={togglePause} disabled={paused === null || sessionActive === false}>
          {paused ? 'Продолжить' : 'Пауза'}
        </button>
      </div>
      <div className="overlay-actions">
        <button type="button" onClick={switchTask}>
          Сменить задачу
        </button>
        <button type="button" onClick={createQuickSession}>
          + Сессия
        </button>
      </div>

      <div className="overlay-tabs">
        {(['today', 'yesterday', 'unassigned'] as const).map((t) => (
          <button key={t} type="button" className={tab === t ? 'active' : ''} onClick={() => setTab(t)}>
            {t === 'today' ? 'Сегодня' : t === 'yesterday' ? 'Вчера' : 'Нераспределено'}
          </button>
        ))}
      </div>

      {error && <p className="overlay-error">{error}</p>}

      <div className="overlay-list">
        {rows.length === 0 && <p className="muted overlay-empty">Нет сессий.</p>}
        {rows.map((s) => (
          <div key={s.id} className={`overlay-row status-${s.reviewStatus}`}>
            <span className="overlay-row-time">
              {formatTimeOfDay(s.startedAtUtc)}–{s.endedAtUtc ? formatTimeOfDay(s.endedAtUtc) : '…'}
            </span>
            <span className="overlay-row-task">{s.issueKey ?? 'Без задачи'}</span>
            <span className="overlay-row-duration">{formatDurationShort(s.activeSeconds)}</span>
            <button type="button" onClick={() => assignIssue(s)}>
              {s.issueKey ? 'Изменить' : 'Назначить'}
            </button>
          </div>
        ))}
      </div>

      <footer className="overlay-footer">
        <span>Итого: {view ? formatDuration(view.totalActiveSeconds) : '…'}</span>
        <button type="button" onClick={() => void api.showMainWindow('worklog')}>
          Отчёт
        </button>
        <button type="button" onClick={() => void api.showMainWindow()}>
          Открыть
        </button>
      </footer>
    </div>
  )
}
