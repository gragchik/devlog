import { invoke } from '@tauri-apps/api/core'
import { useEffect, useState } from 'react'
import type { AppInfo } from '@shared/types/app-info'

export function App(): JSX.Element {
  const [appInfo, setAppInfo] = useState<AppInfo | null>(null)
  const [error, setError] = useState<string | null>(null)

  useEffect(() => {
    invoke<AppInfo>('get_app_info')
      .then(setAppInfo)
      .catch((err: unknown) => setError(err instanceof Error ? err.message : String(err)))
  }, [])

  return (
    <main className="app-shell">
      <h1>DevLog</h1>
      <p className="subtitle">Итерация 0 (Tauri) — каркас приложения. Activity Tracker пока не реализован.</p>

      <section className="card">
        <h2>Статус</h2>
        {error && <p className="error">Ошибка команды: {error}</p>}
        {!error && !appInfo && <p>Загрузка…</p>}
        {appInfo && (
          <dl>
            <dt>Версия</dt>
            <dd>{appInfo.version}</dd>
            <dt>Платформа</dt>
            <dd>{appInfo.platform}</dd>
            <dt>Tracking status</dt>
            <dd>{appInfo.trackingStatusStub} (заглушка, см. Итерацию 2/4)</dd>
          </dl>
        )}
      </section>

      <section className="card">
        <h2>Overlay</h2>
        <p>
          Нажмите <kbd>Ctrl</kbd>+<kbd>Alt</kbd>+<kbd>W</kbd> или выберите «Показать overlay» в трее, чтобы открыть
          overlay-окно поверх других приложений.
        </p>
      </section>
    </main>
  )
}
