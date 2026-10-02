//! Тонкий слой над Win32 API (крейт `windows` — официальные typed-биндинги
//! Microsoft, без отдельной FFI-библиотеки, в отличие от исходного плана
//! на Electron, где для этого рассматривался `koffi` — ADR-0003, archived).
//!
//! FR-02.2: Electron не даёт API для foreground-окна другого приложения —
//! в Tauri/Rust этой проблемы нет вообще, т.к. мы напрямую зовём Win32.
//!
//! Эти функции намеренно тонкие и не покрыты unit-тестами (нельзя
//! осмысленно мокнуть реальную ОС) — проверяются вручную на целевой машине
//! (см. `docs/iterations/02.md`). Вся тестируемая логика (принятие решений
//! по уже полученным сигналам) — в `crate::tracking::engine`.

use windows::Win32::Foundation::{CloseHandle, HANDLE, HWND};
use windows::Win32::System::StationsAndDesktops::{
    GetUserObjectInformationW, OpenInputDesktop, DESKTOP_CONTROL_FLAGS, DESKTOP_SWITCHDESKTOP, UOI_NAME,
};
use windows::Win32::System::SystemInformation::GetTickCount64;
use windows::Win32::System::Threading::{
    OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{GetLastInputInfo, LASTINPUTINFO};
use windows::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowThreadProcessId};

/// Имя процесса (например, `webstorm64.exe`) активного (foreground) окна —
/// чьё бы оно ни было, чужого приложения в том числе (FR-02.2). `None`,
/// если не удалось определить (нет foreground-окна, нет прав на
/// `OpenProcess`, и т.п.) — вызывающий код обязан трактовать это как
/// `UNKNOWN`, не выдумывать активность (FR-02.6).
pub fn get_foreground_process_name() -> Option<String> {
    unsafe {
        let hwnd: HWND = GetForegroundWindow();
        if hwnd.is_invalid() {
            return None;
        }

        let mut pid: u32 = 0;
        if GetWindowThreadProcessId(hwnd, Some(&mut pid)) == 0 || pid == 0 {
            return None;
        }

        let process: HANDLE = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
        let name = query_full_process_image_name(process);
        let _ = CloseHandle(process);
        name.and_then(|full_path| {
            full_path
                .rsplit(['\\', '/'])
                .next()
                .map(str::to_string)
                .filter(|s| !s.is_empty())
        })
    }
}

/// # Safety
/// `process` должен быть валидным открытым хендлом процесса.
unsafe fn query_full_process_image_name(process: HANDLE) -> Option<String> {
    let mut buffer = [0u16; 1024];
    let mut size = buffer.len() as u32;
    QueryFullProcessImageNameW(process, PROCESS_NAME_WIN32, windows::core::PWSTR(buffer.as_mut_ptr()), &mut size)
        .ok()?;
    Some(String::from_utf16_lossy(&buffer[..size as usize]))
}

/// Сколько секунд прошло с последнего ввода (клавиатура/мышь) по системе в
/// целом, не только по нашему приложению. `None` при ошибке API —
/// трактовать как неизвестный idle (FR-02.6).
pub fn get_system_idle_seconds() -> Option<u64> {
    unsafe {
        let mut info = LASTINPUTINFO { cbSize: std::mem::size_of::<LASTINPUTINFO>() as u32, dwTime: 0 };
        if GetLastInputInfo(&mut info).as_bool() {
            let now_ticks = GetTickCount64();
            // dwTime — 32-битный tick count на момент последнего ввода;
            // GetTickCount64 — 64-битный текущий. Разница корректна, пока
            // аптайм между последним вводом и сейчас не превышает ~49.7
            // дней (переполнение 32-битного счётчика) — приемлемо для idle
            // threshold в минутах/часах.
            let last_input_ticks = info.dwTime as u64;
            let now_low32 = now_ticks & 0xFFFF_FFFF;
            let elapsed_ms = now_low32.wrapping_sub(last_input_ticks);
            Some(elapsed_ms / 1000)
        } else {
            None
        }
    }
}

/// Процессы современного экрана блокировки/входа в Windows. **Важное
/// открытие при ручной проверке на реальной машине:** с Windows 8.1+
/// экран блокировки (тот, что со свайпом/фоновым фото) рендерится UWP-
/// приложением `LockApp.exe` поверх обычного "Default" input desktop —
/// переключение на настоящий защищённый Winlogon-десктоп происходит
/// только когда пользователь начинает вводить пароль. Поэтому проверка
/// `OpenInputDesktop`/`GetUserObjectInformationW` в одиночку **пропускает**
/// сам факт блокировки (desktop всё ещё "Default", пока не начат ввод
/// пароля) — нужна обе проверки вместе.
const LOCK_OR_LOGON_PROCESSES: &[&str] = &["LockApp.exe", "LogonUI.exe"];

/// `true`, если: (а) foreground-процесс — сам экран блокировки/входа, или
/// (б) текущий input desktop не "Default" (настоящий secure desktop —
/// происходит при активном вводе пароля/UAC-промпте). Опрашивается на
/// каждом poll — не требует подписки на WM_WTSSESSION_CHANGE через хук
/// оконного сообщения. `foreground_process_name` передаётся вызывающим
/// кодом, чтобы не делать повторный `GetForegroundWindow` — он уже вызван
/// один раз за poll в `get_foreground_process_name()`.
pub fn is_session_locked(foreground_process_name: Option<&str>) -> bool {
    if let Some(name) = foreground_process_name {
        if LOCK_OR_LOGON_PROCESSES.iter().any(|&p| p.eq_ignore_ascii_case(name)) {
            return true;
        }
    }

    unsafe {
        let Ok(desktop) = OpenInputDesktop(DESKTOP_CONTROL_FLAGS(0), false, DESKTOP_SWITCHDESKTOP) else {
            // Не смогли открыть input desktop — на практике это само по
            // себе случается именно когда рабочий стол заблокирован
            // (другой процесс/сессия владеет input desktop).
            return true;
        };

        let mut buffer = [0u16; 256];
        let mut needed: u32 = 0;
        let ok = GetUserObjectInformationW(
            windows::Win32::Foundation::HANDLE(desktop.0),
            UOI_NAME,
            Some(buffer.as_mut_ptr().cast()),
            (buffer.len() * 2) as u32,
            Some(&mut needed),
        );
        let _ = windows::Win32::System::StationsAndDesktops::CloseDesktop(desktop);

        match ok {
            Ok(()) => {
                let len = buffer.iter().position(|&c| c == 0).unwrap_or(buffer.len());
                let name = String::from_utf16_lossy(&buffer[..len]);
                name != "Default"
            }
            Err(_) => true,
        }
    }
}

#[cfg(test)]
mod manual_smoke_tests {
    //! Эти функции зовут настоящий Win32 API — их нельзя осмысленно
    //! unit-тестировать (нет способа мокнуть ОС без непропорциональных
    //! затрат на эту итерацию). `#[ignore]` — не часть обычного `cargo
    //! test`; запускать вручную на реальной машине:
    //! `cargo test manual_smoke -- --ignored --nocapture`.
    use super::*;

    #[test]
    #[ignore]
    fn prints_current_signals_for_manual_inspection() {
        let foreground = get_foreground_process_name();
        println!("foreground process: {foreground:?}");
        println!("idle seconds: {:?}", get_system_idle_seconds());
        println!("is session locked: {}", is_session_locked(foreground.as_deref()));
    }
}
