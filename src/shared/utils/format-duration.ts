/** Форматирует секунды в `H:MM:SS` — секунды это основной формат хранения времени в приложении (ТЗ, раздел 4). */
export function formatDuration(totalSeconds: number): string {
  const whole = Math.max(0, Math.floor(totalSeconds))
  const hours = Math.floor(whole / 3600)
  const minutes = Math.floor((whole % 3600) / 60)
  const seconds = whole % 60
  const pad = (n: number): string => n.toString().padStart(2, '0')
  return `${hours}:${pad(minutes)}:${pad(seconds)}`
}

/** Короткий формат для компактных мест (таймлайн): `1ч 23м`, `45м`, `< 1м`. */
export function formatDurationShort(totalSeconds: number): string {
  const whole = Math.max(0, Math.floor(totalSeconds))
  const hours = Math.floor(whole / 3600)
  const minutes = Math.floor((whole % 3600) / 60)
  if (hours > 0) return `${hours}ч ${minutes}м`
  if (minutes > 0) return `${minutes}м`
  return '< 1м'
}

/** `HH:MM` в локальном времени для меток на таймлайне. */
export function formatTimeOfDay(epochSeconds: number): string {
  return new Date(epochSeconds * 1000).toLocaleTimeString(undefined, { hour: '2-digit', minute: '2-digit' })
}
