import { getCurrentWindow } from '@tauri-apps/api/window'
import { invoke } from '@tauri-apps/api/core'
import { useEffect, useState } from 'react'
import type { AppInfo } from '@shared/types/app-info'

export function OverlayApp(): JSX.Element {
  const [appInfo, setAppInfo] = useState<AppInfo | null>(null)

  useEffect(() => {
    invoke<AppInfo>('get_app_info')
      .then(setAppInfo)
      .catch(() => setAppInfo(null))
  }, [])

  useEffect(() => {
    // FR-06.5: Escape закрывает overlay. Дублирует обработчик в Rust
    // (on_window_event не подписан на Escape напрямую — там перехватывается
    // только CloseRequested; реальная клавиша Escape обрабатывается здесь,
    // в renderer, через прямой вызов window.hide() по capability
    // `core:window:allow-hide`).
    function onKeyDown(event: KeyboardEvent): void {
      if (event.key === 'Escape') {
        void getCurrentWindow().hide()
      }
    }
    window.addEventListener('keydown', onKeyDown)
    return () => window.removeEventListener('keydown', onKeyDown)
  }, [])

  return (
    <div className="overlay-shell">
      <header className="overlay-header">
        <span className="overlay-title">DevLog</span>
        <span className="overlay-status">● {appInfo?.trackingStatusStub ?? 'UNKNOWN'} (заглушка)</span>
        <button
          type="button"
          className="overlay-close"
          onClick={() => void getCurrentWindow().hide()}
          aria-label="Закрыть"
        >
          ✕
        </button>
      </header>
      <div className="overlay-body">
        <p>Итерация 0 (Tauri): overlay-окно работает (hotkey, позиционирование, Escape, команда get_app_info).</p>
        <p>Сессии, задачи и редактирование появятся в Итерации 4/6.</p>
      </div>
    </div>
  )
}
