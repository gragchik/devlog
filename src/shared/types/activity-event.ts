import type { TrackingStatus } from './tracking'

/** Насколько надёжно определён исход наблюдения/сопоставления (FR-03.4, FR-04). */
export type Confidence = 'high' | 'medium' | 'low'

/**
 * Категория приложения, к которому относилось наблюдение. Конкретный набор
 * категорий (ide/terminal/excluded/...) определит Activity Tracker в
 * Итерации 2 — здесь намеренно не сужаем до enum, чтобы не фиксировать
 * прематурное решение.
 */
export type AppCategory = string

/**
 * Единичное пассивное наблюдение активности (FR-02, раздел 5 "activity_events").
 * Сырой слой, НЕ редактируется пользователем — источник истины для
 * Session Engine (Итерация 4). Заголовки окон, URL, текст и т.п. сюда не
 * попадают (FR-02.4) — хранится только то, что перечислено в полях ниже.
 */
export interface ActivityEvent {
  id: string
  timestampUtc: number
  processNameSanitized: string
  appCategory: AppCategory
  projectId: string | null
  branch: string | null
  detectedIssueKey: string | null
  idleSeconds: number | null
  state: TrackingStatus
  confidence: Confidence
  reason: string
  createdAtUtc: number
}

export type CreateActivityEventInput = Omit<ActivityEvent, 'id' | 'createdAtUtc'>
