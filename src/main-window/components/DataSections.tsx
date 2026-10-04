import { useCallback, useEffect, useState } from 'react'
import { api, errorMessage } from '@shared/api'
import {
  DELETE_CONFIRMATION,
  type DeleteRequest,
  type DiagnosticsView,
  type ExportKind
} from '@shared/types/data'
import { copyText } from '../worklog-report'

function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} Б`
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} КБ`
  return `${(bytes / 1024 / 1024).toFixed(1)} МБ`
}

function diagnosticsText(d: DiagnosticsView): string {
  return [
    `DevLog ${d.appVersion}`,
    `Схема БД: ${d.schemaVersion}/${d.latestSchemaVersion}, размер ${formatBytes(d.dbSizeBytes)}`,
    `Целостность SQLite: ${d.integrityError ?? 'ok'}`,
    `Старт: копия перед миграцией=${d.startup.preMigrationBackup ?? 'нет'}, прервано отправок=${d.startup.interruptedSubmissions}`,
    ...d.issues.map((i) => `[${i.kind}] ${i.message}`),
    '',
    '--- лог (секреты и e-mail вырезаны) ---',
    ...d.logTail
  ].join('\n')
}

export function DiagnosticsSection(): JSX.Element {
  const [diag, setDiag] = useState<DiagnosticsView | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [notice, setNotice] = useState<string | null>(null)
  const [showLog, setShowLog] = useState(false)

  const reload = useCallback(() => {
    api
      .diagnosticsGet()
      .then((d) => {
        setDiag(d)
        setError(null)
      })
      .catch((err: unknown) => setError(errorMessage(err)))
  }, [])

  useEffect(reload, [reload])

  async function copy(): Promise<void> {
    if (!diag) return
    try {
      await copyText(diagnosticsText(diag))
      setNotice('Диагностика скопирована.')
      setTimeout(() => setNotice(null), 2000)
    } catch (err) {
      setError(errorMessage(err))
    }
  }

  return (
    <section className="card">
      <h2>Диагностика</h2>
      {error && <p className="error">{error}</p>}
      {notice && <p className="muted">{notice}</p>}
      {diag && (
        <>
          <dl>
            <dt>Версия</dt>
            <dd>{diag.appVersion}</dd>
            <dt>База данных</dt>
            <dd>
              <code>{diag.dbPath}</code> · {formatBytes(diag.dbSizeBytes)} · схема {diag.schemaVersion}
            </dd>
            <dt>Лог</dt>
            <dd>{diag.logPath ? <code>{diag.logPath}</code> : 'недоступен'}</dd>
            <dt>Целостность</dt>
            <dd className={diag.integrityError ? 'error' : ''}>{diag.integrityError ?? 'SQLite: ok'}</dd>
          </dl>
          {diag.startup.preMigrationBackup && (
            <p className="muted">
              Перед обновлением схемы сделана копия: <code>{diag.startup.preMigrationBackup}</code>
            </p>
          )}
          {diag.startup.corruptCopy && (
            <p className="error">
              При запуске база не прошла проверку. Копия исходного файла сохранена: <code>{diag.startup.corruptCopy}</code>
            </p>
          )}
          {diag.startup.interruptedSubmissions > 0 && (
            <p className="muted">
              При запуске найдено {diag.startup.interruptedSubmissions} отправок в Jira, прерванных закрытием приложения, —
              проверьте их во вкладке Worklog.
            </p>
          )}
          {diag.issues.length === 0 ? (
            <p className="muted">Проблем в данных не найдено.</p>
          ) : (
            <ul className="diagnostics-issues">
              {diag.issues.map((i, idx) => (
                <li key={`${i.kind}-${idx}`}>{i.message}</li>
              ))}
            </ul>
          )}
          <div className="settings-add-row">
            <button type="button" onClick={reload}>
              Проверить снова
            </button>
            <button type="button" onClick={() => setShowLog((v) => !v)}>
              {showLog ? 'Скрыть лог' : 'Показать лог'}
            </button>
            <button type="button" onClick={() => void copy()}>
              Скопировать диагностику
            </button>
          </div>
          {showLog && <pre className="diagnostics-log">{diag.logTail.join('\n') || 'Лог пуст.'}</pre>}
        </>
      )}
    </section>
  )
}

