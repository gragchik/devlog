use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, SystemTime};

use rusqlite::Connection;
use tauri::{AppHandle, Manager};

use crate::db::app_database::AppDatabase;
use crate::db::ids::now_utc_seconds;
use crate::db::repositories::{activity_events, projects as projects_repo};
use crate::domain::activity_event::CreateActivityEventInput;
use crate::domain::confidence::Confidence;
use crate::domain::project::Project;
use crate::domain::tracking_status::TrackingStatus;
use crate::platform::windows_activity_adapter::{get_foreground_process_name, get_system_idle_seconds, is_session_locked};
use crate::tracking::engine::{decide, PollSignals};
use crate::tracking::git_context::GitContextResult;
use crate::tracking::session_engine;
use crate::tracking::whitelist::load_whitelist;
use crate::tracking::{git_adapter, git_context};

/// Session Engine перестраивает сессии не на каждый poll (дорого и не
/// нужно — события копятся быстрее, чем пользователю нужна свежая
/// картина), а раз в столько опросов (~30 секунд при 5-секундном интервале).
const SESSION_ENGINE_RUN_EVERY_N_POLLS: u64 = 6;

/// Целевой интервал опроса (ТЗ FR-02.1: "каждые 5 секунд, конфигурируемо").
/// Настройка через Settings — Итерация 5; константа — временное решение,
/// не перечитывается во время работы.
pub const POLL_INTERVAL_SECONDS: u64 = 5;

/// ТЗ FR-02.5: "idle дольше 180 секунд" закрывает сессию.
pub const IDLE_THRESHOLD_SECONDS: u64 = 180;

/// Управляющий хендл трекера — кладётся в Tauri managed state, доступен
/// и трею (переключение паузы), и (в будущем) IPC-командам/UI. Реальный
/// поток опроса не хранится здесь — он работает в фоне независимо от
/// того, жив ли этот хендл (ТЗ: "логика трекинга не должна зависеть от
/// жизненного цикла окон").
pub struct ActivityTrackerHandle {
    paused: Arc<AtomicBool>,
}

impl ActivityTrackerHandle {
    pub fn is_paused(&self) -> bool {
        self.paused.load(Ordering::SeqCst)
    }

    pub fn set_paused(&self, value: bool) {
        self.paused.store(value, Ordering::SeqCst);
    }

    /// Возвращает новое состояние после переключения — удобно для тray-меню,
    /// которому нужно сразу обновить текст пункта.
    pub fn toggle_paused(&self) -> bool {
        let new_value = !self.is_paused();
        self.set_paused(new_value);
        new_value
    }
}

fn confidence_rank(c: Confidence) -> u8 {
    match c {
        Confidence::High => 2,
        Confidence::Medium => 1,
        Confidence::Low => 0,
    }
}

/// Итоговая уверенность события — это уверенность самого слабого звена в
/// цепочке вывода (состояние трекера И привязка к задаче), а не просто
/// одно из двух значений.
fn weaker_confidence(a: Confidence, b: Confidence) -> Confidence {
    if confidence_rank(a) <= confidence_rank(b) {
        a
    } else {
        b
    }
}

/// FR-03: находит "активный" среди настроенных включённых репозиториев
/// (эвристика по `mtime` `.git/HEAD` — `git_context::pick_most_recently_active`),
/// читает его текущую ветку и резолвит issue key по приоритету
/// (FR-03.4). Реальные git/fs вызовы — тонкие, не тестируются напрямую
/// (см. `git_adapter`); вся логика приоритета — в `git_context::resolve`,
/// покрыта unit-тестами отдельно.
fn resolve_git_context(conn: &Connection) -> GitContextResult {
    let projects = match projects_repo::list(conn) {
        Ok(list) => list,
        Err(err) => {
            eprintln!("[tracker] failed to list projects: {err}");
            Vec::new()
        }
    };

    let enabled: Vec<&Project> = projects.iter().filter(|p| p.enabled).collect();
    if enabled.is_empty() {
        return git_context::resolve(None, None, None);
    }

    let candidates: Vec<(&Project, Option<SystemTime>)> =
        enabled.iter().map(|p| (*p, git_adapter::head_mtime(Path::new(&p.repo_path)))).collect();
    let active = git_context::pick_most_recently_active(&candidates);
    let status = active.map(|p| git_adapter::get_current_branch(Path::new(&p.repo_path)));

    git_context::resolve(None, active, status.as_ref())
}

