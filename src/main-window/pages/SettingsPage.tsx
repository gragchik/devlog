import { useEffect, useState } from 'react'
import type { JiraConnectionStatus } from '@shared/types/jira'
import type { Project } from '@shared/types/project'
import { TEMPLATE_PLACEHOLDERS, type CommentTemplate, type TemplateSettings } from '@shared/types/worklog'
import type { TrackerThresholds, WhitelistEntry } from '@shared/types/settings'
import { api, errorMessage } from '@shared/api'
import { DataSection, DiagnosticsSection } from '../components/DataSections'

function ProjectsSection(): JSX.Element {
  const [projects, setProjects] = useState<Project[]>([])
  const [name, setName] = useState('')
  const [repoPath, setRepoPath] = useState('')
  const [error, setError] = useState<string | null>(null)

  function reload(): void {
    api.listProjects().then(setProjects).catch((err: unknown) => setError(errorMessage(err)))
  }

  useEffect(reload, [])

  async function add(): Promise<void> {
    if (!name.trim() || !repoPath.trim()) return
    try {
      await api.addProject({ name: name.trim(), repoPath: repoPath.trim() })
      setName('')
      setRepoPath('')
      setError(null)
      reload()
    } catch (err) {
      setError(errorMessage(err))
    }
  }

  async function toggleEnabled(project: Project): Promise<void> {
    await api.updateProject(project.id, { enabled: !project.enabled })
    reload()
  }

  async function remove(project: Project): Promise<void> {
    await api.removeProject(project.id)
    reload()
  }

  return (
    <section className="card">
      <h2>Репозитории</h2>
      <p className="muted">
        Локальные папки Git-репозиториев, которые трекер проверяет на текущую ветку (FR-03). Issue key извлекается из
        имени ветки по умолчанию по паттерну <code>{'\\b[A-Z][A-Z0-9]+-\\d+\\b'}</code>.
      </p>
      {error && <p className="error">{error}</p>}
      <ul className="settings-list">
        {projects.map((p) => (
          <li key={p.id}>
            <label>
              <input type="checkbox" checked={p.enabled} onChange={() => void toggleEnabled(p)} /> {p.name}
            </label>
            <span className="muted">{p.repoPath}</span>
            <button type="button" onClick={() => void remove(p)}>
              Удалить
            </button>
          </li>
        ))}
      </ul>
      <div className="settings-add-row">
        <input placeholder="Название" value={name} onChange={(e) => setName(e.target.value)} />
        <input placeholder="D:/путь/к/репозиторию" value={repoPath} onChange={(e) => setRepoPath(e.target.value)} />
        <button type="button" onClick={add}>
          Добавить
        </button>
      </div>
    </section>
  )
}

function WhitelistSection(): JSX.Element {
  const [entries, setEntries] = useState<WhitelistEntry[]>([])
  const [processName, setProcessName] = useState('')
  const [category, setCategory] = useState('ide')
  const [error, setError] = useState<string | null>(null)
  const [saved, setSaved] = useState(false)

  function reload(): void {
    api.getWhitelist().then(setEntries).catch((err: unknown) => setError(errorMessage(err)))
  }

  useEffect(reload, [])

  async function persist(next: WhitelistEntry[]): Promise<void> {
    try {
      await api.setWhitelist(next)
      setEntries(next)
      setSaved(true)
      setTimeout(() => setSaved(false), 2000)
    } catch (err) {
      setError(errorMessage(err))
    }
  }

  function add(): void {
    if (!processName.trim()) return
    void persist([...entries, { processName: processName.trim(), category }])
    setProcessName('')
  }

  function remove(index: number): void {
    void persist(entries.filter((_, i) => i !== index))
  }

  return (
    <section className="card">
      <h2>Учитываемые приложения</h2>
      <p className="muted">
        FR-02.3: только процессы из этого списка считаются рабочим временем. Остальные попадают в «Исключено».
        Изменения вступят в силу <strong>после перезапуска приложения</strong> (трекер читает список один раз при
        старте).
      </p>
      {error && <p className="error">{error}</p>}
      {saved && <p className="muted">Сохранено.</p>}
      <ul className="settings-list">
        {entries.map((e, i) => (
          <li key={`${e.processName}-${i}`}>
            <code>{e.processName}</code>
            <span className="muted">{e.category}</span>
            <button type="button" onClick={() => remove(i)}>
              Удалить
            </button>
          </li>
        ))}
      </ul>
      <div className="settings-add-row">
        <input placeholder="webstorm64.exe" value={processName} onChange={(e) => setProcessName(e.target.value)} />
        <input placeholder="категория (ide/terminal/…)" value={category} onChange={(e) => setCategory(e.target.value)} />
        <button type="button" onClick={add}>
          Добавить
        </button>
      </div>
    </section>
  )
}

