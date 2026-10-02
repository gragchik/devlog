import { migration0001InitialSchema } from './0001-initial-schema'
import type { Migration } from './types'

/** Порядок — по возрастанию `version`. Применяются по порядку, без пропусков. */
export const MIGRATIONS: readonly Migration[] = [migration0001InitialSchema]

export type { Migration }
