import type { SessionEditOperation, WorkSession } from './work-session'

/**
 * Запись журнала правок сессии (FR-04.2/FR-04.8, раздел 5 "session_edits").
 * Insert-only: правки не удаляют и не перезаписывают первичную историю.
 * `previousState`/`nextState` — полный снэпшот сессии до/после (а не diff),
 * чтобы undo был простым и не зависел от порядка применения патчей.
 */
export interface SessionEdit {
  id: string
  sessionId: string
  editedAtUtc: number
  operation: SessionEditOperation
  previousState: WorkSession
  nextState: WorkSession
}

export type CreateSessionEditInput = Omit<SessionEdit, 'id' | 'editedAtUtc'>
