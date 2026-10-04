import { listen } from '@tauri-apps/api/event'
import { useCallback, useEffect, useState } from 'react'
import { isTab, TABS, type Navigate, type Tab } from './navigation'
import { Dashboard } from './pages/Dashboard'
import { SettingsPage } from './pages/SettingsPage'
import { Timeline } from './pages/Timeline'
import { WorklogReview } from './pages/WorklogReview'

interface Route {
  tab: Tab
  /** День, на котором открыть вкладку (Timeline/Worklog). */
  localDate?: string
  /** Меняется на каждый переход — пересоздаёт страницу с новой датой. */
  nonce: number
}

export function App(): JSX.Element {
  const [route, setRoute] = useState<Route>({ tab: 'dashboard', nonce: 0 })

  const navigate: Navigate = useCallback((tab, localDate) => {
    setRoute((prev) => ({ tab, localDate, nonce: prev.nonce + 1 }))
  }, [])

  // Overlay просит открыть конкретную вкладку («Отчёт» → Worklog), см.
  // `commands/windows.rs::show_main_window_command`.
  useEffect(() => {
    const unlistenPromise = listen<string>('main:navigate', (event) => {
      if (isTab(event.payload)) navigate(event.payload)
    })
    return () => {
      void unlistenPromise.then((unlisten) => unlisten())
    }
  }, [navigate])

  const { tab, localDate, nonce } = route

  return (
    <div className="app-shell">
      <nav className="app-nav">
        <span className="app-nav-brand">DevLog</span>
        {TABS.map((t) => (
          <button key={t.id} type="button" className={tab === t.id ? 'active' : ''} onClick={() => navigate(t.id)}>
            {t.label}
          </button>
        ))}
      </nav>
      <main className="app-content">
        {tab === 'dashboard' && <Dashboard navigate={navigate} />}
        {tab === 'timeline' && <Timeline key={nonce} initialDate={localDate} />}
        {tab === 'worklog' && <WorklogReview key={nonce} initialDate={localDate} navigate={navigate} />}
        {tab === 'settings' && <SettingsPage />}
      </main>
    </div>
  )
}