function ThresholdsSection(): JSX.Element {
  const [thresholds, setThresholds] = useState<TrackerThresholds | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [saved, setSaved] = useState(false)

  useEffect(() => {
    api.getTrackerThresholds().then(setThresholds).catch((err: unknown) => setError(errorMessage(err)))
  }, [])

  async function save(): Promise<void> {
    if (!thresholds) return
    try {
      await api.setTrackerThresholds(thresholds)
      setSaved(true)
      setTimeout(() => setSaved(false), 2000)
    } catch (err) {
      setError(errorMessage(err))
    }
  }

  return (
    <section className="card">
      <h2>Пороги трекера</h2>
      {error && <p className="error">{error}</p>}
      {saved && <p className="muted">Сохранено.</p>}
      {thresholds && (
        <div className="settings-add-row">
          <label>
            Idle-порог (сек)
            <input
              type="number"
              min={1}
              value={thresholds.idleThresholdSeconds}
              onChange={(e) => setThresholds({ ...thresholds, idleThresholdSeconds: Number(e.target.value) })}
            />
          </label>
          <label>
            Интервал опроса (сек)
            <input
              type="number"
              min={1}
              value={thresholds.pollIntervalSeconds}
              onChange={(e) => setThresholds({ ...thresholds, pollIntervalSeconds: Number(e.target.value) })}
            />
          </label>
          <button type="button" onClick={save}>
            Сохранить
          </button>
        </div>
      )}
      <p className="muted">Требуется перезапуск приложения, чтобы вступить в силу.</p>
    </section>
  )
}

function AutostartSection(): JSX.Element {
  const [enabled, setEnabled] = useState<boolean | null>(null)
  const [error, setError] = useState<string | null>(null)

  useEffect(() => {
    api.getAutostartEnabled().then(setEnabled).catch((err: unknown) => setError(errorMessage(err)))
  }, [])

  async function toggle(): Promise<void> {
    if (enabled === null) return
    try {
      await api.setAutostartEnabled(!enabled)
      setEnabled(!enabled)
    } catch (err) {
      setError(errorMessage(err))
    }
  }

  return (
    <section className="card">
      <h2>Автозапуск</h2>
      <p className="muted">FR-01.3: запуск вместе с Windows — выключен по умолчанию.</p>
      {error && <p className="error">{error}</p>}
      <label>
        <input type="checkbox" checked={enabled ?? false} disabled={enabled === null} onChange={toggle} /> Запускать
        DevLog при входе в Windows
      </label>
    </section>
  )
}

