import { Menu, Tray, app, nativeImage } from 'electron'
import { join } from 'node:path'
import { showMainWindow } from '../windows/main-window'
import { toggleOverlayWindow } from '../windows/overlay-window'

let tray: Tray | null = null
/**
 * Заглушка на Итерацию 0: реальной паузы трекера ещё нет (появится в
 * Итерации 2). Переключатель здесь только проверяет механику меню трея и
 * не должен восприниматься как настоящий Pause/Resume (FR-01.2/FR-01.4).
 */
let pausedStub = false

function buildMenu(): Menu {
  return Menu.buildFromTemplate([
    { label: 'Открыть', click: () => showMainWindow() },
    { label: 'Показать overlay', click: () => toggleOverlayWindow() },
    {
      label: pausedStub ? 'Продолжить (заглушка)' : 'Приостановить (заглушка)',
      click: () => {
        pausedStub = !pausedStub
        updateTrayState()
      }
    },
    { label: 'Сегодня', click: () => showMainWindow() },
    { type: 'separator' },
    {
      label: 'Выход',
      click: () => app.quit()
    }
  ])
}

function updateTrayState(): void {
  if (!tray) return
  tray.setToolTip(`DevLog — ${pausedStub ? 'Paused (заглушка)' : 'Tracking status: Unknown (итерация 0)'}`)
  tray.setContextMenu(buildMenu())
}

export function createTray(): Tray {
  if (tray) return tray

  const iconPath = join(__dirname, '../../resources/tray.png')
  const icon = nativeImage.createFromPath(iconPath)
  tray = new Tray(icon.isEmpty() ? nativeImage.createEmpty() : icon)
  updateTrayState()
  return tray
}

export function destroyTray(): void {
  tray?.destroy()
  tray = null
}
