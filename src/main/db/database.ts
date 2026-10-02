import Database from 'better-sqlite3'
import { runMigrations } from './migrate'

/**
 * Открывает (или создаёт) файл SQLite, включает WAL и внешние ключи,
 * применяет миграции. Единая точка создания соединения — используется и
 * настоящим приложением (`app-database.ts`, путь внутри `userData`), и
 * тестами (временный файл на диске или `:memory:`).
 *
 * WAL даёт устойчивость к падению процесса посреди записи (НФТ "atomic
 * writes, recovery при crash") — SQLite восстанавливает consistent-состояние
 * из WAL-журнала при следующем открытии файла, без ручного кода.
 */
export function createDatabase(filePath: string): Database.Database {
  const db = new Database(filePath)
  db.pragma('journal_mode = WAL')
  db.pragma('foreign_keys = ON')
  runMigrations(db)
  return db
}
