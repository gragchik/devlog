use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;
use tauri::{AppHandle, Manager};

use crate::db::app_database::AppDatabase;
use crate::db::ids::now_utc_seconds;
use crate::db::repositories::activity_events;
use crate::domain::activity_event::CreateActivityEventInput;
use crate::platform::windows_activity_adapter::{get_foreground_process_name, get_system_idle_seconds, is_session_locked};
use crate::tracking::engine::{decide, PollSignals};
use crate::tracking::whitelist::load_whitelist;

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

/// Запускает фоновый поток опроса и возвращает хендл управления им.
/// Каждый poll — это ровно один алгоритм из ТЗ раздела 4: собрать сигналы,
/// решить состояние (`tracking::engine::decide`), записать `ActivityEvent`.
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
            let conn = db.0.lock().expect("AppDatabase mutex poisoned");
            let input = CreateActivityEventInput {
                timestamp_utc: now,
                process_name_sanitized: decided.process_name_sanitized,
                app_category: decided.app_category,
                project_id: None,
                branch: None,
                detected_issue_key: None,
                idle_seconds: decided.idle_seconds,
                state: decided.state,
                confidence: decided.confidence,
                reason: decided.reason,
            };
            if let Err(err) = activity_events::insert(&conn, &input) {
                eprintln!("[tracker] failed to insert activity_event: {err}");
            }
        }
    });

    handle
}
