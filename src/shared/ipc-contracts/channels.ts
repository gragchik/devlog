/**
 * Явный whitelist IPC-каналов между renderer (main-window/overlay) и main.
 * Любой канал, не перечисленный здесь, не регистрируется в `ipcMain` и не
 * проксируется через preload — renderer не получает доступ к произвольным
 * `ipcRenderer.invoke` (см. ТЗ, раздел 6: "не отдавать renderer общий ipcRenderer").
 *
 * Расширять по мере добавления итераций (getToday, updateSession, и т.д. — ТЗ раздел 6).
 */
export const IPC_CHANNELS = {
  /** Версия приложения, платформа и заглушка статуса трекера. Main-window + overlay. */
  APP_GET_INFO: 'app:get-info',
  /** Закрыть (скрыть) overlay-окно. Вызывается только из overlay. */
  OVERLAY_CLOSE: 'overlay:close'
} as const

export type IpcChannel = (typeof IPC_CHANNELS)[keyof typeof IPC_CHANNELS]
