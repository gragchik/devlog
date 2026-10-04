use rusqlite::Connection;
use std::sync::{Mutex, MutexGuard};
use tauri::{AppHandle, Manager};

use super::database::create_database;
use super::ids::now_utc_seconds;
use super::maintenance;
use super::repositories::{jira_submissions, session_edits};
use crate::logging;

const SESSION_EDITS_RETENTION_DAYS: i64 = 30;
const SECONDS_PER_DAY: i64 = 86_400;

/// Единственный экземпляр БД приложения — управляется Tauri как managed
/// state (`app.manage(...)`), доступен командам через `tauri::State`.
/// Main window и overlay работают через один и тот же процесс и, значит,
/// через один и тот же `AppDatabase` — отдельного хранилища для overlay
/// нет и не будет (ТЗ: "Все изменения из overlay и основного окна
/// выполняются через один сервис"). `Mutex`, а не `RwLock` — rusqlite
/// `Connection` не `Sync`, нужен эксклюзивный доступ на каждый вызов.
pub struct AppDatabase(pub Mutex<Connection>);

impl AppDatabase {
    /// Захват соединения, переживающий "отравление" мьютекса. Паника в
    /// одной команде не должна навсегда выключать трекер и все остальные
    /// команды до перезапуска: незавершённая rusqlite-транзакция при панике
    /// откатывается в `Drop`, а одиночные SQL-операторы атомарны, так что
    /// соединение после паники в согласованном состоянии.
    pub fn lock(&self) -> MutexGuard<'_, Connection> {
        lock_recovering(&self.0)
    }
}

pub fn lock_recovering(mutex: &Mutex<Connection>) -> MutexGuard<'_, Connection> {
    mutex.lock().unwrap_or_else(|poisoned| {
        crate::logging::warn("db", "мьютекс БД был отравлен паникой — соединение восстановлено");
        mutex.clear_poison();
        poisoned.into_inner()
    })
}

/// Что произошло с БД при старте — показывается на экране диагностики.
#[derive(Debug, Clone, Default, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StartupReport {
    pub db_path: String,
    /// Копия перед применением новых миграций (обновление приложения).
    pub pre_migration_backup: Option<String>,
    /// `None` — `quick_check` прошёл; иначе текст ошибки SQLite.
    pub integrity_error: Option<String>,
    /// Байт-в-байт копия файла, не прошедшего проверку.
    pub corrupt_copy: Option<String>,
    /// Отправки в Jira, прерванные падением процесса (стали `unknown`).
    pub interrupted_submissions: usize,
}

/// Открывает (или создаёт) БД по реальному пути приложения
/// (`app.path().app_data_dir()`, аналог Electron `app.getPath('userData')`)
/// и выполняет стартовое обслуживание:
/// - копия перед миграциями, если схема устарела;
/// - `PRAGMA quick_check`; при повреждении — копия файла рядом, работа
///   продолжается (SQLite часто может читать/писать неповреждённые части);
/// - чистка `session_edits` старше 30 дней (FR-04.8);
/// - прерванные POST в Jira → `unknown` (FR-07.6).
pub fn init(app: &AppHandle) -> anyhow::Result<(AppDatabase, StartupReport)> {
    let dir = app.path().app_data_dir()?;
    std::fs::create_dir_all(&dir)?;
    let db_path = dir.join("devlog.sqlite3");
    let mut report = StartupReport { db_path: db_path.display().to_string(), ..Default::default() };

    match maintenance::backup_before_migrations(&db_path, &dir.join("backups")) {
        Ok(Some(path)) => {
            logging::info("db", format!("копия перед миграцией: {}", path.display()));
            report.pre_migration_backup = Some(path.display().to_string());
        }
        Ok(None) => {}
        // Не блокируем запуск: без трекинга пользователь потеряет больше,
        // чем без копии. Но в лог и на экран диагностики — обязательно.
        Err(err) => logging::error("db", format!("не удалось сделать копию перед миграцией: {err}")),
    }

    let conn = create_database(&db_path)?;

    if let Err(err) = maintenance::sqlite_quick_check(&conn) {
        logging::error("db", format!("quick_check не пройден: {err}"));
        report.integrity_error = Some(err);
        match maintenance::preserve_corrupt_copy(&db_path) {
            Ok(copy) => report.corrupt_copy = Some(copy.display().to_string()),
            Err(e) => logging::error("db", format!("не удалось сохранить копию повреждённой БД: {e}")),
        }
    }

    session_edits::prune_older_than(&conn, now_utc_seconds() - SESSION_EDITS_RETENTION_DAYS * SECONDS_PER_DAY)?;
    report.interrupted_submissions = jira_submissions::mark_stale_pending_as_unknown(&conn)?;
    if report.interrupted_submissions > 0 {
        logging::warn("jira", format!("{} отправок прервано завершением процесса — требуется проверка", report.interrupted_submissions));
    }

    Ok((AppDatabase(Mutex::new(conn)), report))
}