/// Запускает фоновый поток опроса и возвращает хендл управления им.
/// Каждый poll — это ровно один алгоритм из ТЗ раздела 4: собрать сигналы,
/// решить состояние (`tracking::engine::decide`), при активной работе —
/// дополнительно резолвить git-контекст (FR-03), записать `ActivityEvent`.
/// Поток переживает закрытие любых окон — завершается только вместе с
/// процессом (demon-поток: нет явного join/shutdown, т.к. Tauri сам убивает
/// все потоки при завершении процесса; корректность данных обеспечивает
/// WAL, а не graceful shutdown этого потока).
pub fn start(app: &AppHandle) -> ActivityTrackerHandle {
    let paused = Arc::new(AtomicBool::new(false));
    let handle = ActivityTrackerHandle { paused: Arc::clone(&paused) };

    let app_handle = app.clone();
    thread::spawn(move || {
        // Whitelist читается один раз при старте потока, не на каждый poll
        // (настройки меняются редко; перечитывание тоже не нужно до
        // появления Settings UI в Итерации 5, которая сможет перезапускать
        // трекер при изменении).
        let whitelist = {
            let db = app_handle.state::<AppDatabase>();
            let conn = db.0.lock().expect("AppDatabase mutex poisoned");
            load_whitelist(&conn)
        };

        let mut last_poll_at = now_utc_seconds();
        let mut poll_count: u64 = 0;
        loop {
            thread::sleep(Duration::from_secs(POLL_INTERVAL_SECONDS));

            let now = now_utc_seconds();
            let seconds_since_last_poll = now - last_poll_at;
            last_poll_at = now;

            let foreground = get_foreground_process_name();
            let idle_seconds = get_system_idle_seconds();
            let is_locked = is_session_locked(foreground.as_deref());
            let is_paused = paused.load(Ordering::SeqCst);

            let signals = PollSignals {
                seconds_since_last_poll,
                expected_poll_interval_seconds: POLL_INTERVAL_SECONDS as i64,
                idle_seconds,
                idle_threshold_seconds: IDLE_THRESHOLD_SECONDS,
                is_locked,
                is_paused,
                foreground_process_name: foreground,
            };
            let decided = decide(&signals, &whitelist);

            let db = app_handle.state::<AppDatabase>();
            let mut conn = db.0.lock().expect("AppDatabase mutex poisoned");

            // Git-контекст имеет смысл резолвить только когда пользователь
            // реально работает (TRACKING) — во всех остальных состояниях
            // (idle/paused/locked/suspended/unknown) задача заведомо не
            // определяется этим event'ом.
            let (project_id, branch, detected_issue_key, confidence, reason) =
                if decided.state == TrackingStatus::Tracking {
                    let git = resolve_git_context(&conn);
                    (
                        git.project_id,
                        git.branch,
                        git.issue_key,
                        weaker_confidence(decided.confidence, git.confidence),
                        format!("{}; {}", decided.reason, git.reason),
                    )
                } else {
                    (None, None, None, decided.confidence, decided.reason)
                };

            let input = CreateActivityEventInput {
                timestamp_utc: now,
                process_name_sanitized: decided.process_name_sanitized,
                app_category: decided.app_category,
                project_id,
                branch,
                detected_issue_key,
                idle_seconds: decided.idle_seconds,
                state: decided.state,
                confidence,
                reason,
            };
            if let Err(err) = activity_events::insert(&conn, &input) {
                eprintln!("[tracker] failed to insert activity_event: {err}");
            }

            poll_count += 1;
            if poll_count % SESSION_ENGINE_RUN_EVERY_N_POLLS == 0 {
                let (day_start, day_end) = session_engine::today_local_range_utc();
                if let Err(err) =
                    session_engine::rebuild_detected_sessions_in_range(&mut conn, day_start, day_end, POLL_INTERVAL_SECONDS as i64)
                {
                    eprintln!("[tracker] session engine rebuild failed: {err}");
                }
            }
        }
    });

    handle
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn weaker_confidence_picks_the_lower_one_either_order() {
        assert_eq!(weaker_confidence(Confidence::High, Confidence::Low), Confidence::Low);
        assert_eq!(weaker_confidence(Confidence::Low, Confidence::High), Confidence::Low);
        assert_eq!(weaker_confidence(Confidence::High, Confidence::Medium), Confidence::Medium);
        assert_eq!(weaker_confidence(Confidence::High, Confidence::High), Confidence::High);
    }
}
