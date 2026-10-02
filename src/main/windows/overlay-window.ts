import { BrowserWindow, screen } from 'electron'
import { join } from 'node:path'

const OVERLAY_WIDTH = 520
const OVERLAY_HEIGHT = 380
const SCREEN_EDGE_MARGIN = 16

let overlayWindow: BrowserWindow | null = null

export function getOverlayWindow(): BrowserWindow | null {
  return overlayWindow
}

/**
 * Позиционирует overlay у правого верхнего края дисплея, ближайшего к курсору,
 * строго в границах его `workArea` (учитывает taskbar и DPI-масштабирование,
 * т.к. `workArea` приходит от OS уже в скейленных координатах — ТЗ FR-06.3).
 */
function positionNearCursor(win: BrowserWindow): void {
  const display = screen.getDisplayNearestPoint(screen.getCursorScreenPoint())
  const { x: areaX, y: areaY, width: areaWidth, height: areaHeight } = display.workArea

  const width = Math.min(OVERLAY_WIDTH, areaWidth - SCREEN_EDGE_MARGIN * 2)
  const height = Math.min(OVERLAY_HEIGHT, areaHeight - SCREEN_EDGE_MARGIN * 2)

  const x = Math.round(areaX + areaWidth - width - SCREEN_EDGE_MARGIN)
  const y = Math.round(areaY + SCREEN_EDGE_MARGIN)

  win.setBounds({ x, y, width: Math.round(width), height: Math.round(height) })
}

export function createOverlayWindow(): BrowserWindow {
  if (overlayWindow && !overlayWindow.isDestroyed()) {
    return overlayWindow
  }

  const win = new BrowserWindow({
    width: OVERLAY_WIDTH,
    height: OVERLAY_HEIGHT,
    show: false,
    frame: false,
    alwaysOnTop: true,
    skipTaskbar: true,
    resizable: false,
    fullscreenable: false,
    movable: false,
    webPreferences: {
      preload: join(__dirname, '../preload/overlay-preload.mjs'),
      contextIsolation: true,
      nodeIntegration: false,
      sandbox: true
    }
  })

  // FR-06.5: Escape всегда закрывает overlay, независимо от фокуса/состояния
  // renderer-кода — защита на уровне main в дополнение к keydown-обработчику в React.
  win.webContents.on('before-input-event', (_event, input) => {
    if (input.type === 'keyDown' && input.key === 'Escape') {
      hideOverlayWindow()
    }
  })

  win.on('close', (event) => {
    // Overlay никогда не уничтожается сам по себе — только скрывается, чтобы
    // повторное открытие было быстрым (целевой p95 ~500мс, ТЗ FR-06.4).
    event.preventDefault()
    win.hide()
  })

  if (process.env['ELECTRON_RENDERER_URL']) {
    void win.loadURL(`${process.env['ELECTRON_RENDERER_URL']}/overlay/index.html`)
  } else {
    void win.loadFile(join(__dirname, '../renderer/overlay/index.html'))
  }

  overlayWindow = win
  return win
}

export function showOverlayWindow(): void {
  const win = createOverlayWindow()
  positionNearCursor(win)
  win.show()
  win.focus()
}

export function hideOverlayWindow(): void {
  overlayWindow?.hide()
}

export function toggleOverlayWindow(): void {
  const win = createOverlayWindow()
  if (win.isVisible()) {
    hideOverlayWindow()
  } else {
    showOverlayWindow()
  }
}

/** Для `app.quit()` — overlay перехватывает `close`, поэтому это нужно вызывать явно. */
export function destroyOverlayWindow(): void {
  if (overlayWindow && !overlayWindow.isDestroyed()) {
    overlayWindow.destroy()
  }
  overlayWindow = null
}
