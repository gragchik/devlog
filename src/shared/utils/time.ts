/**
 * Форматирует секунды в `H:MM:SS`. Секунды — основной формат хранения времени
 * во всём приложении (ТЗ, раздел 4 "Правила времени"). Используется в
 * overlay-таймере и на Dashboard — переиспользуемая чистая функция без
 * привязки к состоянию трекера.
 */
export function secondsToHms(totalSeconds: number): string {
  if (!Number.isFinite(totalSeconds) || totalSeconds < 0) {
    throw new RangeError(`secondsToHms: ожидалось неотрицательное конечное число, получено ${totalSeconds}`)
  }
  const whole = Math.floor(totalSeconds)
  const hours = Math.floor(whole / 3600)
  const minutes = Math.floor((whole % 3600) / 60)
  const seconds = whole % 60
  const pad = (n: number): string => n.toString().padStart(2, '0')
  return `${hours}:${pad(minutes)}:${pad(seconds)}`
}
