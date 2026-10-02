import { Notification, globalShortcut } from 'electron'
import { toggleOverlayWindow } from '../windows/overlay-window'

/** Можно сделать настраиваемым в Settings — см. Итерацию 5/6. */
export const DEFAULT_OVERLAY_SHORTCUT = 'Control+Alt+W'

/**
 * Регистрирует глобальный хоткей показа/скрытия overlay. Если комбинация уже
 * занята другим приложением, `register` вернёт `false` — показываем
 * нативное уведомление вместо тихого отказа (FR-06.2). Полноценный UI для
 * переназначения хоткея — Итерация 6.
 */
export function registerGlobalShortcuts(): void {
  const ok = globalShortcut.register(DEFAULT_OVERLAY_SHORTCUT, () => {
    toggleOverlayWindow()
  })

  if (!ok) {
    console.warn(`[shortcuts] Не удалось зарегистрировать ${DEFAULT_OVERLAY_SHORTCUT} — конфликт с другим приложением`)
    if (Notification.isSupported()) {
      new Notification({
        title: 'DevLog',
        body: `Горячая клавиша ${DEFAULT_OVERLAY_SHORTCUT} уже занята другим приложением. Overlay можно открыть через трей.`
      }).show()
    }
  }
}

export function unregisterGlobalShortcuts(): void {
  globalShortcut.unregisterAll()
}
