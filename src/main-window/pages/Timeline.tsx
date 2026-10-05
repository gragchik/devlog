import { useEffect, useMemo, useRef, useState } from 'react'
import { listen } from '@tauri-apps/api/event'
import type { DaySummary, WorkSession } from '@shared/types/work-session'
import { formatDuration, formatDurationShort } from '@shared/utils/format-duration'
import { SessionRow } from '../components/SessionRow'
import { api, errorMessage } from '@shared/api'
import { useDaySessions } from '@shared/hooks/useDaySessions'

type StatusFilter = 'all' | 'unassigned' | 'assigned' | 'excluded'

const STATUS_FILTERS: { id: StatusFilter; label: string }[] = [
  { id: 'all', label: 'Все' },
  { id: 'unassigned', label: 'Без задачи' },
  { id: 'assigned', label: 'С задачей' },
  { id: 'excluded', label: 'Исключённые' }
]

// Сколько календарных дней назад запрашиваем за один раз — дни без единой
// сессии бэкенд не возвращает (см. `list_day_summaries`), так что
// реального списка обычно меньше. "Показать ещё" просто увеличивает это
// окно и перезапрашивает.
const INITIAL_DAY_LOOKBACK = 14
const DAY_LOOKBACK_STEP = 30

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

function formatDayLabel(localDate: string): string {
  const [y, m, d] = localDate.split('-').map(Number) as [number, number, number]
  const date = new Date(y, m - 1, d)
  const today = new Date()
  today.setHours(0, 0, 0, 0)
  const diffDays = Math.round((today.getTime() - date.getTime()) / 86_400_000)
  if (diffDays === 0) return 'Сегодня'
  if (diffDays === 1) return 'Вчера'
  return date.toLocaleDateString('ru-RU', { weekday: 'short', day: 'numeric', month: 'long' })
}

function pluralizeSessions(n: number): string {
  const mod10 = n % 10
  const mod100 = n % 100
  if (mod10 === 1 && mod100 !== 11) return 'сессия'
  if ([2, 3, 4].includes(mod10) && ![12, 13, 14].includes(mod100)) return 'сессии'
  return 'сессий'
}

/** Содержимое развёрнутого дня — список сессий грузится здесь (через
 * `useDaySessions`), т.е. только когда день реально развёрнут: родитель
 * монтирует этот компонент лишь при `expanded === true`, не держит все
 * дни в памяти сразу. */
function DayAccordionBody({
  localDate,
  statusFilter,
  query
}: {
  localDate: string
  statusFilter: StatusFilter
  query: string
}): JSX.Element {
  const { view, loading, error, reload } = useDaySessions(localDate)
  const allSessions = view?.sessions ?? []
  const visibleSessions = allSessions.filter((s) => matchesFilter(s, statusFilter, query))

  return (
    <div className="day-accordion-body">
      {loading && <p>Загрузка…</p>}
      {error && <p className="error">{error}</p>}
      {view && (
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
      )}
    </div>
  )
}

function DayAccordion({
  summary,
  expanded,
  onToggle,
  statusFilter,
  query
}: {
  summary: DaySummary
  expanded: boolean
  onToggle: () => void
  statusFilter: StatusFilter
  query: string
}): JSX.Element {
  return (
    <section className="card day-accordion">
      <button type="button" className="day-accordion-header" onClick={onToggle} aria-expanded={expanded}>
        <span className="day-accordion-caret">{expanded ? '▾' : '▸'}</span>
        <span className="day-accordion-date">{formatDayLabel(summary.localDate)}</span>
        <span className="day-accordion-stats">
          {formatDuration(summary.totalActiveSeconds)} · {summary.sessionCount} {pluralizeSessions(summary.sessionCount)}
          {summary.unassignedSeconds > 0 && <> · без задачи {formatDurationShort(summary.unassignedSeconds)}</>}
        </span>
      </button>
      {expanded && <DayAccordionBody localDate={summary.localDate} statusFilter={statusFilter} query={query} />}
    </section>
  )
}

