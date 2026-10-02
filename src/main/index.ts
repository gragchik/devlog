import { BrowserWindow, app } from 'electron'
import { registerIpcHandlers } from './ipc/handlers'
import { hardenAppSecurity } from './security'
import { registerGlobalShortcuts, unregisterGlobalShortcuts } from './shortcuts/shortcuts'
import { destroyTray, createTray } from './tray/tray'
import { createMainWindow, markAppQuitting, showMainWindow } from './windows/main-window'
import { createOverlayWindow, destroyOverlayWindow } from './windows/overlay-window'

// FR-01.3: только один экземпляр приложения.
const gotSingleInstanceLock = app.requestSingleInstanceLock()

if (!gotSingleInstanceLock) {
  app.quit()
} else {
  app.on('second-instance', () => {
    showMainWindow()
  })

  app.whenReady().then(() => {
    hardenAppSecurity()
    registerIpcHandlers()
    createMainWindow()
    createOverlayWindow()
    createTray()
    registerGlobalShortcuts()
  })

  // FR-01.1: главное окно закрывается скрытием (см. main-window.ts), поэтому
  // "все окна закрыты" на Windows/Linux практически не наступает, пока жив
  // overlay. Обработчик оставлен пустым намеренно — приложение продолжает
  // жить в трее.
  app.on('window-all-closed', () => {})

  app.on('activate', () => {
    if (BrowserWindow.getAllWindows().length === 0) {
      createMainWindow()
    }
  })

  app.on('before-quit', () => {
    markAppQuitting()
    unregisterGlobalShortcuts()
    destroyOverlayWindow()
    destroyTray()
  })
}
