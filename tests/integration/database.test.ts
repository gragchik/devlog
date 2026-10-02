import { randomUUID } from 'node:crypto'
import { mkdtempSync, rmSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { afterEach, describe, expect, it } from 'vitest'
import { createDatabase } from '../../src/main/db/database'
import { ProjectsRepository } from '../../src/main/db/repositories/projects-repository'

describe('createDatabase / миграции / персистентность', () => {
  let dir: string

  afterEach(() => {
    if (dir) rmSync(dir, { recursive: true, force: true })
  })

  it('применяет миграции и создаёт все ожидаемые таблицы', () => {
    dir = mkdtempSync(join(tmpdir(), 'devlog-test-'))
    const db = createDatabase(join(dir, 'db.sqlite3'))

    const tables = db
      .prepare("SELECT name FROM sqlite_master WHERE type = 'table' ORDER BY name")
      .all()
      .map((row) => (row as { name: string }).name)

    expect(tables).toEqual([
      '_migrations',
      'activity_events',
      'projects',
      'session_edits',
      'settings',
      'work_sessions',
      'worklog_drafts'
    ])
    db.close()
  })

  it('включает WAL-режим', () => {
    dir = mkdtempSync(join(tmpdir(), 'devlog-test-'))
    const db = createDatabase(join(dir, 'db.sqlite3'))
    const mode = db.pragma('journal_mode', { simple: true })
    expect(mode).toBe('wal')
    db.close()
  })

  it('повторное открытие не переприменяет миграции и не теряет данные (перезапуск приложения)', () => {
    dir = mkdtempSync(join(tmpdir(), 'devlog-test-'))
    const filePath = join(dir, `${randomUUID()}.sqlite3`)

    const db1 = createDatabase(filePath)
    const projects1 = new ProjectsRepository(db1)
    const created = projects1.create({ name: 'DevLog', repoPath: 'D:/soft/worklog' })
    db1.close()

    // "Перезапуск" — новое соединение на том же файле, как после рестарта приложения.
    const db2 = createDatabase(filePath)
    const appliedMigrations = db2.prepare('SELECT COUNT(*) as n FROM _migrations').get() as { n: number }
    expect(appliedMigrations.n).toBe(1) // миграция не применилась повторно

    const projects2 = new ProjectsRepository(db2)
    const reloaded = projects2.getById(created.id)
    expect(reloaded).toEqual(created)
    db2.close()
  })

  it('CHECK-constraint в work_sessions отвергает endedAtUtc <= startedAtUtc на уровне самого SQLite', () => {
    dir = mkdtempSync(join(tmpdir(), 'devlog-test-'))
    const db = createDatabase(join(dir, 'db.sqlite3'))
    expect(() =>
      db
        .prepare(
          `INSERT INTO work_sessions
             (id, startedAtUtc, endedAtUtc, timezoneId, activeSeconds, source, confidence, reviewStatus, isManuallyEdited, createdAtUtc, updatedAtUtc)
           VALUES ('x', 1000, 1000, 'UTC', 0, 'manual', 'high', 'reviewed', 0, 1000, 1000)`
        )
        .run()
    ).toThrow(/CHECK constraint failed/)
    db.close()
  })
})
