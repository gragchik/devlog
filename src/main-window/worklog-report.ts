import type { WorklogDayView, WorklogDraftView } from '@shared/types/jira'
import { formatDurationShort, formatTimeOfDay } from '@shared/utils/format-duration'

/** Можно выбрать к отправке: ещё черновик и нет неразрешённой попытки (FR-07.6). */
export function isSubmittable(draft: WorklogDraftView): boolean {
  const state = draft.lastSubmission?.state
  return draft.status === 'draft' && state !== 'unknown' && state !== 'pending'
}

/** FR-05.5: текст для ручного переноса в Jira, когда API недоступен. */
export function buildReportText(view: WorklogDayView): string {
  const lines = [`Отчёт за ${view.localDate}`, '']
  let total = 0
  for (const d of view.drafts) {
    total += d.timeSpentSeconds
    const title = d.issueTitle ? ` ${d.issueTitle}` : ''
    lines.push(`${d.issueKey}${title} — ${formatDurationShort(d.timeSpentSeconds)} (с ${formatTimeOfDay(d.startedAtUtc)})`)
    if (d.comment) lines.push(...d.comment.split('\n').map((l) => `  ${l}`))
  }
  lines.push('', `Итого: ${formatDurationShort(total)}`)
  return lines.join('\n')
}

/** `navigator.clipboard` с запасным путём через выделение textarea. */
export async function copyText(text: string): Promise<void> {
  try {
    await navigator.clipboard.writeText(text)
    return
  } catch {
    // ниже — fallback
  }
  const area = document.createElement('textarea')
  area.value = text
  area.style.position = 'fixed'
  area.style.opacity = '0'
  document.body.appendChild(area)
  area.select()
  const ok = document.execCommand('copy')
  document.body.removeChild(area)
  if (!ok) throw new Error('буфер обмена недоступен')
}
