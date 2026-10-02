import type { TrackingStatus } from '../types/tracking'

export interface AppInfo {
  version: string
  /** `process.platform` в main, как обычная строка — shared-типы не должны зависеть от @types/node (используются и в renderer). */
  platform: string
  /**
   * Заглушка статуса трекера. Реального Activity Tracker в Итерации 0 нет —
   * см. ТЗ, Итерация 2. Значение всегда 'UNKNOWN', чтобы не выдавать
   * недостоверную информацию (FR-02.6).
   */
  trackingStatusStub: TrackingStatus
}

/** API, которое main-preload кладёт в `window.devlog` для main-window renderer. */
export interface MainWindowApi {
  getAppInfo: () => Promise<AppInfo>
}

/** API, которое overlay-preload кладёт в `window.devlog` для overlay renderer. */
export interface OverlayApi {
  getAppInfo: () => Promise<AppInfo>
  close: () => Promise<void>
}
