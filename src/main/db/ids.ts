import { randomUUID } from 'node:crypto'

export function generateId(): string {
  return randomUUID()
}

/** Текущее время как эпоха в секундах UTC — единый формат хранения времени в БД. */
export function nowUtcSeconds(): number {
  return Math.floor(Date.now() / 1000)
}
