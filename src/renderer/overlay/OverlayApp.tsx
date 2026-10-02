import { useEffect, useState } from 'react'
import type { AppInfo, OverlayApi } from '@shared/ipc-contracts/types'

const devlogApi = (window as unknown as { devlog: OverlayApi }).devlog

export function OverlayApp(): JSX.Element {
  const [appInfo, setAppInfo] = useState<AppInfo | null>(null)

  useEffect(() => {
    devlogApi.getAppInfo().then(setAppInfo).catch(() => setAppInfo(null))
  }, [])

  useEffect(() => {
    // FR-06.5: Escape закрывает overlay. Дублирует обработчик в main
    // (before-input-event) — здесь на случай, если фокус внутри renderer
    // и для немедленной реакции без похода в main.
    function onKeyDown(event: KeyboardEvent): void {
      if (event.key === 'Escape') {
        void devlogApi.close()
      }
    }
    window.addEventListener('keydown', onKeyDown)
    return () => window.removeEventListener('keydown', onKeyDown)
  }, [])

  return (
    <div className="overlay-shell">
      <header className="overlay-header">
        <span className="overlay-title">DevLog</span>
        <span className="overlay-status">
          ● {appInfo?.trackingStatusStub ?? 'UNKNOWN'} (заглушка)
        </span>
        <button type="button" className="overlay-close" onClick={() => void devlogApi.close()} aria-label="Закрыть">
          ✕
        </button>
      </header>
      <div className="overlay-body">
        <p>Итерация 0: overlay-окно работает (hotkey, позиционирование, Escape, IPC).</p>
        <p>Сессии, задачи и редактирование появятся в Итерации 4/6.</p>
      </div>
    </div>
  )
}