function ShortcutSection(): JSX.Element {
  const [current, setCurrent] = useState<string | null>(null)
  const [draft, setDraft] = useState('')
  const [error, setError] = useState<string | null>(null)
  const [saved, setSaved] = useState(false)

  useEffect(() => {
    api.getOverlayShortcut().then((s) => {
      setCurrent(s)
      setDraft(s)
    }).catch((err: unknown) => setError(errorMessage(err)))
  }, [])

  async function save(): Promise<void> {
    if (!draft.trim()) return
    try {
      await api.setOverlayShortcut(draft.trim())
      setCurrent(draft.trim())
      setError(null)
      setSaved(true)
      setTimeout(() => setSaved(false), 2000)
    } catch (err) {
      // FR-06.2: конфликт с другим приложением — показываем ошибку, не сохраняем.
      setError(errorMessage(err))
    }
  }

  return (
    <section className="card">
      <h2>Горячая клавиша overlay</h2>
      <p className="muted">
        FR-06.2: формат — например <code>Ctrl+Alt+W</code>, <code>Alt+Space</code>. Применяется сразу, без
        перезапуска. Если комбинация занята другим приложением — сохранение не пройдёт, текущая останется активной.
      </p>
      {error && <p className="error">{error}</p>}
      {saved && <p className="muted">Сохранено и применено.</p>}
      <div className="settings-add-row">
        <input value={draft} onChange={(e) => setDraft(e.target.value)} placeholder="Ctrl+Alt+W" />
        <button type="button" onClick={save} disabled={draft.trim() === current}>
          Сохранить
        </button>
      </div>
    </section>
  )
}

function JiraSection(): JSX.Element {
  const [status, setStatus] = useState<JiraConnectionStatus | null>(null)
  const [baseUrl, setBaseUrl] = useState('')
  const [email, setEmail] = useState('')
  const [apiToken, setApiToken] = useState('')
  const [error, setError] = useState<string | null>(null)
  const [info, setInfo] = useState<string | null>(null)
  const [busy, setBusy] = useState(false)

  useEffect(() => {
    api
      .jiraGetConnectionStatus()
      .then((s) => {
        setStatus(s)
        setBaseUrl(s.baseUrl ?? '')
        setEmail(s.email ?? '')
      })
      .catch((err: unknown) => setError(errorMessage(err)))
  }, [])

  async function run(action: () => Promise<string>): Promise<void> {
    setBusy(true)
    setInfo(null)
    try {
      setInfo(await action())
      setError(null)
    } catch (err) {
      setError(errorMessage(err))
    } finally {
      setBusy(false)
    }
  }

  const saveAndTest = (): Promise<void> =>
    run(async () => {
      setStatus(await api.jiraSaveConnection({ baseUrl, email, apiToken }))
      // Токен сразу забываем на стороне UI — он уже в Windows Credential Manager.
      setApiToken('')
      const user = await api.jiraTestConnection()
      return `Сохранено. Подключено как ${user.displayName}.`
    })

  const test = (): Promise<void> =>
    run(async () => `Соединение работает: ${(await api.jiraTestConnection()).displayName}.`)

  const clear = (): Promise<void> =>
    run(async () => {
      await api.jiraClearConnection()
      setStatus({ configured: false, baseUrl: null, email: null })
      setBaseUrl('')
      setEmail('')
      return 'Подключение удалено, токен стёрт из Windows Credential Manager.'
    })

  return (
    <section className="card">
      <h2>Jira</h2>
      <p className="muted">
        Jira Cloud (FR-07): адрес вида <code>https://company.atlassian.net</code>, email и API token (создаётся в
        id.atlassian.com → Security → API tokens). Токен хранится в Windows Credential Manager, а не в базе. Используйте
        Jira API только если это разрешено правилами вашей организации.
      </p>
      {status?.configured && <p className="muted">Подключение сохранено. Оставьте поле токена пустым, чтобы не менять его.</p>}
      {error && <p className="error">{error}</p>}
      {info && <p className="muted">{info}</p>}
      <div className="settings-add-row">
        <input placeholder="https://company.atlassian.net" value={baseUrl} onChange={(e) => setBaseUrl(e.target.value)} />
        <input placeholder="email" value={email} onChange={(e) => setEmail(e.target.value)} />
        <input
          type="password"
          autoComplete="off"
          placeholder={status?.configured ? 'токен сохранён' : 'API token'}
          value={apiToken}
          onChange={(e) => setApiToken(e.target.value)}
        />
        <button type="button" onClick={() => void saveAndTest()} disabled={busy || !baseUrl.trim() || !email.trim()}>
          Сохранить и проверить
        </button>
        {status?.configured && (
          <>
            <button type="button" onClick={() => void test()} disabled={busy}>
              Проверить
            </button>
            <button type="button" onClick={() => void clear()} disabled={busy}>
              Отключить
            </button>
          </>
        )}
      </div>
    </section>
  )
}

