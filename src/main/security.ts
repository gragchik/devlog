import { app, session } from 'electron'

/**
 * Глобальные security-меры верхнего уровня (ТЗ, раздел 7):
 * - запрет создания новых окон/навигации из web content (например, по клику
 *   на внешнюю ссылку в будущем UI) — открываем такие ссылки в системном
 *   браузере, а не внутри Electron;
 * - строгий CSP по умолчанию для всех ответов.
 *
 * Настройки конкретных BrowserWindow (contextIsolation, sandbox,
 * nodeIntegration=false) задаются при создании каждого окна —
 * см. `windows/main-window.ts` и `windows/overlay-window.ts`.
 */
export function hardenAppSecurity(): void {
  app.on('web-contents-created', (_event, contents) => {
    contents.setWindowOpenHandler(() => ({ action: 'deny' }))
    contents.on('will-navigate', (navigationEvent, url) => {
      if (!url.startsWith('file://') && !url.startsWith('http://localhost')) {
        navigationEvent.preventDefault()
      }
    })
  })

  session.defaultSession.webRequest.onHeadersReceived((details, callback) => {
    callback({
      responseHeaders: {
        ...details.responseHeaders,
        'Content-Security-Policy': [
          "default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' data:"
        ]
      }
    })
  })
}
