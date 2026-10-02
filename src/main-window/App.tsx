import { useState } from 'react'
import { Dashboard } from './pages/Dashboard'
import { SettingsPage } from './pages/SettingsPage'
import { Timeline } from './pages/Timeline'

type Tab = 'dashboard' | 'timeline' | 'settings'

const TABS: { id: Tab; label: string }[] = [
  { id: 'dashboard', label: 'Dashboard' },
  { id: 'timeline', label: 'Timeline' },
  { id: 'settings', label: 'Settings' }
]

export function App(): JSX.Element {
  const [tab, setTab] = useState<Tab>('dashboard')

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
        {tab === 'settings' && <SettingsPage />}
      </main>
    </div>
  )
}
