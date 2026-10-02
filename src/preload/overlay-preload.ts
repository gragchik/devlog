import { contextBridge, ipcRenderer } from 'electron'
import { IPC_CHANNELS } from '@shared/ipc-contracts/channels'
import type { AppInfo, OverlayApi } from '@shared/ipc-contracts/types'

/**
 * Единственная точка входа renderer → main для overlay-окна. Отдельный
 * preload от главного окна — overlay не получает доступ к методам главного
 * окна и наоборот (ТЗ, раздел 6).
 */
const api: OverlayApi = {
  getAppInfo: () => ipcRenderer.invoke(IPC_CHANNELS.APP_GET_INFO) as Promise<AppInfo>,
  close: () => ipcRenderer.invoke(IPC_CHANNELS.OVERLAY_CLOSE) as Promise<void>
}

contextBridge.exposeInMainWorld('devlog', api)