function TemplatesSection(): JSX.Element {
  const [value, setValue] = useState<TemplateSettings | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [saved, setSaved] = useState(false)

  useEffect(() => {
    api.worklogGetTemplates().then(setValue).catch((err: unknown) => setError(errorMessage(err)))
  }, [])

  if (!value) {
    return (
      <section className="card">
        <h2>Шаблоны комментариев</h2>
        {error && <p className="error">{error}</p>}
      </section>
    )
  }

  function update(index: number, patch: Partial<CommentTemplate>): void {
    if (!value) return
    setValue({ ...value, templates: value.templates.map((t, i) => (i === index ? { ...t, ...patch } : t)) })
  }

  function remove(index: number): void {
    if (!value) return
    setValue({ ...value, templates: value.templates.filter((_, i) => i !== index) })
  }

  function add(): void {
    if (!value) return
    setValue({ ...value, templates: [...value.templates, { id: '', name: 'Новый шаблон', text: 'Работа над задачей {issueKey}' }] })
  }

  async function save(): Promise<void> {
    if (!value) return
    try {
      setValue(await api.worklogSaveTemplates(value))
      setError(null)
      setSaved(true)
      setTimeout(() => setSaved(false), 2000)
    } catch (err) {
      setError(errorMessage(err))
    }
  }

  return (
    <section className="card">
      <h2>Шаблоны комментариев</h2>
      <p className="muted">
        FR-08: текст worklog собирается только из известных данных. Поля:{' '}
        {TEMPLATE_PLACEHOLDERS.map((p, i) => (
          <span key={p.name}>
            {i > 0 && ', '}
            <code>{`{${p.name}}`}</code> — {p.hint}
          </span>
        ))}
        . Пустые поля убираются вместе с лишней пунктуацией. Отмеченный шаблон используется для новых черновиков.
      </p>
      {error && <p className="error">{error}</p>}
      {saved && <p className="muted">Сохранено.</p>}
      <ul className="settings-list template-list">
        {value.templates.map((t, i) => (
          <li key={t.id || `new-${i}`}>
            <input
              type="radio"
              name="default-template"
              title="По умолчанию"
              checked={t.id !== '' && t.id === value.defaultTemplateId}
              disabled={t.id === ''}
              onChange={() => setValue({ ...value, defaultTemplateId: t.id })}
            />
            <input className="template-name" value={t.name} onChange={(e) => update(i, { name: e.target.value })} />
            <textarea rows={2} value={t.text} onChange={(e) => update(i, { text: e.target.value })} />
            <button type="button" onClick={() => remove(i)} disabled={t.id === value.defaultTemplateId}>
              Удалить
            </button>
          </li>
        ))}
      </ul>
      <div className="settings-add-row">
        <button type="button" onClick={add}>
          + Шаблон
        </button>
        <button type="button" onClick={() => void save()}>
          Сохранить
        </button>
      </div>
    </section>
  )
}

export function SettingsPage(): JSX.Element {
  return (
    <div className="page">
      <h1>Settings</h1>
      <ProjectsSection />
      <WhitelistSection />
      <ThresholdsSection />
      <AutostartSection />
      <ShortcutSection />
      <JiraSection />
      <TemplatesSection />
      <DataSection />
      <DiagnosticsSection />
    </div>
  )
}
