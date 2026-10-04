import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import type {
  DraftSubmitResult,
  JiraConnectionStatus,
  UnknownSubmissionView,
  WorklogDayView,
  WorklogDraftView
} from '@shared/types/jira'
import { api, errorMessage } from '@shared/api'
import { useDaySessions } from '@shared/hooks/useDaySessions'
import type { CommentTemplate } from '@shared/types/worklog'
import { formatDurationShort, formatTimeOfDay } from '@shared/utils/format-duration'
import { localToday, type Navigate } from '../navigation'
import { buildReportText, copyText, isSubmittable } from '../worklog-report'

type DateChoice = 'today' | 'yesterday' | string

function statusLabel(draft: WorklogDraftView): { text: string; className: string } {
  const sub = draft.lastSubmission
  if (draft.status === 'submitted') {
    return { text: sub?.remoteWorklogId ? `В Jira (#${sub.remoteWorklogId})` : 'В Jira', className: 'worklog-status-posted' }
  }
  if (sub?.state === 'unknown' || sub?.state === 'pending') {
    return { text: 'Результат отправки неизвестен', className: 'worklog-status-unknown' }
  }
  if (sub?.state === 'failed') return { text: 'Не отправлено (ошибка)', className: 'worklog-status-failed' }
  return { text: 'Черновик', className: '' }
}

interface DraftRowProps {
  draft: WorklogDraftView
  selected: boolean
  jiraConfigured: boolean
  templates: CommentTemplate[]
  onToggle: () => void
  onChanged: () => void
}

function DraftRow({ draft, selected, jiraConfigured, templates, onToggle, onChanged }: DraftRowProps): JSX.Element {
  const [issueKey, setIssueKey] = useState(draft.issueKey)
  const [minutes, setMinutes] = useState(String(draft.timeSpentSeconds / 60))
  const [comment, setComment] = useState(draft.comment ?? '')
  const [error, setError] = useState<string | null>(null)
  const [busy, setBusy] = useState(false)

  useEffect(() => {
    setIssueKey(draft.issueKey)
    setMinutes(String(draft.timeSpentSeconds / 60))
    setComment(draft.comment ?? '')
  }, [draft.issueKey, draft.timeSpentSeconds, draft.comment])

  const sub = draft.lastSubmission
  const unresolved = sub !== null && (sub.state === 'unknown' || sub.state === 'pending')
  const editable = draft.status === 'draft' && !unresolved
  const status = statusLabel(draft)

  async function run(action: () => Promise<unknown>): Promise<void> {
    setBusy(true)
    try {
      await action()
      setError(null)
      onChanged()
    } catch (err) {
      setError(errorMessage(err))
    } finally {
      setBusy(false)
    }
  }

  function saveIssueKey(): void {
    const next = issueKey.trim().toUpperCase()
    if (next === draft.issueKey) return
    void run(() => api.worklogUpdateDraft(draft.id, { issueKey: next }))
  }

  function saveMinutes(): void {
    const value = Number(minutes)
    if (!Number.isInteger(value) || value < 1) {
      setError('Время — целое число минут, не меньше 1')
      return
    }
    if (value * 60 === draft.timeSpentSeconds) return
    void run(() => api.worklogUpdateDraft(draft.id, { timeSpentSeconds: value * 60 }))
  }

  function saveComment(): void {
    if (comment === (draft.comment ?? '')) return
    void run(() => api.worklogUpdateDraft(draft.id, { comment }))
  }

  return (
    <div className={`worklog-row ${draft.status === 'submitted' ? 'worklog-row-done' : ''}`}>
      <div className="worklog-row-head">
        <input type="checkbox" checked={selected} disabled={!isSubmittable(draft) || !jiraConfigured} onChange={onToggle} />
        <input
          className="worklog-issue"
          value={issueKey}
          disabled={!editable || busy}
          onChange={(e) => setIssueKey(e.target.value)}
          onBlur={saveIssueKey}
        />
        <span className="worklog-title">{draft.issueTitle ?? <span className="muted">название не загружено</span>}</span>
        {jiraConfigured && (
          <button type="button" className="link-button" disabled={busy} onClick={() => void run(() => api.jiraFetchIssue(draft.issueKey))}>
            ↻ из Jira
          </button>
        )}
        <span className={`badge ${status.className}`}>{status.text}</span>
      </div>
      <div className="worklog-row-body">
        <label>
          Начало
          <span>{formatTimeOfDay(draft.startedAtUtc)}</span>
        </label>
        <label>
          Минут
          <input
            type="number"
            min={1}
            step={5}
            value={minutes}
            disabled={!editable || busy}
            onChange={(e) => setMinutes(e.target.value)}
            onBlur={saveMinutes}
          />
          <span className="muted">{formatDurationShort(draft.timeSpentSeconds)}</span>
        </label>
        <textarea
          rows={2}
          value={comment}
          disabled={!editable || busy}
          placeholder="Комментарий для Jira"
          onChange={(e) => setComment(e.target.value)}
          onBlur={saveComment}
        />
      </div>
      <div className="worklog-row-actions">
        {unresolved && sub && (
          <>
            <span className="muted">
              Сеть оборвалась во время отправки — Jira могла принять запись. Повторно отправлять нельзя, пока не проверено.
            </span>
            {jiraConfigured && (
              <button type="button" disabled={busy} onClick={() => void run(() => api.jiraReconcileSubmission(sub.id))}>
                Проверить в Jira
              </button>
            )}
            <button type="button" disabled={busy} onClick={() => void run(() => api.jiraResolveSubmissionManually(sub.id, true))}>
              Запись в Jira есть
            </button>
            <button type="button" disabled={busy} onClick={() => void run(() => api.jiraResolveSubmissionManually(sub.id, false))}>
              Записи в Jira нет
            </button>
          </>
        )}
        {editable && (
          <>
            <select
              value=""
              disabled={busy}
              onChange={(e) => {
                const templateId = e.target.value
                if (templateId) void run(() => api.worklogApplyTemplate(draft.id, templateId))
              }}
            >
              <option value="">Текст по шаблону…</option>
              {templates.map((t) => (
                <option key={t.id} value={t.id}>
                  {t.name}
                </option>
              ))}
            </select>
            <button
              type="button"
              disabled={busy}
              title="Взять время и начало из текущих сессий задачи за день (после правок в Timeline). Комментарий не меняется."
              onClick={() => void run(() => api.worklogRecalculateDraft(draft.id))}
            >
              Пересчитать из сессий
            </button>
            <button type="button" disabled={busy} title="Скопировать комментарий" onClick={() => void run(() => copyText(comment))}>
              Копировать текст
            </button>
            <button type="button" className="danger" disabled={busy} onClick={() => void run(() => api.worklogDeleteDraft(draft.id))}>
              Удалить черновик
            </button>
          </>
        )}
      </div>
      {error && <p className="error">{error}</p>}
    </div>
  )
}