export function Timeline({ initialDate }: { initialDate?: string }): JSX.Element {
  const [dayLookback, setDayLookback] = useState(INITIAL_DAY_LOOKBACK)
  const [summaries, setSummaries] = useState<DaySummary[] | null>(null)
  const [summariesError, setSummariesError] = useState<string | null>(null)
  const [expanded, setExpanded] = useState<Set<string>>(() => new Set(initialDate ? [initialDate] : []))
  const [statusFilter, setStatusFilter] = useState<StatusFilter>('all')
  const [query, setQuery] = useState('')
  const [manualError, setManualError] = useState<string | null>(null)
  // Без явного initialDate разворачиваем самый свежий день сам по себе,
  // один раз, когда сводки впервые приедут — дальше пользователь решает сам.
  const autoExpandedRef = useRef(initialDate !== undefined)

  useEffect(() => {
    let cancelled = false
    api
      .listDaySummaries(dayLookback)
      .then((list) => {
        if (cancelled) return
        setSummaries(list)
        setSummariesError(null)
        const first = list[0]
        if (!autoExpandedRef.current && first) {
          autoExpandedRef.current = true
          setExpanded((prev) => new Set(prev).add(first.localDate))
        }
      })
      .catch((err: unknown) => {
        if (!cancelled) setSummariesError(errorMessage(err))
      })
    return () => {
      cancelled = true
    }
  }, [dayLookback])

  useEffect(() => {
    const unlistenPromise = listen('sessions:changed', () => {
      void api.listDaySummaries(dayLookback).then(setSummaries).catch(() => {})
    })
    return () => {
      void unlistenPromise.then((unlisten) => unlisten())
    }
  }, [dayLookback])

  function toggleDay(localDate: string): void {
    setExpanded((prev) => {
      const next = new Set(prev)
      if (next.has(localDate)) next.delete(localDate)
      else next.add(localDate)
      return next
    })
  }

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
    } catch (err) {
      setManualError(errorMessage(err))
    }
  }

  // Если initialDate не попал в уже загруженную сводку (день за пределами
  // текущего окна lookback, крайне маловероятно — сводка не включает
  // только дни без единой сессии, а именно к такому дню нас не поведут),
  // всё равно рисуем для него строку: `DayAccordionBody` сам подтянет
  // реальные данные, когда она развёрнута (а она развёрнута по умолчанию).
  const visibleSummaries = useMemo(() => {
    if (!summaries) return summaries
    if (!initialDate || summaries.some((d) => d.localDate === initialDate)) return summaries
    const placeholder: DaySummary = { localDate: initialDate, totalActiveSeconds: 0, unassignedSeconds: 0, excludedSeconds: 0, sessionCount: 0 }
    return [...summaries, placeholder].sort((a, b) => (a.localDate < b.localDate ? 1 : -1))
  }, [summaries, initialDate])

  return (
    <div className="page">
      <h1>Timeline</h1>

      <div className="timeline-toolbar">
        <button type="button" onClick={() => void createManualSession()}>
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
      {summariesError && <p className="error">{summariesError}</p>}

      {visibleSummaries === null && <p>Загрузка…</p>}
      {visibleSummaries !== null && visibleSummaries.length === 0 && (
        <p className="muted">Нет сессий за последние {dayLookback} дней.</p>
      )}

      <div className="day-accordion-list">
        {visibleSummaries?.map((day) => (
          <DayAccordion
            key={day.localDate}
            summary={day}
            expanded={expanded.has(day.localDate)}
            onToggle={() => toggleDay(day.localDate)}
            statusFilter={statusFilter}
            query={query}
          />
        ))}
      </div>

      {visibleSummaries !== null && (
        <button type="button" className="link-button" onClick={() => setDayLookback((n) => n + DAY_LOOKBACK_STEP)}>
          Показать более старые дни
        </button>
      )}
    </div>
  )
}
