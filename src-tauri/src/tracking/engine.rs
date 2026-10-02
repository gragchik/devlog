//! Чистая логика принятия решения по уже собранным сигналам (ТЗ, раздел 4,
//! алгоритм каждого poll). Никаких Win32/БД вызовов здесь — только это
//! делает возможным осмысленное unit-тестирование состояний
//! TRACKING/IDLE/PAUSED/LOCKED/SUSPENDED/UNKNOWN и их приоритета.
//! Фактический сбор сигналов — `crate::platform::windows_activity_adapter`
//! (не тестируется unit-тестами — см. комментарий там).

use crate::domain::confidence::Confidence;
use crate::domain::tracking_status::TrackingStatus;
use crate::tracking::whitelist::{find_category, WhitelistEntry};

/// Всё, что движку нужно знать, чтобы принять решение за один poll.
#[derive(Debug, Clone)]
pub struct PollSignals {
    pub seconds_since_last_poll: i64,
    pub expected_poll_interval_seconds: i64,
    pub idle_seconds: Option<u64>,
    pub idle_threshold_seconds: u64,
    pub is_locked: bool,
    pub is_paused: bool,
    pub foreground_process_name: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DecidedEvent {
    pub state: TrackingStatus,
    pub app_category: String,
    /// Редактируется: для не-whitelisted приложений реальное имя процесса
    /// НЕ сохраняется (FR-02.3: "без чувствительных данных").
    pub process_name_sanitized: String,
    pub confidence: Confidence,
    pub reason: String,
    pub idle_seconds: Option<i64>,
}

const EXCLUDED_CATEGORY: &str = "excluded";
const EXCLUDED_PROCESS_PLACEHOLDER: &str = "excluded";

/// Множитель над ожидаемым интервалом опроса, после которого разрыв между
/// двумя poll считается пропущенным временем (сон/перегрузка системы), а
/// не просто дрожанием таймера — ТЗ раздел 4, п.6: "При пропуске нескольких
/// poll... не заполнять пробел как рабочее время автоматически".
pub const SUSPEND_GAP_MULTIPLIER: i64 = 3;

/// Приоритет (сверху вниз, первое совпадение побеждает):
/// `is_paused` (ручная пауза — самый осознанный сигнал) → пропуск poll
/// похожий на сон → заблокированный экран → превышен idle-порог →
/// foreground-detector не смог определить процесс (`UNKNOWN`) → проверка
/// whitelist.
pub fn decide(signals: &PollSignals, whitelist: &[WhitelistEntry]) -> DecidedEvent {
    let idle_seconds_i64 = signals.idle_seconds.map(|s| s as i64);

    if signals.is_paused {
        return DecidedEvent {
            state: TrackingStatus::Paused,
            app_category: EXCLUDED_CATEGORY.to_string(),
            process_name_sanitized: EXCLUDED_PROCESS_PLACEHOLDER.to_string(),
            confidence: Confidence::High,
            reason: "manual pause".to_string(),
            idle_seconds: idle_seconds_i64,
        };
    }

    let suspend_gap_threshold = signals.expected_poll_interval_seconds * SUSPEND_GAP_MULTIPLIER;
    if signals.seconds_since_last_poll > suspend_gap_threshold {
        return DecidedEvent {
            state: TrackingStatus::Suspended,
            app_category: EXCLUDED_CATEGORY.to_string(),
            process_name_sanitized: EXCLUDED_PROCESS_PLACEHOLDER.to_string(),
            confidence: Confidence::High,
            reason: format!(
                "gap {}s since last poll exceeds threshold {}s — suspected sleep/suspend",
                signals.seconds_since_last_poll, suspend_gap_threshold
            ),
            idle_seconds: idle_seconds_i64,
        };
    }

    if signals.is_locked {
        return DecidedEvent {
            state: TrackingStatus::Locked,
            app_category: EXCLUDED_CATEGORY.to_string(),
            process_name_sanitized: EXCLUDED_PROCESS_PLACEHOLDER.to_string(),
            confidence: Confidence::High,
            reason: "session locked (input desktop is not Default)".to_string(),
            idle_seconds: idle_seconds_i64,
        };
    }

    if let Some(idle) = signals.idle_seconds {
        if idle >= signals.idle_threshold_seconds {
            return DecidedEvent {
                state: TrackingStatus::Idle,
                app_category: EXCLUDED_CATEGORY.to_string(),
                process_name_sanitized: EXCLUDED_PROCESS_PLACEHOLDER.to_string(),
                confidence: Confidence::High,
                reason: format!("idle {idle}s >= threshold {}s", signals.idle_threshold_seconds),
                idle_seconds: idle_seconds_i64,
            };
        }
    }

    let Some(process_name) = &signals.foreground_process_name else {
        return DecidedEvent {
            state: TrackingStatus::Unknown,
            app_category: EXCLUDED_CATEGORY.to_string(),
            process_name_sanitized: EXCLUDED_PROCESS_PLACEHOLDER.to_string(),
            confidence: Confidence::Low,
            reason: "foreground process detection failed".to_string(),
            idle_seconds: idle_seconds_i64,
        };
    };

    match find_category(process_name, whitelist) {
        Some(entry) => DecidedEvent {
            state: TrackingStatus::Tracking,
            app_category: entry.category.clone(),
            process_name_sanitized: process_name.clone(),
            confidence: Confidence::High,
            reason: format!("whitelisted process ({})", entry.category),
            idle_seconds: idle_seconds_i64,
        },
        None => DecidedEvent {
            state: TrackingStatus::Tracking,
            app_category: EXCLUDED_CATEGORY.to_string(),
            process_name_sanitized: EXCLUDED_PROCESS_PLACEHOLDER.to_string(),
            confidence: Confidence::Low,
            reason: "foreground process not in whitelist".to_string(),
            idle_seconds: idle_seconds_i64,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tracking::whitelist::default_whitelist;

    fn base_signals() -> PollSignals {
        PollSignals {
            seconds_since_last_poll: 5,
            expected_poll_interval_seconds: 5,
            idle_seconds: Some(0),
            idle_threshold_seconds: 180,
            is_locked: false,
            is_paused: false,
            foreground_process_name: Some("webstorm64.exe".to_string()),
        }
    }

    #[test]
    fn whitelisted_foreground_app_is_tracking() {
        let decided = decide(&base_signals(), &default_whitelist());
        assert_eq!(decided.state, TrackingStatus::Tracking);
        assert_eq!(decided.app_category, "ide");
        assert_eq!(decided.process_name_sanitized, "webstorm64.exe");
    }

    #[test]
    fn non_whitelisted_app_is_tracking_but_excluded_category_and_redacted_name() {
        let mut signals = base_signals();
        signals.foreground_process_name = Some("some-random-game.exe".to_string());
        let decided = decide(&signals, &default_whitelist());
        assert_eq!(decided.state, TrackingStatus::Tracking);
        assert_eq!(decided.app_category, "excluded");
        assert_eq!(decided.process_name_sanitized, "excluded"); // реальное имя не сохранено
    }

    #[test]
    fn idle_above_threshold_wins_over_whitelisted_app() {
        let mut signals = base_signals();
        signals.idle_seconds = Some(200);
        let decided = decide(&signals, &default_whitelist());
        assert_eq!(decided.state, TrackingStatus::Idle);
    }

    #[test]
    fn idle_below_threshold_does_not_trigger_idle_state() {
        let mut signals = base_signals();
        signals.idle_seconds = Some(179);
        signals.idle_threshold_seconds = 180;
        let decided = decide(&signals, &default_whitelist());
        assert_eq!(decided.state, TrackingStatus::Tracking);
    }

    #[test]
    fn locked_wins_over_idle_and_whitelisted_app() {
        let mut signals = base_signals();
        signals.is_locked = true;
        signals.idle_seconds = Some(0); // даже не idle — но locked всё равно должен победить
        let decided = decide(&signals, &default_whitelist());
        assert_eq!(decided.state, TrackingStatus::Locked);
    }

    #[test]
    fn manual_pause_wins_over_everything_else() {
        let mut signals = base_signals();
        signals.is_paused = true;
        signals.is_locked = true;
        signals.idle_seconds = Some(999);
        let decided = decide(&signals, &default_whitelist());
        assert_eq!(decided.state, TrackingStatus::Paused);
    }

    #[test]
    fn large_gap_since_last_poll_is_suspected_suspend_not_counted_as_tracking() {
        let mut signals = base_signals();
        signals.expected_poll_interval_seconds = 5;
        signals.seconds_since_last_poll = 5 * SUSPEND_GAP_MULTIPLIER + 1; // чуть больше порога
        let decided = decide(&signals, &default_whitelist());
        assert_eq!(decided.state, TrackingStatus::Suspended);
    }

    #[test]
    fn gap_just_under_threshold_is_not_treated_as_suspend() {
        let mut signals = base_signals();
        signals.expected_poll_interval_seconds = 5;
        signals.seconds_since_last_poll = 5 * SUSPEND_GAP_MULTIPLIER; // ровно порог — не превышен
        let decided = decide(&signals, &default_whitelist());
        assert_eq!(decided.state, TrackingStatus::Tracking);
    }

    #[test]
    fn missing_foreground_process_is_unknown_not_fabricated() {
        let mut signals = base_signals();
        signals.foreground_process_name = None;
        let decided = decide(&signals, &default_whitelist());
        assert_eq!(decided.state, TrackingStatus::Unknown);
        assert_eq!(decided.confidence, Confidence::Low);
    }

    #[test]
    fn unknown_detection_failure_does_not_override_manual_pause() {
        // Даже если OS-helper отказал, явная пауза пользователя важнее.
        let mut signals = base_signals();
        signals.is_paused = true;
        signals.foreground_process_name = None;
        let decided = decide(&signals, &default_whitelist());
        assert_eq!(decided.state, TrackingStatus::Paused);
    }
}
