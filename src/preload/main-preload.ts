import { contextBridge, ipcRenderer } from 'electron'
import { IPC_CHANNELS } from '@shared/ipc-contracts/channels'
import type { AppInfo, MainWindowApi } from '@shared/ipc-contracts/types'

/**
 * Единственная точка входа renderer → main для главного окна. Никакого
 * общего `ipcRenderer` не отдаём — только эти явные типизированные методы
 * (ТЗ, раздел 6).
 */
const api: MainWindowApi = {
  getAppInfo: () => ipcRenderer.invoke(IPC_CHANNELS.APP_GET_INFO) as Promise<AppInfo>
}

contextBridge.exposeInMainWorld('devlog', api)
