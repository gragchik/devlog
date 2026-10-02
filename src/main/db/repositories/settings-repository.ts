import type Database from 'better-sqlite3'

/**
 * Простое key-value хранилище настроек (раздел 5: "settings: key, value").
 * Чувствительные значения (Jira-токены и т.п.) здесь НЕ хранятся — для них
 * предусмотрено отдельное зашифрованное хранилище через `safeStorage`
 * (Итерация 7/9), а не эта таблица.
 */
export class SettingsRepository {
  constructor(private readonly db: Database.Database) {}

  get(key: string): string | null {
    const row = this.db.prepare('SELECT value FROM settings WHERE key = ?').get(key) as
      | { value: string }
      | undefined
    return row?.value ?? null
  }

  set(key: string, value: string): void {
    this.db
      .prepare('INSERT INTO settings (key, value) VALUES (?, ?) ON CONFLICT(key) DO UPDATE SET value = excluded.value')
      .run(key, value)
  }

  getAll(): Record<string, string> {
    const rows = this.db.prepare('SELECT key, value FROM settings').all() as { key: string; value: string }[]
    return Object.fromEntries(rows.map((row) => [row.key, row.value]))
  }

  remove(key: string): void {
    this.db.prepare('DELETE FROM settings WHERE key = ?').run(key)
  }
}
