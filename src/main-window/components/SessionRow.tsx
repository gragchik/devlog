import { useState } from 'react'
import type { WorkSession } from '@shared/types/work-session'
import { formatDurationShort, formatTimeOfDay } from '@shared/utils/format-duration'
import { api, errorMessage } from '@shared/api'

const STATUS_LABEL: Record<WorkSession['reviewStatus'], string> = {
  detected: 'распознано',
  unassigned: 'не распределено',
  reviewed: 'проверено',
  excluded: 'исключено'
}

const STATUS_CLASS: Record<WorkSession['reviewStatus'], string> = {
  detected: 'status-detected',
  unassigned: 'status-unassigned',
  reviewed: 'status-reviewed',
  excluded: 'status-excluded'
}

interface SessionRowProps {
  session: WorkSession
  /** Сосед снизу — нужен для кнопки "Объединить со следующей". `null`, если это последняя строка. */
  nextSession: WorkSession | null
  onChanged: () => void
}

export function SessionRow({ session, nextSession, onChanged }: SessionRowProps): JSX.Element {
  const [editingIssue, setEditingIssue] = useState(false);
  const [issueDraft, setIssueDraft] = useState(session.issueKey ?? '')
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)

  async function run(action: () => Promise<unknown>): Promise<void> {
    setBusy(true)
    setError(null)
    try {
      await action()
      onChanged()
    } catch (err) {
      setError(errorMessage(err))
    } finally {
      setBusy(false)
    }
  }

  function saveIssueKey(): void {
    const trimmed = issueDraft.trim()
    void run(() => api.updateSession(session.id, { issueKey: trimmed === '' ? null : trimmed })).then(() =>
      setEditingIssue(false)
    )
  }

  function handleSplit(): void {
    if (session.endedAtUtc === null) return
    const midpoint = Math.round((session.startedAtUtc + session.endedAtUtc) / 2)
    const input = window.prompt(
      `Время разбиения (ЧЧ:ММ, локально, между ${formatTimeOfDay(session.startedAtUtc)} и ${formatTimeOfDay(session.endedAtUtc)}):`,
      formatTimeOfDay(midpoint)
    )
    if (!input) return
    const match = /^(\d{1,2}):(\d{2})$/.exec(input.trim())
    if (!match) {
      setError('Ожидался формат ЧЧ:ММ')
      return
    }
    const base = new Date(session.startedAtUtc * 1000)
    base.setHours(Number(match[1]), Number(match[2]), 0, 0)
    const atUtc = Math.floor(base.getTime() / 1000)
    void run(() => api.splitSession(session.id, atUtc))
  }

  const canMergeWithNext =
    nextSession !== null && nextSession.startedAtUtc >= (session.endedAtUtc ?? Infinity) - 60 * 30 // не дальше получаса — разумная защита от случайного объединения далёких сессий

  return (
    <div className={`session-row ${STATUS_CLASS[session.reviewStatus]} ${session.deletedAt !== null ? 'session-row-deleted' : ''}`}>
      <div className="session-row-time">
        {formatTimeOfDay(session.startedAtUtc)}–{session.endedAtUtc ? formatTimeOfDay(session.endedAtUtc) : '…'}
      </div>
      <div className="session-row-duration">{formatDurationShort(session.activeSeconds)}</div>
      <div className="session-row-task">
        {editingIssue ? (
          <input
            autoFocus
            value={issueDraft}
            onChange={(e) => setIssueDraft(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === 'Enter') saveIssueKey()
              if (e.key === 'Escape') setEditingIssue(false)
            }}
            onBlur={saveIssueKey}
            placeholder="OB-448"
          />
        ) : (
          <button type="button" className="link-button" onClick={() => setEditingIssue(true)}>
            {session.issueKey ?? STATUS_LABEL[session.reviewStatus]}
          </button>
        )}
        {session.isManuallyEdited && <span className="badge" title="Изменено вручную">правка</span>}
        {session.deletedAt !== null && <span className="badge badge-deleted" title="Исключено из учёта">исключено</span>}
      </div>
      <div className="session-row-actions">
        {session.deletedAt === null ? (
          <>
            {session.endedAtUtc !== null && (
              <button type="button" disabled={busy} onClick={handleSplit}>
                Разбить
              </button>
            )}
            {canMergeWithNext && (
              <button type="button" disabled={busy} onClick={() => run(() => api.mergeSessions(session.id, nextSession!.id))}>
                Объединить ↓
              </button>
            )}
            <button type="button" disabled={busy} onClick={() => run(() => api.excludeSession(session.id))}>
              Исключить
            </button>
          </>
        ) : (
          <button type="button" disabled={busy} onClick={() => run(() => api.restoreSession(session.id))}>
            Восстановить
          </button>
        )}
        <button type="button" disabled={busy} title="Отменить последнюю правку" onClick={() => run(() => api.undoSessionEdit(session.id))}>
          ↶
        </button>
      </div>
      {error && <div className="session-row-error">{error}</div>}
    </div>
  )
}
