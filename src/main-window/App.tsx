import { listen } from '@tauri-apps/api/event'
import { useEffect, useState } from 'react'
import { Dashboard } from './pages/Dashboard'
import { SettingsPage } from './pages/SettingsPage'
import { Timeline } from './pages/Timeline'
import { WorklogReview } from './pages/WorklogReview'

type Tab = 'dashboard' | 'timeline' | 'worklog' | 'settings'

const TABS: { id: Tab; label: string }[] = [
  { id: 'dashboard', label: 'Dashboard' },
  { id: 'timeline', label: 'Timeline' },
  { id: 'worklog', label: 'Worklog' },
  { id: 'settings', label: 'Settings' }
]

function isTab(value: string): value is Tab {
  return TABS.some((t) => t.id === value)
}

export function App(): JSX.Element {
  const [tab, setTab] = useState<Tab>('dashboard')

  // Overlay просит открыть конкретную вкладку («Отчёт» → Worklog), см.
  // `commands/windows.rs::show_main_window_command`.
  useEffect(() => {
    const unlistenPromise = listen<string>('main:navigate', (event) => {
      if (isTab(event.payload)) setTab(event.payload)
    })
    return () => {
      void unlistenPromise.then((unlisten) => unlisten())
    }
  }, [])

  return (
    <div className="app-shell">
      <nav className="app-nav">
        <span className="app-nav-brand">DevLog</span>
        {TABS.map((t) => (
          <button key={t.id} type="button" className={tab === t.id ? 'active' : ''} onClick={() => setTab(t.id)}>
            {t.label}
          </button>
        ))}
      </nav>
      <main className="app-content">
        {tab === 'dashboard' && <Dashboard />}
        {tab === 'timeline' && <Timeline />}
        {tab === 'worklog' && <WorklogReview />}
        {tab === 'settings' && <SettingsPage />}
      </main>
    </div>
  )
}
