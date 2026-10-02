import { listen } from '@tauri-apps/api/event'
import { useCallback, useEffect, useState } from 'react'
import { api, errorMessage } from '../api'
import type { DaySessionsView } from '../types/work-session'

interface UseDaySessionsResult {
  view: DaySessionsView | null
  loading: boolean
  error: string | null
  reload: () => void
}

/**
 * Загружает сессии за `localDate` (`"today"`/`"yesterday"`/`"YYYY-MM-DD"`)
 * и перезагружает их по событию `sessions:changed` — трекер и любые
 * команды редактирования шлют его при изменениях (см.
 * `src-tauri/src/tracking/tracker.rs`, `commands/sessions.rs`), поэтому
 * здесь нет поллинга таймером. Используется и main-window, и overlay —
 * один и тот же сервис на оба окна (ТЗ, раздел 6).
 */
export function useDaySessions(localDate: string): UseDaySessionsResult {
  const [view, setView] = useState<DaySessionsView | null>(null)
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState<string | null>(null)
  const [reloadToken, setReloadToken] = useState(0)

  const reload = useCallback(() => setReloadToken((t) => t + 1), [])

  useEffect(() => {
    let cancelled = false
    setLoading(true)
    api
      .getSessionsForDay(localDate)
      .then((v) => {
        if (!cancelled) {
          setView(v)
          setError(null)
        }
      })
      .catch((err: unknown) => {
        if (!cancelled) setError(errorMessage(err))
      })
      .finally(() => {
        if (!cancelled) setLoading(false)
      })
    return () => {
      cancelled = true
    }
  }, [localDate, reloadToken])

  useEffect(() => {
    const unlistenPromise = listen('sessions:changed', () => reload())
    return () => {
      void unlistenPromise.then((unlisten) => unlisten())
    }
  }, [reload])

  return { view, loading, error, reload }
}
