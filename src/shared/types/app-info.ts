/**
 * Зеркалит `src-tauri/src/commands/app_info.rs`. Tauri не даёт сквозной
 * типизации Rust↔TS из коробки (в отличие от нашего прежнего Electron IPC
 * слоя, где shared-типы импортировались в main и renderer из одного
 * файла) — здесь типы синхронизируются вручную. Если расхождение станет
 * проблемой, рассмотреть `tauri-specta`/`ts-rs` для codegen (не делаем
 * этого сейчас — Итерация 0, не усложняем раньше необходимости).
 */
export type TrackingStatus = 'TRACKING' | 'IDLE' | 'PAUSED' | 'LOCKED' | 'SUSPENDED' | 'UNKNOWN'

export interface AppInfo {
  version: string
  platform: string
  trackingStatusStub: TrackingStatus
}
