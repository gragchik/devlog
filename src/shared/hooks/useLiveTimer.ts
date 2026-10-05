import { useEffect, useRef, useState } from 'react'

/**
 * Плавно "дотикивает" секунды между синками с бэкендом — `totalActiveSeconds`
 * из `useDaySessions` обновляется только по событию `sessions:changed`
 * (трекер пересобирает сессии дня раз в ~30с, см. `tracker.rs`), а таймер
 * в UI должен идти каждую секунду. `baseSeconds` — последнее известное
 * значение с бэкенда; `running` — тикать ли дальше (false — замереть
 * на месте, например на паузе или при выключенной сессии).
 */
export function useLiveTimer(baseSeconds: number, running: boolean): number {
  const [displaySeconds, setDisplaySeconds] = useState(baseSeconds)
  const baseRef = useRef(baseSeconds)
  const syncedAtRef = useRef(Date.now())

  useEffect(() => {
    baseRef.current = baseSeconds
    syncedAtRef.current = Date.now()
    setDisplaySeconds(baseSeconds)
  }, [baseSeconds])

  useEffect(() => {
    if (!running) return
    const interval = setInterval(() => {
      const elapsedSeconds = Math.floor((Date.now() - syncedAtRef.current) / 1000)
      setDisplaySeconds(baseRef.current + elapsedSeconds)
    }, 1000)
    return () => clearInterval(interval)
  }, [running])

  return displaySeconds
}
