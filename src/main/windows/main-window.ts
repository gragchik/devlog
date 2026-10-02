import { BrowserWindow, shell } from 'electron'
import { join } from 'node:path'

let mainWindow: BrowserWindow | null = null
let appIsQuitting = false

/** Вызывается из `index.ts` перед `app.quit()`, чтобы окно закрылось, а не скрылось. */
export function markAppQuitting(): void {
  appIsQuitting = true
}

export function getMainWindow(): BrowserWindow | null {
  return mainWindow
}

export function createMainWindow(): BrowserWindow {
  if (mainWindow && !mainWindow.isDestroyed()) {
    return mainWindow
  }

  const win = new BrowserWindow({
    width: 1100,
    height: 720,
    minWidth: 760,
    minHeight: 480,
    show: false,
    autoHideMenuBar: true,
    webPreferences: {
      preload: join(__dirname, '../preload/main-preload.mjs'),
      contextIsolation: true,
      nodeIntegration: false,
      sandbox: true
    }
  })

  win.on('ready-to-show', () => win.show())

  // FR-01.1: закрытие главного окна не завершает слежение — просто скрываем,
  // процесс продолжает жить в трее. Полностью закрываем только при выходе из приложения.
  win.on('close', (event) => {
    if (!appIsQuitting) {
      event.preventDefault()
      win.hide()
    }
  })

  win.on('closed', () => {
    mainWindow = null
  })

  // Открывать внешние ссылки в системном браузере, а не новым Electron-окном.
  win.webContents.setWindowOpenHandler(({ url }) => {
    void shell.openExternal(url)
    return { action: 'deny' }
  })

  if (process.env['ELECTRON_RENDERER_URL']) {
    void win.loadURL(`${process.env['ELECTRON_RENDERER_URL']}/main-window/index.html`)
  } else {
    void win.loadFile(join(__dirname, '../renderer/main-window/index.html'))
  }

  mainWindow = win
  return win
}

export function showMainWindow(): void {
  const win = createMainWindow()
  if (win.isMinimized()) win.restore()
  win.show()
  win.focus()
}
