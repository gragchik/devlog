export type Tab = 'dashboard' | 'timeline' | 'worklog' | 'settings'

export const TABS: { id: Tab; label: string }[] = [
  { id: 'dashboard', label: 'Dashboard' },
  { id: 'timeline', label: 'Timeline' },
  { id: 'worklog', label: 'Worklog' },
  { id: 'settings', label: 'Settings' }
]

export function isTab(value: string): value is Tab {
  return TABS.some((t) => t.id === value)
}

/** Переход на вкладку, опционально сразу на конкретный день (`YYYY-MM-DD`). */
export type Navigate = (tab: Tab, localDate?: string) => void

/** Сегодняшняя дата в локальной таймзоне как `YYYY-MM-DD`. */
export function localToday(): string {
  const d = new Date()
  const pad = (n: number): string => n.toString().padStart(2, '0')
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`
}