function outcomeText(result: DraftSubmitResult): string {
  const o = result.outcome
  switch (o.kind) {
    case 'posted':
      return o.remoteWorklogId ? `отправлено (#${o.remoteWorklogId})` : 'отправлено'
    case 'failed':
      return `ошибка: ${o.message}`
    case 'unknown':
      return `результат неизвестен — проверьте: ${o.message}`
    case 'blocked':
      return `не отправлено: ${o.message}`
  }
}

/**
 * Worklog Review (FR-05.5, FR-07.5): черновики за день → правка →
 * preview выбранных записей → явное «Отправить». «Копировать отчёт» —
 * ручной перенос, когда Jira API нет или недоступен.
 */
export function WorklogReview({ initialDate, navigate }: { initialDate?: string; navigate: Navigate }): JSX.Element {
  const [dateChoice, setDateChoice] = useState<DateChoice>(initialDate ?? 'yesterday')
  const [view, setView] = useState<WorklogDayView | null>(null)
  const [templates, setTemplates] = useState<CommentTemplate[]>([])
  const sessions = useDaySessions(dateChoice).view
  // Дни, для которых черновики уже пытались сформировать автоматически —
  // если пользователь удалил все черновики, не создаём их снова за его спиной.
  const autoGenerated = useRef(new Set<string>())
  const [jira, setJira] = useState<JiraConnectionStatus | null>(null)
  const [selected, setSelected] = useState<Set<string>>(new Set())
  const [previewOpen, setPreviewOpen] = useState(false)
  const [submitting, setSubmitting] = useState(false)
  const [results, setResults] = useState<DraftSubmitResult[] | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [notice, setNotice] = useState<string | null>(null)
  const [unknown, setUnknown] = useState<UnknownSubmissionView[]>([])

  const reload = useCallback(() => {
    api
      .worklogGetDay(dateChoice)
      .then(async (v) => {
        // Прошедший день без черновиков — формируем сразу, чтобы отчёт за
        // вчера занимал минуту, а не поиск нужной кнопки (Итерация 8).
        // Сегодня не трогаем: день ещё не закончен.
        if (v.drafts.length === 0 && v.localDate < localToday() && !autoGenerated.current.has(v.localDate)) {
          autoGenerated.current.add(v.localDate)
          v = await api.worklogGenerateDrafts(v.localDate)
        }
        setView(v)
        setError(null)
      })
      .catch((err: unknown) => setError(errorMessage(err)))
  }, [dateChoice])

  useEffect(() => {
    api
      .worklogGetTemplates()
      .then((s) => setTemplates(s.templates))
      .catch(() => setTemplates([]))
  }, [])

  useEffect(() => {
    setSelected(new Set())
    setPreviewOpen(false)
    setResults(null)
    reload()
  }, [reload])

  useEffect(() => {
    api.jiraGetConnectionStatus().then(setJira).catch(() => setJira(null))
  }, [])

  // Перезапрашиваем вместе с днём: после reconcile/submit список меняется.
  useEffect(() => {
    api.jiraListUnknownSubmissions().then(setUnknown).catch(() => setUnknown([]))
  }, [view])

  const drafts = useMemo(() => view?.drafts ?? [], [view])
  const chosen = drafts.filter((d) => selected.has(d.id) && isSubmittable(d))
  const chosenTotal = chosen.reduce((sum, d) => sum + d.timeSpentSeconds, 0)
  const dayTotal = drafts.reduce((sum, d) => sum + d.timeSpentSeconds, 0)
  const jiraConfigured = jira?.configured ?? false

  const submittable = drafts.filter(isSubmittable)
  const allSelected = submittable.length > 0 && submittable.every((d) => selected.has(d.id))

  function toggleAll(): void {
    setSelected(allSelected ? new Set() : new Set(submittable.map((d) => d.id)))
  }

  async function generate(): Promise<void> {
    try {
      setView(await api.worklogGenerateDrafts(dateChoice))
      setError(null)
    } catch (err) {
      setError(errorMessage(err))
    }
  }

  function toggle(id: string): void {
    setSelected((prev) => {
      const next = new Set(prev)
      if (next.has(id)) next.delete(id)
      else next.add(id)
      return next
    })
  }

  async function copyReport(): Promise<void> {
    if (!view) return
    try {
      await copyText(buildReportText(view))
      setNotice('Отчёт скопирован в буфер обмена.')
      setTimeout(() => setNotice(null), 2500)
    } catch (err) {
      setError(`Не удалось скопировать: ${errorMessage(err)}`)
    }
  }

  async function submit(): Promise<void> {
    setSubmitting(true)
    try {
      setResults(await api.jiraSubmitDrafts(chosen.map((d) => d.id)))
      setPreviewOpen(false)
      setSelected(new Set())
      reload()
    } catch (err) {
      setError(errorMessage(err))
    } finally {
      setSubmitting(false)
    }
  }

  return (
    <div className="page">
      <h1>Worklog Review</h1>

      <div className="timeline-toolbar">
        <button type="button" className={dateChoice === 'yesterday' ? 'active' : ''} onClick={() => setDateChoice('yesterday')}>
          Вчера
        </button>
        <button type="button" className={dateChoice === 'today' ? 'active' : ''} onClick={() => setDateChoice('today')}>
          Сегодня
        </button>
        <input
          type="date"
          value={dateChoice === 'today' || dateChoice === 'yesterday' ? (view?.localDate ?? '') : dateChoice}
          onChange={(e) => e.target.value && setDateChoice(e.target.value)}
        />
        <button type="button" onClick={generate}>
          Сформировать из сессий
        </button>
        <button type="button" onClick={copyReport} disabled={drafts.length === 0}>
          Копировать отчёт
        </button>
      </div>

      {jira && !jira.configured && (
        <p className="muted">
          Jira не подключена (Settings → Jira) — отчёт можно скопировать и перенести вручную.
        </p>
      )}
      {error && <p className="error">{error}</p>}
      {sessions && sessions.unassignedSeconds >= 60 && (
        <p className="muted">
          {formatDurationShort(sessions.unassignedSeconds)} за этот день без задачи и в отчёт не попадёт.{' '}
          <button type="button" className="link-button" onClick={() => navigate('timeline', sessions.localDate)}>
            Назначить в Timeline
          </button>
        </p>
      )}
      {notice && <p className="muted">{notice}</p>}

      {unknown.some((u) => u.localDay !== view?.localDate) && (
        <section className="card">
          <h2>Отправки с неизвестным результатом</h2>
          <p className="muted">
            Связь с Jira оборвалась во время отправки — запись могла создаться. Откройте день и проверьте её, прежде чем
            отправлять снова.
          </p>
          <ul className="worklog-results">
            {unknown
              .filter((u) => u.localDay !== view?.localDate)
              .map((u) => (
                <li key={u.id} className="worklog-result-unknown">
                  <strong>{u.issueKey}</strong>{' '}
                  {u.localDay ? (
                    <button type="button" className="link-button" onClick={() => setDateChoice(u.localDay ?? 'today')}>
                      за {u.localDay}
                    </button>
                  ) : (
                    '(черновик удалён)'
                  )}
                </li>
              ))}
          </ul>
        </section>
      )}

      {results && (
        <section className="card">
          <h2>Результат отправки</h2>
          <ul className="worklog-results">
            {results.map((r) => {
              const draft = drafts.find((d) => d.id === r.draftId)
              return (
                <li key={r.draftId} className={`worklog-result-${r.outcome.kind}`}>
                  <strong>{draft?.issueKey ?? r.draftId}</strong> — {outcomeText(r)}
                </li>
              )
            })}
          </ul>
        </section>
      )}

      <section className="card">
        <h2>
          Черновики {view ? `за ${view.localDate}` : ''} · итого {formatDurationShort(dayTotal)}
        </h2>
        {drafts.length === 0 && (
          <p className="muted">
            Черновиков нет. «Сформировать из сессий» создаст по записи на каждую задачу дня; нераспределённое время сначала
            назначьте задаче в Timeline.
          </p>
        )}
        {drafts.map((d) => (
          <DraftRow
            key={d.id}
            draft={d}
            selected={selected.has(d.id)}
            jiraConfigured={jiraConfigured}
            templates={templates}
            onToggle={() => toggle(d.id)}
            onChanged={reload}
          />
        ))}
        {jiraConfigured && drafts.length > 0 && (
          <div className="worklog-footer">
            <span className="muted">
              Выбрано: {chosen.length} · {formatDurationShort(chosenTotal)}
            </span>
            <button type="button" className="link-button" disabled={submittable.length === 0} onClick={toggleAll}>
              {allSelected ? 'Снять выбор' : 'Выбрать все'}
            </button>
            <button type="button" disabled={chosen.length === 0} onClick={() => setPreviewOpen(true)}>
              Предпросмотр отправки…
            </button>
          </div>
        )}
      </section>

      {previewOpen && (
        <section className="card worklog-preview">
          <h2>Будет отправлено в Jira ({jira?.baseUrl})</h2>
          <table>
            <thead>
              <tr>
                <th>Задача</th>
                <th>Дата и начало</th>
                <th>Время</th>
                <th>Комментарий</th>
              </tr>
            </thead>
            <tbody>
              {chosen.map((d) => (
                <tr key={d.id}>
                  <td>
                    <strong>{d.issueKey}</strong>
                    {d.issueTitle && <div className="muted">{d.issueTitle}</div>}
                  </td>
                  <td>
                    {d.localDay} {formatTimeOfDay(d.startedAtUtc)}
                  </td>
                  <td>{formatDurationShort(d.timeSpentSeconds)}</td>
                  <td className="worklog-preview-comment">{d.comment ?? ''}</td>
                </tr>
              ))}
            </tbody>
          </table>
          <p>
            Итого: <strong>{formatDurationShort(chosenTotal)}</strong> в {chosen.length} записях. Оценка (estimate) задач не
            изменяется.
          </p>
          <div className="worklog-footer">
            <button type="button" onClick={() => setPreviewOpen(false)} disabled={submitting}>
              Отмена
            </button>
            <button type="button" className="primary" onClick={submit} disabled={submitting || chosen.length === 0}>
              {submitting ? 'Отправка…' : `Отправить ${chosen.length} в Jira`}
            </button>
          </div>
        </section>
      )}
    </div>
  )
}
