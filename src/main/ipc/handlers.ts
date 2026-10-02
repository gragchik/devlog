import { app, ipcMain, type IpcMainInvokeEvent } from 'electron'
import { IPC_CHANNELS } from '@shared/ipc-contracts/channels'
import type { AppInfo } from '@shared/ipc-contracts/types'
import { getMainWindow } from '../windows/main-window'
import { getOverlayWindow, hideOverlayWindow } from '../windows/overlay-window'

/**
 * Итерация 0: реального Activity Tracker нет, статус — всегда `UNKNOWN`,
 * чтобы не выдавать недостоверные данные (FR-02.6). Заменится в Итерации 2/4
 * на чтение текущего состояния `session-engine`.
 */
function getTrackingStatusStub(): AppInfo['trackingStatusStub'] {
  return 'UNKNOWN'
}

/** Разрешаем вызов только из окон, которые приложение само создало. */
function isTrustedSender(event: IpcMainInvokeEvent): boolean {
  const frame = event.senderFrame
  if (!frame) return false
  return (
    frame === getMainWindow()?.webContents.mainFrame || frame === getOverlayWindow()?.webContents.mainFrame
  )
}

function isOverlaySender(event: IpcMainInvokeEvent): boolean {
  const frame = event.senderFrame
  return !!frame && frame === getOverlayWindow()?.webContents.mainFrame
}

export function registerIpcHandlers(): void {
  ipcMain.handle(IPC_CHANNELS.APP_GET_INFO, (event): AppInfo => {
    if (!isTrustedSender(event)) {
      throw new Error('Untrusted IPC sender for app:get-info')
    }
    return {
      version: app.getVersion(),
      platform: process.platform,
      trackingStatusStub: getTrackingStatusStub()
    }
  })

  ipcMain.handle(IPC_CHANNELS.OVERLAY_CLOSE, (event): void => {
    if (!isOverlaySender(event)) {
      throw new Error('Untrusted IPC sender for overlay:close')
    }
    hideOverlayWindow()
  })
}