type DeleteChoice = 'before' | 'allHistory' | 'everything'

function daysAgo(days: number): string {
  const d = new Date()
  d.setDate(d.getDate() - days)
  const pad = (n: number): string => n.toString().padStart(2, '0')
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`
}

export function DataSection(): JSX.Element {
  const [error, setError] = useState<string | null>(null)
  const [notice, setNotice] = useState<string | null>(null)
  const [busy, setBusy] = useState(false)
  const [choice, setChoice] = useState<DeleteChoice>('before')
  const [beforeDate, setBeforeDate] = useState(daysAgo(90))
  const [confirmation, setConfirmation] = useState('')

  async function exportData(kind: ExportKind): Promise<void> {
    setBusy(true)
    try {
      const path = await api.dataExport(kind)
      setNotice(path ? `Сохранено: ${path}` : null)
      setError(null)
    } catch (err) {
      setError(errorMessage(err))
    } finally {
      setBusy(false)
    }
  }

  async function remove(): Promise<void> {
    const request: DeleteRequest = choice === 'before' ? { kind: 'before', localDate: beforeDate } : { kind: choice }
    setBusy(true)
    try {
      const s = await api.dataDelete(request, confirmation)
      setNotice(`Удалено: сессий ${s.workSessions}, событий ${s.activityEvents}, черновиков ${s.worklogDrafts}.`)
      setConfirmation('')
      setError(null)
    } catch (err) {
      setError(errorMessage(err))
    } finally {
      setBusy(false)
    }
  }

  return (
    <section className="card">
      <h2>Данные</h2>
      <p className="muted">
        Всё хранится только на этом компьютере. Экспорт и резервная копия содержат историю работы (задачи, время,
        имена программ) — храните их так же аккуратно, как рабочие документы. Токен Jira в них не попадает.
      </p>
      {error && <p className="error">{error}</p>}
      {notice && <p className="muted">{notice}</p>}
      <div className="settings-add-row">
        <button type="button" disabled={busy} onClick={() => void exportData('json')}>
          Экспорт JSON
        </button>
        <button type="button" disabled={busy} onClick={() => void exportData('csv')}>
          Сессии в CSV
        </button>
        <button type="button" disabled={busy} onClick={() => void exportData('backup')}>
          Резервная копия БД
        </button>
      </div>

      <h2 className="data-delete-title">Удаление</h2>
      <div className="data-delete">
        <label>
          <input type="radio" checked={choice === 'before'} onChange={() => setChoice('before')} /> История раньше{' '}
          <input type="date" value={beforeDate} onChange={(e) => setBeforeDate(e.target.value)} disabled={choice !== 'before'} />
        </label>
        <label>
          <input type="radio" checked={choice === 'allHistory'} onChange={() => setChoice('allHistory')} /> Вся история
          (настройки и репозитории останутся)
        </label>
        <label>
          <input type="radio" checked={choice === 'everything'} onChange={() => setChoice('everything')} /> Всё, включая
          настройки, репозитории и токен Jira
        </label>
      </div>
      <div className="settings-add-row">
        <input
          placeholder={`Введите ${DELETE_CONFIRMATION}`}
          value={confirmation}
          onChange={(e) => setConfirmation(e.target.value)}
        />
        <button
          type="button"
          className="danger"
          disabled={busy || confirmation.trim() !== DELETE_CONFIRMATION || (choice === 'before' && !beforeDate)}
          onClick={() => void remove()}
        >
          Удалить безвозвратно
        </button>
      </div>
      <p className="muted">Удалённое стирается из файла базы физически, восстановить его нельзя (только из своей копии).</p>
    </section>
  )
}
